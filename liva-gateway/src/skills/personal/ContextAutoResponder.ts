import { z } from "zod";
import { logger } from "@utils/logger";
import LRUCache from "lru-cache";
import activeWindow from "active-win";

// ─── Zod Schema ────────────────────────────────────────────────────────────────
const AutoResponderSchema = z.object({
    action: z.enum(["enable", "disable", "status", "set_templates", "set_cooldown"])
        .describe("Hành động cấu hình auto-reply"),
    templates: z.record(z.string(), z.string()).optional()
        .describe("Map context → message template"),
    cooldownMinutes: z.number().min(1).max(1440).optional()
        .describe("Cooldown giữa 2 lần tự reply cho cùng 1 sender (phút)"),
});

// ─── Metadata ──────────────────────────────────────────────────────────────────
export const metadata = {
    name: "contextual_auto_responder",
    description: "[AUTO_RUN] Configure automatic reply when busy. Detects current activity (coding, gaming, meeting) and sends context-aware auto-replies to Zalo/Telegram.",
    kit: "PERSONAL_KIT",
    search_keywords: ["auto reply", "tự trả lời", "auto responder", "busy", "bận", "zalo", "telegram"],
    parameters: {
        type: "object",
        properties: {
            action: { type: "string", enum: ["enable", "disable", "status", "set_templates", "set_cooldown"] },
            templates: {
                type: "object",
                description: "Map of context to message template. Keys: vscode, game, meeting, default",
                additionalProperties: { type: "string" },
            },
            cooldownMinutes: { type: "number", description: "Cooldown between auto-replies per sender (minutes), default 120" },
        },
        required: ["action"],
    },
};

// ─── Context Detection Patterns ────────────────────────────────────────────────
/** Patterns để nhận diện context từ window title / owner name */
const CONTEXT_PATTERNS: Array<{ context: string; patterns: RegExp[] }> = [
    {
        context: "vscode",
        patterns: [
            /visual studio code/i,
            /\bvscode\b/i,
            /\bcode\b.*\b(oss|insiders)\b/i,
            /\bcursor\b/i,
            /\bwindsurf\b/i,
            /\bzed\b/i,
            /\bneovim\b/i,
            /\bwebstorm\b/i,
            /\bintelliJ\b/i,
            /\bpycharm\b/i,
        ],
    },
    {
        context: "game",
        patterns: [
            /\bgame\b/i,
            /\bsteam\b/i,
            /\bepic games\b/i,
            /\bleague of legends\b/i,
            /\bvalorant\b/i,
            /\bgenshin\b/i,
            /\bminecraft\b/i,
            /\bdota\b/i,
            /\bcounter.?strike\b/i,
            /\bfortnite\b/i,
            /\broblox\b/i,
            /\boverwatch\b/i,
            /\bapex legends\b/i,
            /\bpubg\b/i,
            /\bunity\b.*\bgame\b/i,
        ],
    },
    {
        context: "meeting",
        patterns: [
            /\bzoom\b/i,
            /\bgoogle meet\b/i,
            /\bmicrosoft teams\b/i,
            /\bteams\b/i,
            /\bwebex\b/i,
            /\bslack\b.*\b(huddle|call)\b/i,
            /\bdiscord\b.*\b(voice|call)\b/i,
            /\bskype\b/i,
        ],
    },
];

// ─── Default Templates ────────────────────────────────────────────────────────
const DEFAULT_TEMPLATES: Record<string, string> = {
    vscode: "Dạ chào bạn, sếp Dương hiện đang trong phiên Deep Work code dự án. Dự kiến 30 phút nữa sếp sẽ check tin nhắn ạ.",
    game: "Dương đang kẹt trong trận rank căng thẳng, lát hết game sẽ rep nha!",
    meeting: "Sếp Dương đang họp, vui lòng để lại lời nhắn.",
    default: "Dương đang bận, sẽ trả lời sớm nhất có thể!",
};

// ─── Default cooldown: 2 hours ─────────────────────────────────────────────────
const DEFAULT_COOLDOWN_MS = 2 * 60 * 60 * 1000;

// ─── AutoResponderEngine Singleton ─────────────────────────────────────────────
class AutoResponderEngine {
    #enabled = false;
    #templates: Record<string, string>;
    #cooldownMs: number;

    /**
     * Passive Circuit Breaker: LRUCache rate limiter.
     * Key = senderId, Value = timestamp of last auto-reply.
     * TTL = cooldown period → tự động hết hạn.
     */
    #rateLimiter: LRUCache<string, number>;

    constructor() {
        this.#templates = { ...DEFAULT_TEMPLATES };
        this.#cooldownMs = DEFAULT_COOLDOWN_MS;
        this.#rateLimiter = new LRUCache<string, number>({
            max: 500,
            ttl: this.#cooldownMs,
        });
    }

    /**
     * Bật auto-responder.
     */
    public enable(): void {
        this.#enabled = true;
        logger.info("[AutoResponder] ✅ Đã bật auto-responder");
    }

    /**
     * Tắt auto-responder.
     */
    public disable(): void {
        this.#enabled = false;
        logger.info("[AutoResponder] 🔴 Đã tắt auto-responder");
    }

    /**
     * Kiểm tra trạng thái enabled.
     */
    public isEnabled(): boolean {
        return this.#enabled;
    }

    /**
     * Cập nhật templates.
     */
    public setTemplates(templates: Record<string, string>): void {
        // Merge với default, cho phép override từng key
        this.#templates = { ...DEFAULT_TEMPLATES, ...templates };
        logger.info(`[AutoResponder] 📝 Đã cập nhật ${Object.keys(templates).length} templates`);
    }

    /**
     * Cập nhật cooldown.
     */
    public setCooldown(minutes: number): void {
        this.#cooldownMs = minutes * 60 * 1000;
        // Tạo lại LRUCache với TTL mới
        this.#rateLimiter = new LRUCache<string, number>({
            max: 500,
            ttl: this.#cooldownMs,
        });
        logger.info(`[AutoResponder] ⏱️ Đã đặt cooldown = ${minutes} phút`);
    }

    /**
     * Lấy auto-reply cho sender dựa trên context hiện tại.
     * 
     * Semantic Cache L0.5: Trả về pre-generated template, KHÔNG gọi LLM.
     * Passive Circuit Breaker: Chặn nếu sender đã nhận reply trong cooldown period.
     * 
     * @param senderId ID/tên người gửi tin nhắn
     * @returns Message string hoặc null nếu không cần reply
     */
    public async getAutoReply(senderId: string): Promise<string | null> {
        // 1. Kiểm tra enabled
        if (!this.#enabled) return null;

        // 2. Passive Circuit Breaker — kiểm tra rate limiter
        const senderKey = senderId.toLowerCase().trim();
        if (this.#rateLimiter.has(senderKey)) {
            logger.debug(`[AutoResponder] 🔒 Cooldown active cho sender '${senderId}', bỏ qua`);
            return null;
        }

        // 3. Detect current context qua active-win
        const context = await this.#detectContext();
        logger.debug(`[AutoResponder] 🔍 Detected context: '${context}' cho sender '${senderId}'`);

        // 4. Lấy template theo context
        const template = this.#templates[context] ?? this.#templates["default"] ?? DEFAULT_TEMPLATES["default"];

        // 5. Set rate limiter cho sender (Passive Circuit Breaker)
        this.#rateLimiter.set(senderKey, Date.now());

        logger.info(`[AutoResponder] 📤 Auto-reply cho '${senderId}' (context: ${context})`);
        return template;
    }

    /**
     * Lấy trạng thái chi tiết.
     */
    public getStatus(): {
        enabled: boolean;
        cooldownMinutes: number;
        templates: Record<string, string>;
        activeCooldowns: number;
    } {
        return {
            enabled: this.#enabled,
            cooldownMinutes: Math.round(this.#cooldownMs / 60_000),
            templates: { ...this.#templates },
            activeCooldowns: this.#rateLimiter.size,
        };
    }

    /**
     * Nhận diện context hiện tại qua active-win.
     * Trả về key tương ứng trong CONTEXT_PATTERNS hoặc "default".
     */
    async #detectContext(): Promise<string> {
        try {
            const win = await activeWindow();
            if (!win) return "default";

            const titleAndOwner = `${win.title} ${win.owner?.name ?? ""}`;

            for (const { context, patterns } of CONTEXT_PATTERNS) {
                for (const pattern of patterns) {
                    if (pattern.test(titleAndOwner)) {
                        return context;
                    }
                }
            }

            return "default";
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[AutoResponder] Lỗi detect context: ${errMsg}`);
            return "default";
        }
    }
}

// ─── Module-level Singleton ────────────────────────────────────────────────────
/** Export singleton cho Gateway listeners sử dụng */
export const autoResponderEngine = new AutoResponderEngine();

// ─── Execute (LLM-callable) ───────────────────────────────────────────────────
export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = AutoResponderSchema.parse(argsObj);

        switch (parsed.action) {
            case "enable": {
                autoResponderEngine.enable();
                return `[AUTORESPONDER SUCCESS] Đã bật auto-responder. Tin nhắn đến sẽ được tự động trả lời dựa trên context hiện tại.`;
            }

            case "disable": {
                autoResponderEngine.disable();
                return `[AUTORESPONDER SUCCESS] Đã tắt auto-responder.`;
            }

            case "status": {
                const status = autoResponderEngine.getStatus();
                const templateList = Object.entries(status.templates)
                    .map(([key, val]) => `  • ${key}: "${val}"`)
                    .join("\n");

                return [
                    `[AUTORESPONDER SUCCESS] Trạng thái auto-responder:`,
                    `  Enabled: ${status.enabled ? "✅ BẬT" : "🔴 TẮT"}`,
                    `  Cooldown: ${status.cooldownMinutes} phút`,
                    `  Sender đang trong cooldown: ${status.activeCooldowns}`,
                    `  Templates:`,
                    templateList,
                ].join("\n");
            }

            case "set_templates": {
                if (!parsed.templates || Object.keys(parsed.templates).length === 0) {
                    return `[AUTORESPONDER ERROR] Thiếu tham số 'templates'. Cần truyền object map context → message.`;
                }
                autoResponderEngine.setTemplates(parsed.templates);
                const keys = Object.keys(parsed.templates).join(", ");
                return `[AUTORESPONDER SUCCESS] Đã cập nhật templates cho: ${keys}`;
            }

            case "set_cooldown": {
                if (parsed.cooldownMinutes === undefined) {
                    return `[AUTORESPONDER ERROR] Thiếu tham số 'cooldownMinutes'.`;
                }
                autoResponderEngine.setCooldown(parsed.cooldownMinutes);
                return `[AUTORESPONDER SUCCESS] Đã đặt cooldown = ${parsed.cooldownMinutes} phút giữa 2 lần auto-reply cho cùng sender.`;
            }

            default:
                return `[AUTORESPONDER ERROR] Hành động không hợp lệ.`;
        }
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[AutoResponder] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[AUTORESPONDER ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[AUTORESPONDER ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
