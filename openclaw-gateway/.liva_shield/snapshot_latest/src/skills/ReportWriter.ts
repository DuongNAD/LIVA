import axios from "axios";
import fs from "fs";
import path from "path";
import { notifyZalo } from "../utils/ZaloNotifier";
import { livaEngine, generateSmartFilename } from "../utils/LivaEngine";

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
  
  const workspace = args.fileLocation;
  if (!fs.existsSync(workspace)) {
     fs.mkdirSync(workspace, { recursive: true });
  }

  await notifyZalo(`📝 [Chuyên Viên Báo Cáo LIVA]: Bắt đầu tiến trình phân tích Đa phần cho báo cáo "${args.topic}". Tiến trình này sẽ làm cực kỳ tỉ mỉ từng Chương một!`);
  
  let rawData = args.providedContext || "Không có dữ liệu số liệu thô tự cung cấp.";

  // Nếu là báo cáo Khoa học, fetch Semantic Scholar
  if (args.isAcademic) {
     const semanticKey = process.env.SEMANTIC_SCHOLAR_API_KEY || "";
     try {
       await notifyZalo(`🔎 [Giáo Sư Học Thuật]: Đây là báo cáo có tính Học Thuật. Đang truy cập Semantic Scholar để lấy 10 bài Abstract siêu uy tín...`);
       const encodedTopic = encodeURIComponent(args.topic);
       const headers = semanticKey ? { "x-api-key": semanticKey } : {};
       const scholarUrl = `https://api.semanticscholar.org/graph/v1/paper/search?query=${encodedTopic}&limit=10&fields=title,abstract,authors,year,url,citationCount`;
       
       const response = await axios.get(scholarUrl, { headers });
       if (response.data?.data && response.data.data.length > 0) {
          let extractedContext = [];
          for (const paper of response.data.data) {
             if (!paper.abstract) continue;
             const authors = paper.authors ? paper.authors.map((a:any)=>a.name).join(", ") : "Unknown";
             extractedContext.push(`[Trích Dẫn] Title: ${paper.title} (Năm: ${paper.year})\nTác giả: ${authors}\nURL: ${paper.url}\nAbstract: ${paper.abstract}`);
          }
           rawData += "\n\n=== TÀI LIỆU KHOA HỌC THAM KHẢO TỪ SEMANTIC SCHOLAR ===\n" + extractedContext.join("\n\n");
           await notifyZalo(`✅ [Giáo Sư Học Thuật]: Đã thu thập xong ${response.data.data.length} nghiên cứu! Bắt đầu chắp bút...`);
       }
     } catch (err: any) {
         console.error("Semantic Scholar API Error:", err.message);
     }
  }

  // Dùng LLM tự nặn tên file chuẩn chỉnh
  const shortName = await generateSmartFilename(args.topic, "report");
  const targetPath = path.join(workspace, shortName.substring(0, 40) + "_report.md");
  fs.writeFileSync(targetPath, "", "utf8");

  const parts = [
    { name: "Phần 1: Thông tin chung (Header / Cover Page)", instruction: "Tạo Tiêu đề báo cáo, Người lập (LIVA AI), Người nhận (Ban Lãnh Đạo), Thời gian báo cáo." },
    { name: "Phần 2: Tóm tắt thực thi (Executive Summary)", instruction: "Tóm tắt gọn gàng (khoảng 200-300 chữ): Vấn đề cốt lõi là gì? Kết quả nổi bật nhất? Kiến nghị quan trọng nhất?" },
    { name: "Phần 3: Mở đầu & Bối cảnh (Introduction)", instruction: "Lý do và bối cảnh lập báo cáo. Định hướng mục tiêu của bài báo cáo này." },
    { name: "Phần 4: Dữ liệu & Hiện trạng (Findings / Data)", instruction: "Liệt kê rõ ràng số liệu gốc. Khuyến khích sử dụng BẢNG Markdown. Tránh đưa ra nhận định cá nhân ở phần này." },
    { name: "Phần 5: Phân tích & Đánh giá (Analysis)", instruction: "Từ số liệu trên, rút ra Insight gì? Điểm sáng là gì? Điểm yếu là gì? Nguyên nhân vì sao?" },
    { name: "Phần 6: Kết luận & Kiến nghị (Conclusion & Recommendations)", instruction: "Đề xuất Next-steps cụ thể. Phải làm gì tiếp theo?" },
    { name: "Phần 7: Phụ lục & Trích dẫn (Appendices / References)", instruction: "Danh sách nguồn dữ liệu và tài liệu tham khảo." }
  ];

  let conversation: any[] = [
    { 
       role: "system", 
       content: `Bạn là LIVA - Cố Vấn Cao Cấp và Chuyên gia Phân tích.
Bạn sẽ viết CHẬM RÃI từng Phần của Báo Cáo. 
DỮ LIỆU ĐẦU VÀO ĐƯỢC CUNG CẤP LÀ:
====================
${rawData}
====================`
    }
  ];

  for (let i = 0; i < parts.length; i++) {
     const part = parts[i];
     console.log(`[ReportWriter] Đang viết ${part.name}...`);
     
     conversation.push({ 
        role: "user", 
        content: `HÃY VIẾT: **${part.name}**\nHướng dẫn: ${part.instruction}\nYêu cầu: Viết dài, sâu sắc. BẮT BUỘC sử dụng Markdown kết hợp với cú pháp Toán học LaTeX ($$..$$ hoặc $..$) để làm nổi bật các phép tính và luận điểm. TRẢ VỀ TRỰC TIẾP NỘI DUNG của Phần này, KHÔNG CẦN CHÀO HỎI.` 
     });

     try {
       const res = await livaEngine.chat.completions.create({
          model: "expert",
          messages: conversation,
          temperature: 0.35,
          max_tokens: 3000,
       });

       let replyContent = res.choices[0]?.message?.content || "";
       if (!replyContent || replyContent.length < 5) {
          replyContent = `*(Lỗi rỗng do giới hạn API)*\n`;
       }

       conversation.push({ role: "assistant", content: replyContent });
       fs.appendFileSync(targetPath, `\n\n## ${part.name}\n\n${replyContent}\n\n---\n`, "utf8");
       await notifyZalo(`🗓️ [Báo Cáo]: Đã viết xong ${part.name}...`);

     } catch(e: any) {
       console.error(`Error generating ${part.name}:`, e.message);
       fs.appendFileSync(targetPath, `\n\n## ${part.name}\n\n*(Lỗi mạng/VRAM)*\n\n---\n`, "utf8");
     }
  }

  const absolutePath = path.resolve(targetPath);
  await notifyZalo(`✅ [Báo Cáo Hoàn Tất]: Tuyệt phẩm độ dài ngàn chữ đã ra lò! Mời sếp duyệt file Markdown tại: ${absolutePath}`);

  return `Báo cáo đã xuất bản cực kỳ chi tiết tại: ${absolutePath}`;
};
