import { executeDocumentWriter, DocumentSection } from "./DocumentWriterBase";
import { logger } from "../utils/logger";
export const metadata = {
  name: "plan_writer",
  search_keywords: ["plan_writer","plan writer"],
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
  const parts: DocumentSection[] = [
    { name: "Phần 1: Tổng quan dự án (Project Overview)", instruction: "Tên kế hoạch, Người phụ trách chính (LIVA AI Project Manager), Tóm tắt mục đích: Kế hoạch này lập ra để giải quyết bài toán gì?" },
    { name: "Phần 2: Phân tích bối cảnh (Situation Analysis - SWOT)", instruction: "Đánh giá hiện trạng. Sử dụng mô hình SWOT (Điểm mạnh, Điểm yếu, Cơ hội, Thách thức). Hãy phân tích thật sắc bén." },
    { name: "Phần 3: Mục tiêu (Objectives & KPIs - SMART)", instruction: "Thiết lập mục tiêu theo nguyên tắc SMART (Cụ thể, Đo lường được, Khả thi, Có thời hạn). Đề xuất cụ thể các con số KPI." },
    { name: "Phần 4: Chiến lược & Kế hoạch hành động chi tiết (Action Plan)", instruction: "Chiến lược chung (Strategy) và Bảng Hạng mục công việc (What & How). Phân rã thành các giai đoạn (Phases), đầu việc (Tasks) và phân công (Who - PIC)." },
    { name: "Phần 5: Tiến độ thực hiện (Timeline / When)", instruction: "Deadline cho từng hạng mục. Trình bày bằng Bảng biểu rành mạch để thay thế cho Gantt Chart." },
    { name: "Phần 6: Nguồn lực & Ngân sách (Resources & Budget)", instruction: "Cần bao nhiêu tiền? Bảng dự toán chi phí chi tiết. Nguồn lực nhân sự và công cụ cần có." },
    { name: "Phần 7: Quản trị rủi ro (Risk Management)", instruction: "Dự báo tình huống xấu rủi ro phát sinh và BẮT BUỘC đưa ra Phương án dự phòng (Plan B)." },
    { name: "Phần 8: Tiêu chí đo lường (Evaluation Metrics)", instruction: "Đo lường thành công bằng công cụ/chỉ số nào? Tần suất họp báo cáo tiến độ (check-point)." }
  ];

  return executeDocumentWriter({
    title: args.projectName,
    workspace: args.fileLocation,
    type: "plan",
    systemPrompt: `Bạn là Vị Giám Đốc Dự Án (Project Manager) xuất sắc nhất thế giới.\nNhiệm vụ của bạn là lập một BẢN KẾ HOẠCH TƯƠNG LAI CỰC KỲ CHI TIẾT (Tránh nói chung chung, đưa ra các giả định về số liệu, deadline, đầu việc cục kỳ thực tế).`,
    startMessage: `📋 [Tư Vấn Dự Án LIVA]: Đã nhận lệnh lập kế hoạch "${args.projectName}". Em bắt đầu nặn ra 8 phần chuẩn chỉnh cho sếp nha!`,
    endMessage: `🚀 [Tư Vấn Dự Án LIVA]: XONG! Kế hoạch kinh điển với 8 phần đã được vạch ra đầy đủ! 📂 Mời xem tại: {absolutePath}`,
    successMessage: "Hoàn tất Xuất chúng! Đã tạo thành công Bản Kế Hoạch tại: {absolutePath}",
    rawData: args.providedContext || "Không có yêu cầu/dữ liệu cụ thể nào từ người dùng, hãy tự đề xuất các con số hợp lý nhằm minh hoạ một bản kế hoạch hoàn hảo.",
    parts,
    loggerPrefix: "[PlanWriter]",
    zaloPrefix: "🗓️ [LIVA Plan]"
  });
};
