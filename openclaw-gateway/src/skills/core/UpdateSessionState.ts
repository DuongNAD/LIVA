import { SkillMetadata } from "../../types/Contracts";

export const update_session_state: SkillMetadata = {
  name: "update_session_state",
  description:
    "Cập nhật trạng thái phiên làm việc (SESSION-STATE.md) theo nguyên tắc Write-Ahead Logging (WAL). BẮT BUỘC sử dụng kỹ năng này ĐỂ LƯU LẠI SUY NGHĨ HOẶC KẾ HOẠCH TRƯỚC KHI phản hồi người dùng hoặc thực hiện tác vụ dài hạn. Việc này giúp hệ thống không bị mất tiến độ nếu bị khởi động lại.",
  isCoreSkill: true,
  parameters: {
    type: "object",
    properties: {
      intent: {
        type: "string",
        description: "Mục tiêu cốt lõi của phiên làm việc hiện tại (Ví dụ: 'Phân tích mã nguồn', 'Tạo báo cáo').",
      },
      current_context: {
        type: "string",
        description: "Ngữ cảnh hiện tại: Các dữ liệu đang xử lý, lỗi gặp phải, hoặc tiến độ chung.",
      },
      pending_tasks: {
        type: "array",
        items: {
          type: "string",
        },
        description: "Danh sách các tác vụ cụ thể cần làm tiếp theo.",
      },
    },
    required: ["intent", "current_context", "pending_tasks"],
  },
  execute: async (args: any) => {
    const { intent, current_context, pending_tasks } = args;

    if (!intent || !current_context || !Array.isArray(pending_tasks)) {
      return "Lỗi: Tham số không hợp lệ. Phải cung cấp đầy đủ intent, current_context và mảng pending_tasks.";
    }

    // Build the formatted markdown content
    let content = `# WORKING SESSION STATE\n\n`;
    content += `## Intent\n${intent}\n\n`;
    content += `## Current Context\n${current_context}\n\n`;
    content += `## Pending Tasks\n`;
    for (const task of pending_tasks) {
      content += `- [ ] ${task}\n`;
    }

    const memory = (global as any).kernelInstance?.memory;
    if (memory) {
      await memory.updateSessionState(content);
      return "✅ Đã lưu Session State thành công. Hệ thống đã an toàn để tiếp tục thực thi hoặc trả lời người dùng.";
    } else {
      return "Lỗi: Không tìm thấy hệ thống MemoryManager.";
    }
  },
};
