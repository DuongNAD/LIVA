import { safeFetch } from "@utils/HttpClient";
import { executeDocumentWriter, DocumentSection, ContentEnricher } from "./DocumentWriterBase";
import { logger } from "@utils/logger";
export const metadata = {
  name: "report_writer",
  search_keywords: ["report_writer","report writer"],
  description:
    "Kỹ năng Viết Báo Cáo Kinh doanh & Khoa học (Report Writer). Sử dụng khi người dùng yêu cầu 'Viết báo cáo', 'Phân tích số liệu', hoặc 'Tổng hợp nghiên cứu'. Tự động chia báo cáo thành 7 phần chuẩn mực. Nếu được yêu cầu báo cáo học thuật khoa học, nó sẽ tự đối soát với Semantic Scholar.",
  parameters: {
    type: "object",
    properties: {
      topic: {
        type: "string",
        description: "Chủ đề báo cáo. Ví dụ: 'Báo cáo doanh thu tháng 4', 'Báo cáo xu hướng AI 2024'.",
      },
      fileLocation: {
        type: "string",
        description: "Thư mục lưu báo cáo định dạng Markdown. Khuyến nghị: E:/Project/LIVA/scratch_workspace"
      },
      providedContext: {
         type: "string",
         description: "Dữ liệu, con số hoặc thông tin thô do người dùng cung cấp (nếu có).",
      },
      isAcademic: {
         type: "boolean",
         description: "Tích bằng True nếu đây là báo cáo Y khoa, Khoa học hoặc Học thuật cần trích dẫn hàn lâm."
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
       const data = await response.json() as any;
       if (data?.data && data.data.length > 0) {
          const extractedContext = [];
          for (const paper of data.data) {
             if (!paper.abstract) continue;
             const authors = paper.authors ? paper.authors.map((a:any)=>a.name).join(", ") : "Unknown";
             extractedContext.push(`[Trích Dẫn] Title: ${paper.title} (Năm: ${paper.year})\nTác giả: ${authors}\nURL: ${paper.url}\nAbstract: ${paper.abstract}`);
          }
          newRawData += "\n\n=== TÀI LIỆU KHOA HỌC THAM KHẢO TỪ SEMANTIC SCHOLAR ===\n" + extractedContext.join("\n\n");
          await logger.info(`✅ [Giáo Sư Học Thuật]: Đã thu thập xong ${data.data.length} nghiên cứu! Bắt đầu chắp bút...`);
       }
     } catch (err: any) {
         logger.error("Semantic Scholar API Error:", err.message);
     }
     return newRawData;
  } : undefined;

  const parts: DocumentSection[] = [
    { name: "Phần 1: Thông tin chung (Header / Cover Page)", instruction: "Tạo Tiêu đề báo cáo, Người lập (LIVA AI), Người nhận (Ban Lãnh Đạo), Thời gian báo cáo." },
    { name: "Phần 2: Tóm tắt thực thi (Executive Summary)", instruction: "Tóm tắt gọn gàng (khoảng 200-300 chữ): Vấn đề cốt lõi là gì? Kết quả nổi bật nhất? Kiến nghị quan trọng nhất?" },
    { name: "Phần 3: Mở đầu & Bối cảnh (Introduction)", instruction: "Lý do và bối cảnh lập báo cáo. Định hướng mục tiêu của bài báo cáo này." },
    { name: "Phần 4: Dữ liệu & Hiện trạng (Findings / Data)", instruction: "Liệt kê rõ ràng số liệu gốc. Khuyến khích sử dụng BẢNG Markdown. Tránh đưa ra nhận định cá nhân ở phần này." },
    { name: "Phần 5: Phân tích & Đánh giá (Analysis)", instruction: "Từ số liệu trên, rút ra Insight gì? Điểm sáng là gì? Điểm yếu là gì? Nguyên nhân vì sao?" },
    { name: "Phần 6: Kết luận & Kiến nghị (Conclusion & Recommendations)", instruction: "Đề xuất Next-steps cụ thể. Phải làm gì tiếp theo?" },
    { name: "Phần 7: Phụ lục & Trích dẫn (Appendices / References)", instruction: "Danh sách nguồn dữ liệu và tài liệu tham khảo." }
  ];

  return executeDocumentWriter({
    title: args.topic,
    workspace: args.fileLocation,
    type: "report",
    systemPrompt: `Bạn là LIVA - Cố Vấn Cao Cấp và Chuyên gia Phân tích.\nBạn sẽ viết CHẬM RÃI từng Phần của Báo Cáo.`,
    startMessage: `📝 [Chuyên Viên Báo Cáo LIVA]: Bắt đầu tiến trình phân tích Đa phần cho báo cáo "${args.topic}". Tiến trình này sẽ làm cực kỳ tỉ mỉ từng Chương một!`,
    endMessage: `✅ [Báo Cáo Hoàn Tất]: Tuyệt phẩm độ dài ngàn chữ đã ra lò! Mời sếp duyệt file Markdown tại: {absolutePath}`,
    successMessage: "Báo cáo đã xuất bản cực kỳ chi tiết tại: {absolutePath}",
    rawData: args.providedContext || "Không có dữ liệu số liệu thô tự cung cấp.",
    parts,
    loggerPrefix: "[ReportWriter]",
    zaloPrefix: "🗓️ [Báo Cáo]",
    enrichContent: enricher
  });
};
