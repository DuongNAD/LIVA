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
  name: "plan_writer",
  description:
    "Kỹ năng Viết Bản Kế Hoạch Dự Án (Plan Writer). Sử dụng khi người dùng yêu cầu 'Lập kế hoạch', 'Lên lộ trình', 'Plan ra mắt', 'Action plan'. Tự động chia làm 8 phần chuẩn mực quản trị (SWOT, SMART, Action Plan, Timeline, Budget, Risk Management).",
  parameters: {
    type: "object",
    properties: {
      projectName: {
        type: "string",
        description: "Tên dự án hoặc Kế hoạch cần lập. Ví dụ: 'Kế hoạch ra mắt sản phẩm mới', 'Kế hoạch Marketing Quý 2'.",
      },
      fileLocation: {
        type: "string",
        description: "Thư mục lưu bản kế hoạch Markdown. Khuyến nghị: E:/Project/LIVA/scratch_workspace"
      },
      providedContext: {
         type: "string",
         description: "Dữ kiện thực tế (yêu cầu từ sếp, ngân sách dự kiến, thời hạn) do người dùng cung cấp (nếu có).",
      }
    },
    required: ["projectName", "fileLocation"],
  },
};

export const execute = async (args: {
  projectName: string;
  fileLocation: string;
  providedContext?: string;
}): Promise<string> => {
  
  const workspace = args.fileLocation;
  if (!fs.existsSync(workspace)) {
     fs.mkdirSync(workspace, { recursive: true });
  }

  await notifyZalo(`📋 [Tư Vấn Dự Án LIVA]: Đã nhận lệnh lập kế hoạch "${args.projectName}". Em bắt đầu nặn ra 8 phần chuẩn chỉnh cho sếp nha!`);
  
  let rawData = args.providedContext || "Không có yêu cầu/dữ liệu cụ thể nào từ người dùng, hãy tự đề xuất các con số và giả định hợp lý nhằm minh hoạ một bản kế hoạch hoàn hảo.";

  const aiClient = new OpenAI({
    baseURL: "http://127.0.0.1:8000/v1",
    apiKey: "local-ghost-layer",
  });

  // Dùng LLM tự nặn tên file ngắn gọn (Smart Filename)
  let shortName = "plan";
  try {
     const resName = await aiClient.chat.completions.create({
        model: "expert",
        messages: [{ role: "user", content: `Hành động như một bot đổi tên file. Rút gọn tên dự án sau thành 1 tên file tiếng Anh ngắn gọn, cực kỳ ý nghĩa (tối đa 4 từ, cách phối bằng gạch dưới _). VÍ DỤ: "Kế hoạch ra mắt ứng dụng Giao đồ ăn" -> "food_delivery_launch_plan". Tương tự hãy làm với Dự án: "${args.projectName}". CHỈ TRẢ VỀ TÊN FILE, KHÔNG GIẢI THÍCH.` }],
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
  
  const safeFileName = shortName.substring(0, 40) + "_plan.md";
  const targetPath = path.join(workspace, safeFileName);
  
  // Khởi tạo file trống
  fs.writeFileSync(targetPath, "", "utf8");

  const parts = [
    { name: "Phần 1: Tổng quan dự án (Project Overview)", instruction: "Tên kế hoạch, Người phụ trách chính (LIVA AI Project Manager), Tóm tắt mục đích: Kế hoạch này lập ra để giải quyết bài toán gì?" },
    { name: "Phần 2: Phân tích bối cảnh (Situation Analysis - SWOT)", instruction: "Đánh giá hiện trạng. Sử dụng mô hình SWOT (Điểm mạnh, Điểm yếu, Cơ hội, Thách thức). Hãy phân tích thật sắc bén." },
    { name: "Phần 3: Mục tiêu (Objectives & KPIs - SMART)", instruction: "Thiết lập mục tiêu theo nguyên tắc SMART (Cụ thể, Đo lường được, Khả thi, Có thời hạn). Đề xuất cụ thể các con số KPI." },
    { name: "Phần 4: Chiến lược & Kế hoạch hành động chi tiết (Action Plan)", instruction: "Chiến lược chung (Strategy) và Bảng Hạng mục công việc (What & How). Phân rã thành các giai đoạn (Phases), đầu việc (Tasks) và phân công (Who - PIC)." },
    { name: "Phần 5: Tiến độ thực hiện (Timeline / When)", instruction: "Deadline cho từng hạng mục. Trình bày bằng Bảng biểu rành mạch để thay thế cho Gantt Chart." },
    { name: "Phần 6: Nguồn lực & Ngân sách (Resources & Budget)", instruction: "Cần bao nhiêu tiền? Bảng dự toán chi phí chi tiết. Nguồn lực nhân sự và công cụ cần có." },
    { name: "Phần 7: Quản trị rủi ro (Risk Management)", instruction: "Dự báo tình huống xấu rủi ro phát sinh và BẮT BUỘC đưa ra Phương án dự phòng (Plan B)." },
    { name: "Phần 8: Tiêu chí đo lường (Evaluation Metrics)", instruction: "Đo lường thành công bằng công cụ/chỉ số nào? Tần suất họp báo cáo tiến độ (check-point)." }
  ];

  let conversation: any[] = [
    { 
       role: "system", 
       content: `Bạn là Vị Giám Đốc Dự Án (Project Manager) và Cố vấn Chiến Lược xuất sắc nhất thế giới.
Nhiệm vụ của bạn là lập một BẢN KẾ HOẠCH TƯƠNG LAI CỰC KỲ CHI TIẾT (Tránh nói chung chung, phải luôn đưa ra các giả định về số liệu, deadline, đầu việc cực kỳ thực tế).
Bạn sẽ viết CHẬM RÃI từng Phần một theo yêu cầu dưới đây để đảm bảo chất lượng sắc bén nhất.
YÊU CẦU ĐẦU VÀO CỦA DỰ ÁN TỪ SẾP:
====================
${rawData}
====================
`
    }
  ];

  for (let i = 0; i < parts.length; i++) {
     const part = parts[i];
     console.log(`[PlanWriter] Đang viết ${part.name}...`);
     
     // Thêm Prompt cho AI
     conversation.push({ 
        role: "user", 
        content: `HÃY VIẾT: **${part.name}**\nHướng dẫn: ${part.instruction}\nYêu cầu: Viết dài, sâu sắc. BẮT BUỘC tận dụng Markdown kết hợp cú pháp Toán học LaTeX ($$...$$) để biểu diễn bảng biểu, công thức hoặc các tính toán ngân sách. TRẢ VỀ TRỰC TIẾP NỘI DUNG của Phần này, KHÔNG CẦN CHÀO HỎI.` 
     });

     try {
       const res = await aiClient.chat.completions.create({
          model: "expert",
          messages: conversation,
          temperature: 0.35,
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
       await notifyZalo(`🗓️ [LIVA Plan]: Đã viết xong ${part.name}...`);

     } catch(e: any) {
       console.error(`Error generating ${part.name}:`, e.message);
       fs.appendFileSync(targetPath, `\n\n## ${part.name}\n\n*(Lỗi mạng/VRAM khi lập kế hoạch phần này: ${e.message})*\n\n---\n`, "utf8");
     }
  }

  const absolutePath = path.resolve(targetPath);
  await notifyZalo(`🚀 [Tư Vấn Dự Án LIVA]: XONG! Kế hoạch kinh điển với 8 phần đã được vạch ra đầy đủ! Sẵn sàng chinh phục mục tiêu!
📂 Chiêm ngưỡng tại: ${absolutePath}`);

  return `Hoàn tất Xuất chúng! Đã tạo thành công Bản Kế Hoạch lưu tại: ${absolutePath}`;
};
