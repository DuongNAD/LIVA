import { z } from "zod";
import { logger } from "@utils/logger";

const NotificationSchema = z.object({
  title: z.string().min(1, "Thiếu tiêu đề thông báo"),
  message: z.string().min(1, "Thiếu nội dung thông báo"),
  type: z.enum(["info", "success", "warning", "error"]).optional().default("info"),
  durationMs: z.number().optional().default(5000)
});

export const metadata = {
  name: "push_ui_notification",
  description: "Bắn trực tiếp thông báo (Toast/Notification) lên màn hình Desktop/UI nội bộ của người dùng thông qua IPC, không dùng bash script.",
  kit: "SOCIAL_KIT",
  parameters: {
    type: "object",
    properties: {
      title: { type: "string", description: "Tiêu đề thông báo" },
      message: { type: "string", description: "Nội dung chi tiết" },
      type: { type: "string", enum: ["info", "success", "warning", "error"], description: "Loại thông báo" },
      durationMs: { type: "number", description: "Thời gian hiển thị (mili giây)" }
    },
    required: ["title", "message"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = NotificationSchema.parse(argsObj);
        
        const ipcMessage = JSON.stringify({
            event: "SHOW_TOAST",
            payload: {
                title: parsed.title,
                message: parsed.message,
                type: parsed.type,
                duration: parsed.durationMs
            }
        });

        // Bắn sự kiện IPC chuẩn (Giao tiếp với Tauri/Electron process cha)
        process.stdout.write(ipcMessage + "\n");
        logger.info(`[NotificationPusher] Bắn IPC Toast: ${parsed.title} - ${parsed.type}`);

        return `[NOTIFICATION SUCCESS] Đã hiển thị thông báo '${parsed.title}' lên màn hình người dùng.`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[NotificationPusher] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[NOTIFICATION ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[NOTIFICATION ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
