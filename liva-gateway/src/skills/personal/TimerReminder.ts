import { z } from "zod";
import { logger } from "@utils/logger";
import { randomUUID } from "node:crypto";

const TimerSchema = z.object({
  action: z.enum(["set_timer", "cancel_timer", "list_timers"]),
  durationMinutes: z.number().min(0.1).max(120).optional().describe("Thời gian hẹn (tính bằng phút)"),
  message: z.string().optional().describe("Nội dung lời nhắc khi hết giờ"),
  timerId: z.string().optional().describe("ID của timer cần hủy (dùng cho cancel_timer)")
});

export const metadata = {
  name: "timer_reminder",
  description: "[AUTO_RUN] Set countdown timer, Pomodoro, or reminder. LIVA monitors in background and auto-pushes notification when time expires. Can also cancel or list active timers.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["set_timer", "cancel_timer", "list_timers"] },
      durationMinutes: { type: "number", description: "Timer duration in minutes (for set_timer)" },
      message: { type: "string", description: "Reminder message text (for set_timer)" },
      timerId: { type: "string", description: "ID to cancel (for cancel_timer)" }
    },
    required: ["action"],
  },
};

class TimerRegistry {
    #activeTimers: Map<string, { timeout: NodeJS.Timeout, message: string, triggerTime: number }> = new Map();

    addTimer(durationMs: number, message: string): string {
        const id = randomUUID();
        const triggerTime = Date.now() + durationMs;
        
        const timeout = setTimeout(() => {
            logger.info(`[TimerReminder] ⏰ Hết giờ! Báo thức: ${message}`);
            const ipcMessage = JSON.stringify({
                event: "SHOW_TOAST",
                payload: {
                    title: "⏰ Hết giờ (Timer)",
                    message: message,
                    type: "warning",
                    duration: 15000 // Hiện 15 giây để user kịp chú ý
                }
            });
            process.stdout.write(ipcMessage + "\n");
            
            // Auto remove when fired
            this.#activeTimers.delete(id);
        }, durationMs);

        this.#activeTimers.set(id, { timeout, message, triggerTime });
        return id;
    }

    cancelTimer(id: string): boolean {
        const timer = this.#activeTimers.get(id);
        if (timer) {
            clearTimeout(timer.timeout);
            this.#activeTimers.delete(id);
            return true;
        }
        return false;
    }

    listTimers(): Array<{ id: string, message: string, remainingMs: number }> {
        const now = Date.now();
        const result: Array<{ id: string, message: string, remainingMs: number }> = [];
        for (const [id, data] of this.#activeTimers.entries()) {
            result.push({
                id,
                message: data.message,
                remainingMs: Math.max(0, data.triggerTime - now)
            });
        }
        return result;
    }

    dispose(): void {
        for (const [id, data] of this.#activeTimers.entries()) {
            clearTimeout(data.timeout);
            logger.info(`[TimerReminder] Đã dọn dẹp timer zombie (ID: ${id}) do hệ thống shutdown.`);
        }
        this.#activeTimers.clear();
    }
}

// Global registry instance
export const timerRegistry = new TimerRegistry();

export const execute = async (argsObj: unknown): Promise<string> => {
    try {
        const parsed = TimerSchema.parse(argsObj);

        if (parsed.action === "list_timers") {
            const list = timerRegistry.listTimers();
            if (list.length === 0) return "[TIMER INFO] Không có bộ hẹn giờ nào đang chạy.";
            let out = "[TIMER INFO] Các bộ đếm ngược đang chạy:\n";
            list.forEach((t, i) => {
                const mins = (t.remainingMs / 60000).toFixed(1);
                out += `${i+1}. ID: ${t.id} - Còn lại: ${mins} phút - Lời nhắc: "${t.message}"\n`;
            });
            return out;
        }

        if (parsed.action === "cancel_timer") {
            if (!parsed.timerId) return "[TIMER ERROR] Cần cung cấp timerId để hủy.";
            const success = timerRegistry.cancelTimer(parsed.timerId);
            if (success) return `[TIMER CANCELLED] Đã hủy bộ hẹn giờ ID: ${parsed.timerId}.`;
            return `[TIMER ERROR] Không tìm thấy bộ hẹn giờ với ID: ${parsed.timerId}.`;
        }

        if (parsed.action === "set_timer") {
            if (!parsed.durationMinutes || !parsed.message) {
                 return "[TIMER ERROR] Yêu cầu 'durationMinutes' và 'message' để đặt hẹn giờ.";
            }
            const durationMs = parsed.durationMinutes * 60 * 1000;
            const id = timerRegistry.addTimer(durationMs, parsed.message);
            logger.info(`[TimerReminder] Đã thiết lập hẹn giờ ${parsed.durationMinutes} phút. Nội dung: "${parsed.message}". ID: ${id}`);
            return `[TIMER SET] Đã bắt đầu đếm ngược ${parsed.durationMinutes} phút chạy ngầm. Khi hết giờ sẽ có thông báo cảnh báo: "${parsed.message}".\nTimer ID: ${id} (Có thể dùng ID này để hủy).`;
        }
        
        return "[TIMER ERROR] Hành động không hợp lệ.";
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[TimerReminder] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[TIMER ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[TIMER ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};

export const dispose = (): void => {
    timerRegistry.dispose();
};
