import { safeFetch } from "@utils/HttpClient";
import { executeDocumentWriter, DocumentSection, ContentEnricher } from "./DocumentWriterBase";
import { logger } from "@utils/logger";
export const metadata = {
  name: "report_writer",
  search_keywords: ["report", "writer", "báo cáo", "phân tích", "analysis"],
  description:
    "[AUTO_RUN] Business & Academic Report Writer. Use when the user requests 'Viết báo cáo', 'Phân tích số liệu', or 'Tổng hợp nghiên cứu'. Automatically splits the report into 7 standard sections. If an academic report is requested, it automatically cross-checks with Semantic Scholar.",
  parameters: {
    type: "object",
    properties: {
      topic: {
        type: "string",
        description: "[VIETNAMESE] Report topic exactly as user requested. Example: 'Báo cáo doanh thu tháng 4', 'Báo cáo xu hướng AI 2024'.",
      },
      fileLocation: {
        type: "string",
        description: "Output directory for Markdown report. Recommended: E:/Project/LIVA/scratch_workspace"
      },
      providedContext: {
         type: "string",
         description: "[VIETNAMESE] Raw data, numbers, or info provided by user (if any).",
      },
      isAcademic: {
         type: "boolean",
         description: "Set True if this is a Medical, Scientific, or Academic report requiring scholarly citations."
      }
    },
    required: ["topic", "fileLocation"],
  },
};

export const execute = async (args: {
  topic: string;
  fileLocation: string;
  providedContext?: string;
  isAcademic?: boolean;
}): Promise<string> => {
  const enricher: ContentEnricher | undefined = args.isAcademic ? async (currentRawData: string) => {
     let newRawData = currentRawData;
     const semanticKey = process.env.SEMANTIC_SCHOLAR_API_KEY || "";
     try {
       await logger.info(`🔎 [Giáo Sư Học Thuật]: Đây là báo cáo có tính Học Thuật. Đang truy cập Semantic Scholar để lấy 10 bài Abstract siêu uy tín...`);
       const encodedTopic = encodeURIComponent(args.topic);
       const headers: Record<string, string> = {};
       if (semanticKey) {
           headers["x-api-key"] = semanticKey;
       }
       const scholarUrl = `https://api.semanticscholar.org/graph/v1/paper/search?query=${encodedTopic}&limit=10&fields=title,abstract,authors,year,url,citationCount`;
       
       const response = await safeFetch(scholarUrl, { headers }, 10000);
       const data = await response.json() as { data?: { abstract?: string, authors?: { name: string }[], title: string, year: number, url: string }[] };
       if (data?.data && data.data.length > 0) {
          const extractedContext = [];
          for (const paper of data.data) {
             if (!paper.abstract) continue;
             const authors = paper.authors ? paper.authors.map((a: { name: string }) => a.name).join(", ") : "Unknown";
             extractedContext.push(`[Trích Dẫn] Title: ${paper.title} (Năm: ${paper.year})\nTác giả: ${authors}\nURL: ${paper.url}\nAbstract: ${paper.abstract}`);
          }
          newRawData += "\n\n=== TÀI LIỆU KHOA HỌC THAM KHẢO TỪ SEMANTIC SCHOLAR ===\n" + extractedContext.join("\n\n");
          await logger.info(`✅ [Giáo Sư Học Thuật]: Đã thu thập xong ${data.data.length} nghiên cứu! Bắt đầu chắp bút...`);
       }
     } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message : String(err);
         logger.error("Semantic Scholar API Error:" + " " + errMsg);
     }
     return newRawData;
  } : undefined;

  const parts: DocumentSection[] = [
    { name: "Part 1: General Info (Header / Cover Page)", instruction: "Create Report Title, Author (LIVA AI), Recipient (Management), Report Time." },
    { name: "Part 2: Executive Summary", instruction: "Concise summary (200-300 words): What is the core issue? Most notable results? Most important recommendations?" },
    { name: "Part 3: Introduction & Context", instruction: "Reason and context for the report. Goal orientation of this report." },
    { name: "Part 4: Findings & Data", instruction: "Clearly list original data. Encourage using Markdown TABLES. Avoid personal opinions here." },
    { name: "Part 5: Analysis & Evaluation", instruction: "From the above data, what Insights can be drawn? What are the bright spots? What are the weaknesses? Why?" },
    { name: "Part 6: Conclusion & Recommendations", instruction: "Propose specific Next-steps. What to do next?" },
    { name: "Part 7: Appendices & References", instruction: "List of data sources and references." }
  ];

  return executeDocumentWriter({
    title: args.topic,
    workspace: args.fileLocation,
    type: "report",
    systemPrompt: `You are LIVA - Senior Advisor and Analysis Expert. You will write the report CAREFULLY, section by section.
[CRITICAL INSTRUCTION] You MUST use Chain-of-Thought (CoT). First, analyze and plan your section using English inside a <thought> block. Then, output the final report content in fluent VIETNAMESE inside a <report> block.
Example:
<thought>
The data shows a 20% increase in sales. I need to format this in a table.
</thought>
<report>
Dữ liệu cho thấy doanh số đã tăng 20%...
</report>`,
    startMessage: `📝 [ReportWriter]: Bắt đầu tiến trình phân tích đa phần cho báo cáo "${args.topic}". Tiến trình này sẽ làm cực kỳ tỉ mỉ từng chương một!`,
    endMessage: `✅ [Báo Cáo Hoàn Tất]: Báo cáo đã hoàn thành! Mời sếp duyệt file Markdown tại: {absolutePath}`,
    successMessage: "Báo cáo đã xuất bản cực kỳ chi tiết tại: {absolutePath}",
    rawData: args.providedContext || "No raw data provided.",
    parts,
    loggerPrefix: "[ReportWriter]",
    zaloPrefix: "🗓️ [Báo Cáo]",
    enrichContent: enricher
  });
};
