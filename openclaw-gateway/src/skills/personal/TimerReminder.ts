import { z } from "zod";
import { logger } from "@utils/logger";

const TimerSchema = z.object({
  action: z.enum(["set_timer"]),
  durationMinutes: z.number().min(0.1).max(120).describe("Thời gian hẹn (tính bằng phút)"),
  message: z.string().describe("Nội dung lời nhắc khi hết giờ")
});

export const metadata = {
  name: "timer_reminder",
  description: "Thiết lập bộ đếm lùi, Pomodoro hoặc hẹn giờ nhắc nhở. LIVA sẽ theo dõi ngầm và tự động bắn thông báo ra màn hình khi hết thời gian.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["set_timer"] },
      durationMinutes: { type: "number", description: "Số phút hẹn giờ" },
      message: { type: "string", description: "Nội dung báo thức" }
    },
    required: ["action", "durationMinutes", "message"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = TimerSchema.parse(argsObj);
        const durationMs = parsed.durationMinutes * 60 * 1000;

        logger.info(`[TimerReminder] Đã thiết lập hẹn giờ ${parsed.durationMinutes} phút. Nội dung: "${parsed.message}"`);

        // Đẩy 1 tiến trình chạy ngầm (Zero-Blocking Main Thread)
        setTimeout(() => {
            logger.info(`[TimerReminder] ⏰ Hết giờ! Báo thức: ${parsed.message}`);
            const ipcMessage = JSON.stringify({
                event: "SHOW_TOAST",
                payload: {
                    title: "⏰ Hết giờ (Timer)",
                    message: parsed.message,
                    type: "warning",
                    duration: 15000 // Hiện 15 giây để user kịp chú ý
                }
            });
            process.stdout.write(ipcMessage + "\n");
        }, durationMs);

        return `[TIMER SET] Đã bắt đầu đếm ngược ${parsed.durationMinutes} phút chạy ngầm. Khi hết giờ sẽ có thông báo cảnh báo: "${parsed.message}". Bạn có thể chuyển sang làm việc khác.`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[TimerReminder] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[TIMER ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[TIMER ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
