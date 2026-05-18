import { StructuredMemory } from "@memory/StructuredMemory";

export default {
  name: "update_task",
  description: "[AUTO_RUN] Cập nhật thông tin chi tiết (description), tiêu đề (title), hoặc trạng thái (status) của một kế hoạch/task trên hệ thống Dashboard của người dùng. Hãy dùng skill này để lưu lại lịch trình sau khi đã thảo luận xong với người dùng.",
  category: "core",
  isCoreSkill: true,
  parameters: {
    type: "object",
    properties: {
      task_id: {
        type: "string",
        description: "Mã ID của task cần cập nhật (ví dụ: task_123456_abcdef)."
      },
      title: {
        type: "string",
        description: "Tiêu đề mới của task (tuỳ chọn)."
      },
      description: {
        type: "string",
        description: "Nội dung/lịch trình chi tiết đã được tóm tắt (tuỳ chọn)."
      },
      status: {
        type: "string",
        description: "Trạng thái mới (ví dụ: pending, in-progress, completed) (tuỳ chọn)."
      }
    },
    required: ["task_id"]
  },
  execute: async (args: any) => {
    const { task_id, title, description, status } = args;
    
    if (!task_id) {
        return "Error: Missing task_id parameter.";
    }

    try {
        const sm = await StructuredMemory.create("liva_core");
        sm.updateTask(task_id, { title, description, status });
        
        // Notify the frontend via WS if possible, or assume it will poll/refresh
        // The UI might need to send a get_tasks event to refresh, but saving to DB is the main goal.
        
        return `Thành công! Đã cập nhật task ${task_id}.`;
    } catch (e: any) {
        return `Error updating task: ${e.message}`;
    }
  }
};
