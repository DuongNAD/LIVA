import { safeFetch } from "../utils/HttpClient";
import { logger } from "../utils/logger";

export interface InlineKeyboardButton {
    text: string;
    callback_data: string;
}

export class TelegramManager {
    private botToken: string;
    private chatId: string;

    constructor() {
        this.botToken = process.env.TELEGRAM_BOT_TOKEN || "";
        this.chatId = process.env.TELEGRAM_CHAT_ID || "";
    }

    public async sendMessage(text: string, inlineKeyboard?: InlineKeyboardButton[][]): Promise<number | null> {
        if (!this.botToken || !this.chatId) {
            logger.warn("[TelegramManager] Thiếu TELEGRAM_BOT_TOKEN hoặc TELEGRAM_CHAT_ID, bỏ qua gửi tin nhắn.");
            return null;
        }

        const url = `https://api.telegram.org/bot${this.botToken}/sendMessage`;
        const body: any = {
            chat_id: this.chatId,
            text: text,
            parse_mode: "Markdown"
        };

        if (inlineKeyboard && inlineKeyboard.length > 0) {
            body.reply_markup = {
                inline_keyboard: inlineKeyboard
            };
        }

        const executePost = async (): Promise<number | null> => {
            const res = await safeFetch(url, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(body)
            }, 10000);
            const data = await res.json() as any;
            if (data.ok && data.result) {
                return data.result.message_id;
            }
            return null;
        };

        try {
            return await executePost();
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            const causeMsg = (e instanceof Error && e.cause instanceof Error) ? e.cause.message : errMsg;
            
            // Lọc và xử lý cấu trúc Retry-After nếu bị Rate Limit HTTP 429
            if (errMsg.includes("HTTP 429")) {
                const match = errMsg.match(/\{.*\}/);
                if (match) {
                    try {
                        const tgErr = JSON.parse(match[0]);
                        if (tgErr.parameters && tgErr.parameters.retry_after) {
                            const retryAfter = tgErr.parameters.retry_after;
                            logger.warn(`[TelegramManager] Bị Rate Limit HTTP 429. Chờ Retry-After: ${retryAfter}s`);
                            await new Promise(res => setTimeout(res, retryAfter * 1000));
                            // Thử lại lần 1
                            return await executePost();
                        }
                    } catch (parseErr) {
                        // Bỏ qua lỗi parse JSON nếu body không đúng chuẩn
                    }
                }
            }

            logger.error(`[TelegramManager] Lỗi gửi Telegram: ${errMsg}`);
            throw new Error(`TelegramManager Error: ${errMsg}`);
        }
    }

    public async editMessage(messageId: number, text: string, inlineKeyboard?: InlineKeyboardButton[][]): Promise<void> {
        if (!this.botToken || !this.chatId) return;

        const url = `https://api.telegram.org/bot${this.botToken}/editMessageText`;
        const body: any = {
            chat_id: this.chatId,
            message_id: messageId,
            text: text,
            parse_mode: "Markdown"
        };

        if (inlineKeyboard && inlineKeyboard.length > 0) {
            body.reply_markup = {
                inline_keyboard: inlineKeyboard
            };
        }

        try {
            await safeFetch(url, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(body)
            }, 10000);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            const causeMsg = (e instanceof Error && e.cause instanceof Error) ? e.cause.message : errMsg;
            if (causeMsg.includes("message is not modified")) {
                return;
            }
            logger.error(`[TelegramManager] Lỗi sửa tin nhắn Telegram: ${errMsg}`);
        }
    }
}
