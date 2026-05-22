import { logger } from "../utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

// ─── Constants ─────────────────────────────────────────────────────────────────
const CHECK_INTERVAL_MS = 60 * 1000;               // Kiểm tra mỗi 60 giây
const DEFAULT_MAX_CONTINUOUS_MS = 2 * 60 * 60 * 1000; // 2 tiếng liên tục → nhắc nghỉ
const DEFAULT_BREAK_DURATION_MS = 5 * 60 * 1000;    // Nghỉ tối thiểu 5 phút
const IDLE_RESET_THRESHOLD_MS = 5 * 60 * 1000;      // Idle > 5 phút → reset session
const ACTIVE_THRESHOLD_MS = 5 * 1000;               // Idle < 5 giây = đang active

// Dim screen về mức này khi nhắc nghỉ
const DIM_BRIGHTNESS = 20;

// ─── PowerShell Script: GetLastInputInfo ───────────────────────────────────────
const PS_GET_IDLE_MS = `
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class IdleTime {
    [DllImport("user32.dll")] static extern bool GetLastInputInfo(ref LASTINPUTINFO plii);
    [StructLayout(LayoutKind.Sequential)] struct LASTINPUTINFO { public uint cbSize; public uint dwTime; }
    public static uint Get() {
        LASTINPUTINFO lii = new LASTINPUTINFO();
        lii.cbSize = (uint)Marshal.SizeOf(typeof(LASTINPUTINFO));
        GetLastInputInfo(ref lii);
        return ((uint)Environment.TickCount - lii.dwTime);
    }
}
'@
[IdleTime]::Get()
`.trim().replace(/\n/g, "; ");

// ─── Dependency Interface ──────────────────────────────────────────────────────
export interface HealthPostureMonitorDeps {
    /** Đọc TTS nhắc nghỉ */
    speakTTS: (text: string) => Promise<void>;
    /** Push toast notification lên UI */
    pushNotification: (title: string, body: string) => void;
    /** Dim/restore màn hình */
    setBrightness: (level: number) => Promise<void>;
    /** Kiểm tra Agent đang bận xử lý hay không */
    isAgentBusy: () => boolean;
}

// ─── Daemon Class ──────────────────────────────────────────────────────────────
export class HealthPostureMonitor {
    #deps: HealthPostureMonitorDeps;
    #intervalRef: ReturnType<typeof setInterval> | null = null;

    /** Thời điểm bắt đầu session làm việc liên tục */
    #sessionStartTime: number | null = null;
    /** Đã gửi reminder trong session hiện tại chưa */
    #reminderSent = false;
    /** Thời điểm gửi reminder (để track break duration) */
    #reminderSentAt: number | null = null;

    /** Cấu hình có thể tùy chỉnh */
    #maxContinuousMs: number;
    #breakDurationMs: number;

    constructor(
        deps: HealthPostureMonitorDeps,
        options?: { maxContinuousMs?: number; breakDurationMs?: number },
    ) {
        this.#deps = deps;
        this.#maxContinuousMs = options?.maxContinuousMs ?? DEFAULT_MAX_CONTINUOUS_MS;
        this.#breakDurationMs = options?.breakDurationMs ?? DEFAULT_BREAK_DURATION_MS;
    }

    /**
     * Bắt đầu daemon. Timer uses .unref() để prevent zombie.
     */
    public start(): void {
        if (this.#intervalRef) return;

        logger.info(
            `[HealthPosture] 🏥 Started — nhắc nghỉ sau ${this.#maxContinuousMs / 60_000} phút liên tục, nghỉ tối thiểu ${this.#breakDurationMs / 60_000} phút`,
        );

        this.#intervalRef = setInterval(() => {
            this.#tick().catch(err => {
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[HealthPosture] Tick error: ${msg}`);
            });
        }, CHECK_INTERVAL_MS);
        this.#intervalRef.unref();
    }

    /**
     * Dừng daemon và cleanup.
     */
    public dispose(): void {
        if (this.#intervalRef) {
            clearInterval(this.#intervalRef);
            this.#intervalRef = null;
        }
        this.#sessionStartTime = null;
        this.#reminderSent = false;
        this.#reminderSentAt = null;
        logger.info("[HealthPosture] 🛑 Disposed.");
    }

    /**
     * Lấy idle time hiện tại qua PowerShell Win32 GetLastInputInfo.
     * @returns Thời gian idle tính bằng milliseconds, hoặc -1 nếu lỗi.
     */
    async #getIdleMs(): Promise<number> {
        try {
            const { stdout } = await execAsync(
                `powershell.exe -NoProfile -Command "${PS_GET_IDLE_MS}"`,
                { timeout: 8_000 },
            );
            const ms = parseInt(stdout.trim(), 10);
            return Number.isNaN(ms) ? -1 : ms;
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[HealthPosture] Lỗi đọc idle time: ${errMsg}`);
            return -1;
        }
    }

    /**
     * Main tick — chạy mỗi 60 giây.
     */
    async #tick(): Promise<void> {
        const idleMs = await this.#getIdleMs();
        if (idleMs < 0) return; // Lỗi đọc, bỏ qua tick này

        const now = Date.now();
        const isActive = idleMs < ACTIVE_THRESHOLD_MS;
        const isLongIdle = idleMs >= IDLE_RESET_THRESHOLD_MS;

        // ── User idle lâu → reset session ──
        if (isLongIdle) {
            if (this.#sessionStartTime !== null) {
                const sessionDuration = Math.round((now - this.#sessionStartTime) / 60_000);
                logger.info(`[HealthPosture] 😴 User idle ${Math.round(idleMs / 1000)}s — reset session (đã làm ${sessionDuration} phút)`);
                this.#sessionStartTime = null;
                this.#reminderSent = false;
                this.#reminderSentAt = null;
            }
            return;
        }

        // ── User đang active ──
        if (isActive) {
            // Bắt đầu session mới nếu chưa có
            if (this.#sessionStartTime === null) {
                this.#sessionStartTime = now;
                this.#reminderSent = false;
                this.#reminderSentAt = null;
                logger.debug("[HealthPosture] 🟢 Session mới bắt đầu");
                return;
            }

            const continuousMs = now - this.#sessionStartTime;

            // Đã gửi reminder rồi → chờ break hoặc reset
            if (this.#reminderSent) {
                // Nếu user vẫn active sau khi được nhắc → nhắc lại mỗi 30 phút
                if (this.#reminderSentAt && (now - this.#reminderSentAt) >= 30 * 60 * 1000) {
                    logger.info("[HealthPosture] ⚠️ User vẫn chưa nghỉ sau reminder, nhắc lại");
                    await this.#sendBreakReminder(continuousMs);
                }
                return;
            }

            // Kiểm tra đã đến ngưỡng chưa
            if (continuousMs >= this.#maxContinuousMs) {
                logger.info(
                    `[HealthPosture] 🚨 User làm việc liên tục ${Math.round(continuousMs / 60_000)} phút — kích hoạt nhắc nghỉ`,
                );
                await this.#sendBreakReminder(continuousMs);
            }
        }
    }

    /**
     * Gửi break reminder: dim screen, TTS, push toast.
     */
    async #sendBreakReminder(continuousMs: number): Promise<void> {
        const hours = Math.floor(continuousMs / 3_600_000);
        const minutes = Math.round((continuousMs % 3_600_000) / 60_000);
        const timeStr = hours > 0 ? `${hours} tiếng ${minutes} phút` : `${minutes} phút`;

        this.#reminderSent = true;
        this.#reminderSentAt = Date.now();

        // Dim screen
        try {
            await this.#deps.setBrightness(DIM_BRIGHTNESS);
            logger.info(`[HealthPosture] 🔅 Đã dim screen về ${DIM_BRIGHTNESS}%`);
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[HealthPosture] Không thể dim screen: ${errMsg}`);
        }

        // TTS reminder
        const ttsMessage = `Sếp ngồi code liên tục ${timeStr} rồi, đứng lên vươn vai uống nước 5 phút đi ạ!`;
        try {
            await this.#deps.speakTTS(ttsMessage);
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[HealthPosture] Lỗi TTS: ${errMsg}`);
        }

        // Push UI toast
        try {
            this.#deps.pushNotification(
                "🧘 Nhắc nghỉ ngơi",
                `Bạn đã làm việc liên tục ${timeStr}. Hãy đứng dậy vươn vai, uống nước và nghỉ mắt ${this.#breakDurationMs / 60_000} phút nhé!`,
            );
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[HealthPosture] Lỗi push notification: ${errMsg}`);
        }

        logger.info(`[HealthPosture] ✅ Đã gửi nhắc nghỉ sau ${timeStr} làm việc liên tục`);
    }
}
