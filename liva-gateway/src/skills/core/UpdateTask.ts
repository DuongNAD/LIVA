import { StructuredMemory } from "@memory/StructuredMemory";

export const metadata = {
  name: "update_task",
  search_keywords: ["task", "kế hoạch", "lịch trình", "cập nhật task", "todo", "to-do", "nhiệm vụ", "công việc"],
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
};

export const execute = async (args: any): Promise<string> => {
  const { task_id, title, description, status } = args;
  
  if (!task_id) {
      return "Error: Missing task_id parameter.";
  }

  try {
      const sm = await StructuredMemory.create("liva_core");
      sm.updateTask(task_id, { title, description, status });
      
      return `Thành công! Đã cập nhật task ${task_id}.`;
  } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      return `Error updating task: ${errMsg}`;
  }
};

