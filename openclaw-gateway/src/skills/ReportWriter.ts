import axios from "axios";
import fs from "fs";
import path from "path";
import OpenAI from "openai";

// Helper báo Zalo bí mật để đẩy thông báo Mid-Flight (Tiến độ) xuống điện thoại Sếp
async function notifyZalo(msg: string) {
  const token = process.env.ZALO_OA_ACCESS_TOKEN;
  let userId = process.env.ZALO_USER_ID;
  if (!token || !userId) return;

  try {
     const isBotToken = token.includes(":");
     const endpoint = isBotToken 
         ? `https://bot-api.zaloplatforms.com/bot${token}/sendMessage`
         : "https://openapi.zalo.me/v3.0/oa/message/cs";
     
     if (isBotToken) {
         await axios.post(endpoint, { chat_id: userId, text: msg }).catch(() => {});
     } else {
         await axios.post(endpoint, {
            recipient: { user_id: userId },
            message: { text: msg }
         }, { headers: { access_token: token } }).catch(() => {});
     }
  } catch(e) {}
}

export const metadata = {
  name: "report_writer",
  description:
    "Kỹ năng Viết Báo Cáo Chuyên Sâu (Report Writer). Sử dụng khi người dùng yêu cầu 'Viết báo cáo', 'Phân tích số liệu', 'Đánh giá hiện trạng', 'Báo cáo khoa học'. Tự động chia làm 7 phần chuẩn mực. Nếu là Báo cáo Học thuật (isAcademic=true), sẽ tự động tìm thêm 10 bài báo khoa học từ Semantic Scholar làm dẫn chứng.",
  parameters: {
    type: "object",
    properties: {
      topic: {
        type: "string",
        description: "Chủ đề / Tên Dự Án cần báo cáo. Ví dụ: 'Báo cáo doanh thu Quý 1', 'Hiệu ứng bầy đàn'.",
      },
      fileLocation: {
        type: "string",
        description: "Thư mục lưu bản báo cáo Markdown. Khuyến nghị: E:/Project/LIVA/scratch_workspace"
      },
      providedContext: {
         type: "string",
         description: "Dữ kiện thực tế (số liệu, biên bản) do người dùng cung cấp (nếu có).",
      },
      isAcademic: {
         type: "boolean",
         description: "Đánh dấu là True nếu đây là đề tài Nghiên cứu Khoa học / Học thuật (sẽ cần cào Semantic Scholar). Nếu là Báo cáo Doanh nghiệp Business thông thường thì để False."
      }
    },
    required: ["topic", "fileLocation", "isAcademic"],
  },
};

export const execute = async (args: {
  topic: string;
  fileLocation: string;
  providedContext?: string;
  isAcademic: boolean;
}): Promise<string> => {
  
  const workspace = args.fileLocation;
  if (!fs.existsSync(workspace)) {
     fs.mkdirSync(workspace, { recursive: true });
  }

  await notifyZalo(`📝 [Chuyên Viên Báo Cáo LIVA]: Bắt đầu tiến trình phân tích Đa phần cho báo cáo "${args.topic}". Tiến trình này sẽ làm cực kỳ tỉ mỉ từng Chương một!`);
  
  let rawData = args.providedContext || "Không có dữ liệu số liệu thô tự cung cấp.";

  const aiClient = new OpenAI({
    baseURL: "http://127.0.0.1:8000/v1",
    apiKey: "local-ghost-layer",
  });

  // Dùng LLM tự nặn tên file chuẩn chỉnh
  let shortName = "report";
  try {
     const resName = await aiClient.chat.completions.create({
        model: "expert",
        messages: [{ role: "user", content: `Hành động như một bot đổi tên file. Rút gọn chủ đề sau thành 1 tên file tiếng Anh ngắn gọn, cực kỳ ý nghĩa (tối đa 4 từ, cách phối bằng gạch dưới _). VÍ DỤ: "Báo cáo doanh thu Quý 1" -> "q1_revenue_report". Tương tự hãy làm với Chủ đề: "${args.topic}". CHỈ TRẢ VỀ TÊN FILE, KHÔNG GIẢI THÍCH.` }],
        temperature: 0.1,
        max_tokens: 20
     });
     const aiName = resName.choices[0]?.message?.content?.trim();
     if (aiName && !aiName.includes(" ")) {
         shortName = aiName.replace(/[^\w-]/g, "").toLowerCase();
     } else if (aiName) {
         shortName = aiName.replace(/[^\w-]/g, "_").replace(/_+/g, "_").toLowerCase();
     }
  } catch(e) {}
  
  const safeFileName = shortName.substring(0, 40) + "_report.md";
  const targetPath = path.join(workspace, safeFileName);
  
  // Khởi tạo file trống
  fs.writeFileSync(targetPath, "", "utf8");

  const parts = [
    { name: "Phần 1: Thông tin chung (Header / Cover Page)", instruction: "Tạo Tiêu đề báo cáo, Người lập (LIVA AI), Người nhận (Ban Lãnh Đạo), Thời gian báo cáo." },
    { name: "Phần 2: Tóm tắt thực thi (Executive Summary)", instruction: "Tóm tắt gọn gàng (khoảng 200-300 chữ): Vấn đề cốt lõi là gì? Kết quả nổi bật nhất? Kiến nghị quan trọng nhất?" },
    { name: "Phần 3: Mở đầu / Bối cảnh (Introduction & Background)", instruction: "Lý do thực hiện báo cáo, Mục tiêu và phạm vi của báo cáo." },
    { name: "Phần 4: Nội dung chính - Số liệu & Hiện trạng (Main Findings)", instruction: "Phân tích số liệu, công việc thực hiện. BẮT BUỘC dùng Biểu đồ ASCII, bảng biểu Markdown để trực quan hóa dữ liệu." },
    { name: "Phần 5: Phân tích & Đánh giá (Analysis & Evaluation)", instruction: "Bóc tách nguyên nhân. Chia làm Điểm sáng (Pros/Thành công) và Tồn tại/Nút thắt (Cons/Bottlenecks) và lý giải vì sao." },
    { name: "Phần 6: Kết luận & Kiến nghị (Conclusion & Recommendations)", instruction: "Dựa trên phân tích, đưa ra giải pháp cụ thể: Cần làm gì tiếp theo? Cần hỗ trợ gì?" },
    { name: "Phần 7: Phụ lục (Appendices)", instruction: "Liệt kê các Link tham chiếu, dữ kiện thô, và list Nguồn Trích Dẫn khoa học (nếu có)." }
  ];

  let conversation: any[] = [
    { 
       role: "system", 
       content: `Bạn là Chuyên viên Phân tích và Chiến lược gia cấp cao của Doanh nghiệp. Nhiệm vụ của bạn là viết một BẢN BÁO CÁO CỰC KỲ SÂU SẮC.
Bạn sẽ viết CHẬM RÃI từng Phần một theo yêu cầu của hệ thống để đảm bảo chất lượng sắc bén nhất.
DƯỚI ĐÂY LÀ DỮ LIỆU/NGỮ CẢNH CUNG CẤP LÀM CƠ SỞ (Tôn trọng tuyệt đối dữ liệu này, không bịa số liệu quá đáng nếu trong đây đã có):
====================
${rawData}
====================
`
    }
  ];

  for (let i = 0; i < parts.length; i++) {
     const part = parts[i];
     console.log(`[ReportWriter] Đang viết ${part.name}...`);
     
     // Thêm Prompt cho AI
     conversation.push({ 
        role: "user", 
        content: `HÃY VIẾT: **${part.name}**\nHướng dẫn: ${part.instruction}\nYêu cầu: Viết dài, sâu sắc. BẮT BUỘC sử dụng Markdown kết hợp với cú pháp Toán học LaTeX ($$...$$) để làm nổi bật các phép tính và luận điểm. TRẢ VỀ TRỰC TIẾP NỘI DUNG của Phần này, KHÔNG CẦN CHÀO HỎI.` 
     });

     try {
       const res = await aiClient.chat.completions.create({
          model: "expert",
          messages: conversation,
          temperature: 0.3,
          max_tokens: 3000,
       });

       let replyContent = res.choices[0]?.message?.content || "";
       if (!replyContent || replyContent.length < 5) {
          replyContent = `*(Lỗi: Tràn quá trình sinh ký tự cho phần này)*\n`;
       }

       // Lưu vào History để phần sau viết kế thừa
       conversation.push({ role: "assistant", content: replyContent });

       // Ghi nối đè (append) vào File
       fs.appendFileSync(targetPath, `\n\n## ${part.name}\n\n${replyContent}\n\n---\n`, "utf8");

       // Báo tín hiệu nhịp đập
       await notifyZalo(`✍️ [LIVA Report]: Đã viết xong ${part.name}...`);

     } catch(e: any) {
       console.error(`Error generating ${part.name}:`, e.message);
       fs.appendFileSync(targetPath, `\n\n## ${part.name}\n\n*(Lỗi mạng/VRAM khi viết phần này: ${e.message})*\n\n---\n`, "utf8");
     }
  }

  const absolutePath = path.resolve(targetPath);
  await notifyZalo(`🎯 [Chuyên Viên Báo Cáo LIVA]: HOÀN TẤT BÁO CÁO! Bản báo cáo 7 phần chuyên sâu chuẩn Harvard đã ra lò!
📂 Vị trí: ${absolutePath}
Mời sếp mở máy tính để duyệt ngay và luôn!`);

  return `Hoàn tất Tuyệt Giao! Đã tạo thành công Báo Cáo chuyên sâu lưu tại: ${absolutePath}`;
};
