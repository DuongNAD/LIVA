import { z } from "zod";
import { logger } from "@utils/logger";
import { DatabaseSync } from "node:sqlite";
import { randomUUID } from "node:crypto";
import * as path from "node:path";
import * as fs from "node:fs";

// ============================================================
// Zod Schema
// ============================================================

const MessageSchedulerSchema = z.object({
    action: z.enum(["schedule", "cancel", "list"]).describe("Hành động: lên lịch, hủy, hoặc liệt kê tin nhắn"),
    channel: z.enum(["zalo", "telegram", "email"]).optional().describe("Kênh gửi tin nhắn"),
    target: z.string().optional().describe("Người nhận (tên hoặc ID)"),
    message: z.string().optional().describe("Nội dung tin nhắn"),
    scheduledTime: z.string().optional().describe("Thời gian gửi (ISO 8601 hoặc HH:MM hoặc '8h sáng mai')"),
    messageId: z.string().optional().describe("ID tin nhắn cần hủy"),
});

// ============================================================
// Metadata (LLM function calling)
// ============================================================

export const metadata = {
    name: "message_scheduler",
    description: "[AUTO_RUN] Schedule messages to be sent at a specific time. Supports Zalo, Telegram, Email channels. Messages persist across restarts via SQLite.",
    kit: "PERSONAL_KIT",
    search_keywords: ["schedule", "message", "hẹn giờ", "gửi tin", "lên lịch", "nhắn tin", "zalo", "telegram", "email"],
    parameters: {
        type: "object",
        properties: {
            action: { type: "string", enum: ["schedule", "cancel", "list"], description: "Action to perform" },
            channel: { type: "string", enum: ["zalo", "telegram", "email"], description: "Message channel" },
            target: { type: "string", description: "Recipient name or ID" },
            message: { type: "string", description: "Message content" },
            scheduledTime: { type: "string", description: "When to send (ISO 8601, HH:MM, or Vietnamese time expression)" },
            messageId: { type: "string", description: "Message ID to cancel" },
        },
        required: ["action"],
    },
};

// ============================================================
// Types
// ============================================================

interface ScheduledMessage {
    id: string;
    channel: string;
    target: string;
    message: string;
    scheduled_at: number;
    status: string;
    created_at: number;
}

// ============================================================
// Send handler type — wired externally by BootstrapManager
// ============================================================

type SendHandler = (channel: string, target: string, message: string) => Promise<boolean>;

// ============================================================
// ScheduledMessageQueue — SQLite-backed persistent scheduler
// ============================================================

class ScheduledMessageQueue {
    #db: DatabaseSync | null = null;
    #dbPath: string;
    #intervalRef: ReturnType<typeof setInterval> | null = null;
    #sendHandler: SendHandler | null = null;

    constructor(dbPath?: string) {
        this.#dbPath = dbPath || path.join(process.cwd(), "data", "agents", "liva_core", "scheduled_messages.db");
    }

    // ---- Lazy DB initialization (same pattern as PersistentQueue) ----

    #ensureDb(): DatabaseSync {
        if (this.#db) return this.#db;

        const dir = path.dirname(this.#dbPath);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }

        this.#db = new DatabaseSync(this.#dbPath);
        this.#db.exec("PRAGMA journal_mode = WAL");
        this.#db.exec("PRAGMA synchronous = NORMAL");
        this.#db.exec(`
            CREATE TABLE IF NOT EXISTS scheduled_messages (
                id TEXT PRIMARY KEY,
                channel TEXT NOT NULL,
                target TEXT NOT NULL,
                message TEXT NOT NULL,
                scheduled_at INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            )
        `);

        logger.info(`[MessageScheduler] SQLite initialized at ${this.#dbPath}`);
        return this.#db;
    }

    // ---- Public API ----

    /**
     * Register an external send handler (wired by BootstrapManager).
     * This decouples the scheduler from SendZaloRPA/TelegramManager.
     */
    public setSendHandler(handler: SendHandler): void {
        this.#sendHandler = handler;
        logger.info("[MessageScheduler] Send handler registered.");
    }

    /**
     * Schedule a message for future delivery.
     * @returns The unique message ID
     */
    public schedule(channel: string, target: string, message: string, scheduledAt: Date): string {
        const db = this.#ensureDb();
        const id = `msg_${randomUUID().substring(0, 8)}`;
        const stmt = db.prepare(
            "INSERT INTO scheduled_messages (id, channel, target, message, scheduled_at) VALUES (?, ?, ?, ?, ?)"
        );
        stmt.run(id, channel, target, message, Math.floor(scheduledAt.getTime() / 1000));
        logger.info(`[MessageScheduler] Scheduled message ${id} → ${channel}:${target} at ${scheduledAt.toISOString()}`);
        return id;
    }

    /**
     * Cancel a pending scheduled message.
     */
    public cancel(id: string): boolean {
        const db = this.#ensureDb();
        const result = db.prepare(
            "UPDATE scheduled_messages SET status = 'cancelled' WHERE id = ? AND status = 'pending'"
        ).run(id);
        const changed = (result as any).changes ?? 0;
        if (changed > 0) {
            logger.info(`[MessageScheduler] Cancelled message ${id}`);
            return true;
        }
        return false;
    }

    /**
     * List all pending scheduled messages.
     */
    public listPending(): ScheduledMessage[] {
        const db = this.#ensureDb();
        return db.prepare(
            "SELECT * FROM scheduled_messages WHERE status = 'pending' ORDER BY scheduled_at ASC"
        ).all() as unknown as ScheduledMessage[];
    }

    // ---- Background Worker ----

    /**
     * Start the background tick that checks for due messages every 30 seconds.
     */
    public start(): void {
        if (this.#intervalRef) return;

        this.#intervalRef = setInterval(() => {
            this.#processDueMessages().catch(err => {
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[MessageScheduler] Worker tick error: ${msg}`);
            });
        }, 30_000);
        this.#intervalRef.unref(); // Prevent zombie timer

        logger.info("[MessageScheduler] ⏰ Background worker started (tick: 30s).");
    }

    /**
     * Process all due messages.
     */
    async #processDueMessages(): Promise<void> {
        const db = this.#ensureDb();
        const nowUnix = Math.floor(Date.now() / 1000);

        const dueMessages = db.prepare(
            "SELECT * FROM scheduled_messages WHERE scheduled_at <= ? AND status = 'pending'"
        ).all(nowUnix) as unknown as ScheduledMessage[];

        if (dueMessages.length === 0) return;

        logger.info(`[MessageScheduler] 📨 Processing ${dueMessages.length} due message(s)...`);

        const updateStmt = db.prepare("UPDATE scheduled_messages SET status = ? WHERE id = ?");

        for (const msg of dueMessages) {
            updateStmt.run("sending", msg.id);

            try {
                if (this.#sendHandler) {
                    const success = await this.#sendHandler(msg.channel, msg.target, msg.message);
                    if (success) {
                        updateStmt.run("sent", msg.id);
                        logger.info(`[MessageScheduler] ✅ Sent ${msg.id} → ${msg.channel}:${msg.target}`);
                    } else {
                        updateStmt.run("failed", msg.id);
                        logger.warn(`[MessageScheduler] ❌ Send failed for ${msg.id} (handler returned false)`);
                    }
                } else {
                    // No handler wired — log placeholder and mark as sent
                    logger.info(`[MessageScheduler] 📋 (No send handler) Would send to ${msg.channel}:${msg.target}: "${msg.message.substring(0, 80)}"`);
                    updateStmt.run("sent", msg.id);
                }

                // Push IPC notification to UI
                const ipcPayload = JSON.stringify({
                    event: "SHOW_TOAST",
                    payload: {
                        title: `📨 Tin nhắn đã gửi (${msg.channel})`,
                        message: `Gửi đến ${msg.target}: "${msg.message.substring(0, 60)}"`,
                        type: "info",
                        duration: 8000,
                    },
                });
                process.stdout.write(ipcPayload + "\n");
            } catch (err: unknown) {
                const errMsg = err instanceof Error ? err.message : String(err);
                updateStmt.run("failed", msg.id);
                logger.error(`[MessageScheduler] ❌ Send error for ${msg.id}: ${errMsg}`);
            }
        }
    }

    /**
     * Dispose: stop background worker and close SQLite.
     */
    public dispose(): void {
        if (this.#intervalRef) {
            clearInterval(this.#intervalRef);
            this.#intervalRef = null;
        }
        if (this.#db) {
            try {
                this.#db.close();
                this.#db = null;
                logger.info("[MessageScheduler] Database closed.");
            } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e);
                logger.error(`[MessageScheduler] Close failed: ${errMsg}`);
            }
        }
        logger.info("[MessageScheduler] 🛑 Disposed.");
    }
}

// ============================================================
// Singleton — exported for external use (BootstrapManager, etc.)
// ============================================================

export const scheduledMessageQueue = new ScheduledMessageQueue();

// ============================================================
// Vietnamese Time Parser
// ============================================================

/**
 * Parse flexible time expressions into a Date object.
 * Supports:
 *   - ISO 8601: "2026-05-22T14:30:00+07:00"
 *   - HH:MM format: "14:30" (today or tomorrow if already passed)
 *   - Vietnamese expressions: "8h sáng mai", "2h chiều", "9h tối nay", "7h30 sáng"
 */
function parseScheduledTime(input: string): Date {
    // 1. Try ISO 8601 first
    const isoDate = new Date(input);
    if (!isNaN(isoDate.getTime()) && input.includes("-")) {
        return isoDate;
    }

    const now = new Date();

    // 2. Try HH:MM format (e.g., "14:30", "08:00")
    const hhmmMatch = input.match(/^(\d{1,2}):(\d{2})$/);
    if (hhmmMatch) {
        const hour = parseInt(hhmmMatch[1], 10);
        const minute = parseInt(hhmmMatch[2], 10);
        const target = new Date(now);
        target.setHours(hour, minute, 0, 0);

        // If time already passed today, schedule for tomorrow
        if (target.getTime() <= now.getTime()) {
            target.setDate(target.getDate() + 1);
        }
        return target;
    }

    // 3. Parse Vietnamese time expressions
    const vnInput = input.toLowerCase().trim();

    // Extract hour and optional minutes: "8h", "8h30", "14h", "2h30"
    const hourMatch = vnInput.match(/(\d{1,2})h(\d{2})?/);
    if (!hourMatch) {
        throw new Error(`Không thể phân tích thời gian: "${input}". Hãy dùng định dạng HH:MM hoặc ISO 8601.`);
    }

    let hour = parseInt(hourMatch[1], 10);
    const minute = hourMatch[2] ? parseInt(hourMatch[2], 10) : 0;

    // Detect period: sáng (morning), chiều/trưa (afternoon), tối/đêm (evening/night)
    const isAfternoon = /chiều|trưa/.test(vnInput);
    const isEvening = /tối|đêm/.test(vnInput);

    if (isAfternoon && hour < 12) {
        hour += 12;
    } else if (isEvening && hour < 12) {
        hour += 12;
    }

    // Detect day: "mai" (tomorrow), "ngày mai" (tomorrow), "nay" (today)
    const isTomorrow = /mai|ngày mai/.test(vnInput);

    const target = new Date(now);
    target.setHours(hour, minute, 0, 0);

    if (isTomorrow) {
        target.setDate(target.getDate() + 1);
    } else if (target.getTime() <= now.getTime()) {
        // "nay" or no day specified — if already passed, push to tomorrow
        target.setDate(target.getDate() + 1);
    }

    return target;
}

// ============================================================
// Execute function (LLM-callable)
// ============================================================

export const execute = async (argsObj: unknown): Promise<string> => {
    try {
        const parsed = MessageSchedulerSchema.parse(argsObj);

        if (parsed.action === "list") {
            const pending = scheduledMessageQueue.listPending();
            if (pending.length === 0) {
                return "[SCHEDULER INFO] Không có tin nhắn nào đang chờ gửi.";
            }
            let out = `[SCHEDULER INFO] Có ${pending.length} tin nhắn đang chờ gửi:\n`;
            for (const [i, msg] of pending.entries()) {
                const scheduledDate = new Date(msg.scheduled_at * 1000);
                const timeStr = scheduledDate.toLocaleString("vi-VN", {
                    timeZone: "Asia/Ho_Chi_Minh",
                    hour: "2-digit",
                    minute: "2-digit",
                    day: "2-digit",
                    month: "2-digit",
                });
                out += `${i + 1}. [${msg.id}] ${msg.channel} → ${msg.target} lúc ${timeStr}: "${msg.message.substring(0, 60)}"\n`;
            }
            return out;
        }

        if (parsed.action === "cancel") {
            if (!parsed.messageId) {
                return "[SCHEDULER ERROR] Cần cung cấp 'messageId' để hủy tin nhắn.";
            }
            const success = scheduledMessageQueue.cancel(parsed.messageId);
            if (success) {
                return `[SCHEDULER SUCCESS] Đã hủy tin nhắn ${parsed.messageId}.`;
            }
            return `[SCHEDULER ERROR] Không tìm thấy tin nhắn pending với ID: ${parsed.messageId}.`;
        }

        if (parsed.action === "schedule") {
            if (!parsed.channel) {
                return "[SCHEDULER ERROR] Cần chỉ định 'channel' (zalo, telegram, email).";
            }
            if (!parsed.target) {
                return "[SCHEDULER ERROR] Cần chỉ định 'target' (người nhận).";
            }
            if (!parsed.message) {
                return "[SCHEDULER ERROR] Cần cung cấp 'message' (nội dung tin nhắn).";
            }
            if (!parsed.scheduledTime) {
                return "[SCHEDULER ERROR] Cần cung cấp 'scheduledTime' (thời gian gửi).";
            }

            const scheduledAt = parseScheduledTime(parsed.scheduledTime);
            const id = scheduledMessageQueue.schedule(parsed.channel, parsed.target, parsed.message, scheduledAt);

            const timeStr = scheduledAt.toLocaleString("vi-VN", {
                timeZone: "Asia/Ho_Chi_Minh",
                weekday: "long",
                hour: "2-digit",
                minute: "2-digit",
                day: "2-digit",
                month: "2-digit",
            });

            return `[SCHEDULER SUCCESS] Đã lên lịch gửi tin nhắn qua ${parsed.channel} đến "${parsed.target}" vào ${timeStr}.\nMessage ID: ${id} (dùng để hủy nếu cần).`;
        }

        return "[SCHEDULER ERROR] Hành động không hợp lệ.";
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[MessageScheduler] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[SCHEDULER ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[SCHEDULER ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};

/**
 * Dispose function — called by SkillRegistry on shutdown.
 */
export const dispose = (): void => {
    scheduledMessageQueue.dispose();
};
