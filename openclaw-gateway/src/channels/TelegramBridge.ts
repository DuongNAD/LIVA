import { EventEmitter } from "node:events";
import { Telegraf } from "telegraf";
import { logger } from "../utils/logger";
import type { ChannelAdapter, NormalizedMessage } from "./ChannelNormalizer";
import { TelegramCommandHandler } from "./TelegramCommandHandler";
import { CDPBridge } from "../bridges/CDPBridge";

export class TelegramBridge extends EventEmitter implements ChannelAdapter {
    public readonly channelName = "telegram" as const;

    readonly #botToken: string;
    readonly #allowedIds: Set<string>;
    #bot: Telegraf | null = null;
    #commandHandler: TelegramCommandHandler;
    #cdpBridge: CDPBridge | null = null;
    #isPolling = false;

    constructor() {
        super();
        this.#botToken = process.env.TELEGRAM_BOT_TOKEN || "";
        const allowedRaw = process.env.TELEGRAM_ALLOWED_IDS || "";
        this.#allowedIds = new Set(
            allowedRaw.split(",").map(id => id.trim()).filter(Boolean)
        );

        this.#commandHandler = new TelegramCommandHandler();

        if (!this.#botToken) {
            logger.warn("⚠️ [Telegram] TELEGRAM_BOT_TOKEN chưa cấu hình. Bridge tạm tắt.");
        } else {
            this.#bot = new Telegraf(this.#botToken, { handlerTimeout: 900_000 });
            this.#setupBot();
            logger.info(`📡 [Telegram] Bridge khởi tạo (Telegraf). Whitelist: ${this.#allowedIds.size} IDs.`);
        }
    }

    public setBridges(cdpBridge: CDPBridge, autoAcceptDaemon?: any): void {
        this.#cdpBridge = cdpBridge;
        if (this.#bot) {
            this.#commandHandler.registerHandlers(this.#bot, this.#cdpBridge, autoAcceptDaemon);
        }
    }

    #setupBot(): void {
        // 1. Auth Middleware (ZMAS Guard)
        this.#bot.use((ctx, next) => {
            const senderId = String(ctx.from?.id);
            if (this.#allowedIds.size > 0 && !this.#allowedIds.has(senderId)) {
                logger.warn(`[Telegram] 🛡️ Blocked unauthorized sender: ${senderId}`);
                return; // drop silently or ctx.reply("⛔ Unauthorized")
            }
            return next();
        });

        // 2. Handle standard text messages (pass to AgentLoop)
        this.#bot.on("text", (ctx, next) => {
            // If it's a command, let command handlers handle it
            if (ctx.message.text.startsWith("/")) {
                return next();
            }

            const normalized: NormalizedMessage = {
                channel: "telegram",
                senderId: String(ctx.from.id),
                senderName: ctx.from.first_name,
                text: ctx.message.text,
                rawPayload: ctx.update,
                timestamp: ctx.message.date * 1000,
            };
            
            logger.info(`💬 [Telegram] Tin nhắn từ ${normalized.senderName}: "${normalized.text}"`);
            this.emit("message", normalized);
        });

        // 3. Handle photos
        this.#bot.on("photo", (ctx) => {
            const normalized: NormalizedMessage = {
                channel: "telegram",
                senderId: String(ctx.from.id),
                senderName: ctx.from.first_name,
                text: ctx.message.caption || "",
                rawPayload: ctx.update,
                timestamp: ctx.message.date * 1000,
            };
            const largest = ctx.message.photo[ctx.message.photo.length - 1];
            normalized.mediaUrl = largest.file_id;
            normalized.mediaType = "image";
            
            logger.info(`💬 [Telegram] Hình ảnh từ ${normalized.senderName}`);
            this.emit("message", normalized);
        });

        // 4. Handle Callback Queries (Approve/Reject buttons)
        this.#bot.on("callback_query", async (ctx) => {
            // @ts-ignore - telegraf types for callback query data
            const data = ctx.callbackQuery.data;
            if (!data) return;

            logger.info(`🔘 [Telegram] Callback query: ${data} from ${ctx.from.first_name}`);
            this.emit("callback_query", {
                queryId: ctx.callbackQuery.id,
                senderId: String(ctx.from.id),
                data: data,
                chatId: ctx.chat?.id,
                messageId: ctx.callbackQuery.message?.message_id,
            });

            try {
                await ctx.answerCbQuery();
            } catch (e) {
                // ignore
            }
        });
    }

    public async startPolling(): Promise<void> {
        if (!this.#bot || this.#isPolling) return;
        try {
            await this.#bot.launch({ dropPendingUpdates: true });
            this.#isPolling = true;
            logger.info("📡 [Telegram] Bắt đầu Long-Polling (Telegraf)...");
        } catch (e: any) {
            logger.error(`[Telegram] Polling error: ${e.message}. Auto-reconnect in 10s...`);
            setTimeout(() => this.startPolling(), 10_000);
        }
    }

    public stop(): void {
        if (!this.#isPolling || !this.#bot) return;
        this.#isPolling = false;
        this.#bot.stop("SIGTERM");
        logger.info("⚠️ [Telegram] Bridge đã dừng.");
    }

    // ═══════════════════════════════════════
    //  Outbound API Methods (ChannelAdapter)
    // ═══════════════════════════════════════

    public async sendText(chatId: string, text: string): Promise<void> {
        if (!this.#bot) return;
        await this.#bot.telegram.sendMessage(chatId, text.substring(0, 4096), {
            parse_mode: "Markdown",
        });
    }

    public async sendApprovalCard(
        senderId: string,
        title: string,
        body: string,
        approvalId: string
    ): Promise<void> {
        if (!this.#bot) return;
        const inlineKeyboard = [
            [
                { text: "✅ Approve", callback_data: `approve:${approvalId}` },
                { text: "❌ Reject", callback_data: `reject:${approvalId}` },
            ],
        ];

        await this.#bot.telegram.sendMessage(
            senderId,
            `🔐 *${title}*\n\n\`\`\`\n${body.substring(0, 3500)}\n\`\`\``,
            {
                parse_mode: "Markdown",
                reply_markup: { inline_keyboard: inlineKeyboard },
            }
        );
    }

    public async sendScreenshot(chatId: string, imageBuffer: Buffer): Promise<void> {
        if (!this.#bot) return;
        await this.#bot.telegram.sendPhoto(chatId, {
            source: imageBuffer,
            filename: "screenshot.png"
        });
    }

    public async editMessage(chatId: string, messageId: number, text: string): Promise<void> {
        if (!this.#bot) return;
        try {
            await this.#bot.telegram.editMessageText(
                chatId,
                messageId,
                undefined,
                text.substring(0, 4096),
                { parse_mode: "Markdown" }
            );
        } catch (e) {
            // Ignore "message is not modified" errors
        }
    }
}
