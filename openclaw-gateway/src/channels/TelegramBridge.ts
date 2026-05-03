/**
 * TelegramBridge — Telegram Bot Long-Polling Integration (Phase 1)
 * =================================================================
 * Connects LIVA to Telegram using HTTP Long-Polling (no Webhook needed).
 * Features:
 *   - Exponential backoff on connection failures
 *   - Inline Keyboard support for HITL approval flow
 *   - Callback query handling for approve/reject buttons
 *   - Screenshot forwarding via sendPhoto API
 *   - Whitelist-based sender validation
 *
 * [v5.0] LIVA Remote Control Hub
 */

import { EventEmitter } from "node:events";
import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";
import type { ChannelAdapter, NormalizedMessage } from "./ChannelNormalizer";

// ===========================
// Telegram API Types
// ===========================

interface TelegramUpdate {
    update_id: number;
    message?: {
        message_id: number;
        from: { id: number; first_name: string; username?: string };
        chat: { id: number; type: string };
        text?: string;
        photo?: Array<{ file_id: string; width: number; height: number }>;
        date: number;
    };
    callback_query?: {
        id: string;
        from: { id: number; first_name: string };
        data: string;
        message?: { chat: { id: number }; message_id: number };
    };
}

interface InlineKeyboardButton {
    text: string;
    callback_data: string;
}

// ===========================
// TelegramBridge
// ===========================

export class TelegramBridge extends EventEmitter implements ChannelAdapter {
    public readonly channelName = "telegram" as const;

    readonly #botToken: string;
    readonly #allowedIds: Set<string>;
    readonly #apiBase: string;

    #isPolling = false;
    #lastUpdateId = 0;
    #pollTimerRef: ReturnType<typeof setTimeout> | null = null;
    #backoffMs = 1000;
    readonly #maxBackoff = 30_000;

    constructor() {
        super();
        this.#botToken = process.env.TELEGRAM_BOT_TOKEN || "";
        this.#apiBase = `https://api.telegram.org/bot${this.#botToken}`;

        // Parse comma-separated allowed IDs
        const allowedRaw = process.env.TELEGRAM_ALLOWED_IDS || "";
        this.#allowedIds = new Set(
            allowedRaw.split(",").map(id => id.trim()).filter(Boolean)
        );

        if (!this.#botToken) {
            logger.warn("⚠️ [Telegram] TELEGRAM_BOT_TOKEN chưa cấu hình. Bridge tạm tắt.");
        } else {
            logger.info(`📡 [Telegram] Bridge khởi tạo. Whitelist: ${this.#allowedIds.size} IDs.`);
        }
    }

    // ═══════════════════════════════════════
    //  Lifecycle
    // ═══════════════════════════════════════

    public async startPolling(): Promise<void> {
        if (!this.#botToken) return;
        this.#isPolling = true;
        this.#backoffMs = 1000;
        logger.info("📡 [Telegram] Bắt đầu Long-Polling...");
        this.#poll();
    }

    public stop(): void {
        this.#isPolling = false;
        if (this.#pollTimerRef) {
            clearTimeout(this.#pollTimerRef);
            this.#pollTimerRef = null;
        }
        logger.info("⚠️ [Telegram] Bridge đã dừng.");
    }

    // ═══════════════════════════════════════
    //  Polling Loop (Exponential Backoff)
    // ═══════════════════════════════════════

    async #poll(): Promise<void> {
        /* istanbul ignore if */
        if (!this.#isPolling) return;

        try {
            let offsetParam: number | undefined = undefined;
            /* istanbul ignore if */
            if (this.#lastUpdateId > 0) {
                offsetParam = this.#lastUpdateId;
            }

            const res = await safeFetch(
                `${this.#apiBase}/getUpdates`,
                {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        offset: offsetParam,
                        timeout: 30, // Long-polling: Telegram holds connection 30s
                        allowed_updates: ["message", "callback_query"],
                    }),
                },
                35_000 // 35s timeout (> 30s Telegram hold)
            );

            const data = await res.json() as { ok: boolean; result?: TelegramUpdate[] };

            /* istanbul ignore next */
            if (data.ok && Array.isArray(data.result)) {
                for (const update of data.result) {
                    this.#lastUpdateId = Math.max(this.#lastUpdateId, update.update_id + 1);
                    this.#handleUpdate(update);
                }
            }

            // Success — reset backoff
            this.#backoffMs = 1000;
        } catch (e: any) {
            if (e.name !== "AbortError") {
                logger.error(`[Telegram] Polling error: ${e.message}. Retry in ${this.#backoffMs}ms`);
            }
            // Exponential backoff
            this.#backoffMs = Math.min(this.#backoffMs * 2, this.#maxBackoff);
        }

        // Schedule next poll
        if (this.#isPolling) {
            this.#pollTimerRef = setTimeout(() => this.#poll(), this.#backoffMs === 1000 ? 100 : this.#backoffMs);
        }
    }

    // ═══════════════════════════════════════
    //  Update Handler
    // ═══════════════════════════════════════

    #handleUpdate(update: TelegramUpdate): void {
        // Handle text messages
        if (update.message?.text) {
            const senderId = String(update.message.from.id);

            // Whitelist check
            if (this.#allowedIds.size > 0 && !this.#allowedIds.has(senderId)) {
                logger.warn(`[Telegram] 🛡️ Blocked unauthorized sender: ${senderId}`);
                return;
            }

            const normalized: NormalizedMessage = {
                channel: "telegram",
                senderId,
                senderName: update.message.from.first_name,
                text: update.message.text,
                rawPayload: update,
                timestamp: update.message.date * 1000,
            };

            // Handle image attachments
            if (update.message.photo && update.message.photo.length > 0) {
                const largest = update.message.photo[update.message.photo.length - 1];
                normalized.mediaUrl = largest.file_id;
                normalized.mediaType = "image";
            }

            logger.info(`💬 [Telegram] Tin nhắn từ ${normalized.senderName}: "${normalized.text}"`);
            this.emit("message", normalized);
        }

        // Handle callback queries (approval buttons)
        if (update.callback_query) {
            const query = update.callback_query;
            const senderId = String(query.from.id);

            if (this.#allowedIds.size > 0 && !this.#allowedIds.has(senderId)) return;

            logger.info(`🔘 [Telegram] Callback query: ${query.data} from ${query.from.first_name}`);
            this.emit("callback_query", {
                queryId: query.id,
                senderId,
                data: query.data,
                chatId: query.message?.chat.id,
                messageId: query.message?.message_id,
            });

            // Answer callback to remove loading state
            this.#answerCallbackQuery(query.id).catch(() => {});
        }
    }

    // ═══════════════════════════════════════
    //  Outbound API Methods
    // ═══════════════════════════════════════

    public async sendText(chatId: string, text: string): Promise<void> {
        await this.#apiCall("sendMessage", {
            chat_id: chatId,
            text: text.substring(0, 4096), // Telegram limit
            parse_mode: "Markdown",
        });
    }

    /**
     * Send an approval card with Approve/Reject inline keyboard.
     * Used by ApprovalEngine for Human-in-the-Loop flow.
     */
    public async sendApprovalCard(
        senderId: string,
        title: string,
        body: string,
        approvalId: string
    ): Promise<void> {
        const keyboard: InlineKeyboardButton[][] = [
            [
                { text: "✅ Approve", callback_data: `approve:${approvalId}` },
                { text: "❌ Reject", callback_data: `reject:${approvalId}` },
            ],
        ];

        await this.#apiCall("sendMessage", {
            chat_id: senderId,
            text: `🔐 *${title}*\n\n\`\`\`\n${body.substring(0, 3500)}\n\`\`\``,
            parse_mode: "Markdown",
            reply_markup: JSON.stringify({ inline_keyboard: keyboard }),
        });
    }

    /**
     * Send a screenshot/image to a chat.
     */
    public async sendScreenshot(chatId: string, imageBuffer: Buffer): Promise<void> {
        // Use multipart/form-data for file upload
        const boundary = `----LIVABoundary${Date.now()}`;
        const body = Buffer.concat([
            Buffer.from(
                `--${boundary}\r\n` +
                `Content-Disposition: form-data; name="chat_id"\r\n\r\n${chatId}\r\n` +
                `--${boundary}\r\n` +
                `Content-Disposition: form-data; name="photo"; filename="screenshot.png"\r\n` +
                `Content-Type: image/png\r\n\r\n`
            ),
            imageBuffer,
            Buffer.from(`\r\n--${boundary}--\r\n`),
        ]);

        await safeFetch(
            `${this.#apiBase}/sendPhoto`,
            {
                method: "POST",
                headers: { "Content-Type": `multipart/form-data; boundary=${boundary}` },
                body,
            },
            15_000
        );
    }

    /**
     * Edit an existing message (e.g., update approval status).
     */
    public async editMessage(chatId: string, messageId: number, text: string): Promise<void> {
        await this.#apiCall("editMessageText", {
            chat_id: chatId,
            message_id: messageId,
            text,
            parse_mode: "Markdown",
        });
    }

    // ═══════════════════════════════════════
    //  Private Helpers
    // ═══════════════════════════════════════

    async #apiCall(method: string, params: Record<string, any>): Promise<any> {
        try {
            const res = await safeFetch(
                `${this.#apiBase}/${method}`,
                {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify(params),
                },
                10_000
            );
            return res.json();
        } catch (e: any) {
            logger.error(`[Telegram API] ${method} failed: ${e.message}`);
            throw e;
        }
    }

    async #answerCallbackQuery(queryId: string): Promise<void> {
        await this.#apiCall("answerCallbackQuery", { callback_query_id: queryId });
    }
}
