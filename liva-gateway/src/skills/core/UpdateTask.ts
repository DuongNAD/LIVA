import { StructuredMemory } from "@memory/StructuredMemory";

export const metadata = {
  name: "update_task",
  search_keywords: ["task", "kế hoạch", "lịch trình", "cập nhật task", "todo", "to-do", "nhiệm vụ", "công việc"],
  description: "[AUTO_RUN] Update task details (description, title, or status) on the user\'s Dashboard. Use this skill to save schedules/plans after discussing with the user.",
  category: "core",
  isCoreSkill: true,
  parameters: {
    type: "object",
    properties: {
      task_id: {
        type: "string",
        description: "Task ID to update (e.g., task_123456_abcdef)."
      },
      title: {
        type: "string",
        description: "New task title (optional)."
      },
      description: {
        type: "string",
        description: "Summarized details/schedule content (optional)."
      },
      status: {
        type: "string",
        description: "New task status (e.g., pending, in-progress, completed) (optional)."
      }
    },
    required: ["task_id"]
  },
};

export const execute = async (args: { task_id?: string; title?: string; description?: string; status?: string; }): Promise<string> => {
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

