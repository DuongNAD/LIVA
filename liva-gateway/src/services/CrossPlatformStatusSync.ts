import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";

// ─── Status Types ────────────────────────────────────────────────────────────

type UserStatus = "coding" | "meeting" | "gaming" | "deepwork" | "afk" | "online";

interface StatusDisplay {
    emoji: string;
    text: string;
}

const STATUS_MAP: Record<UserStatus, StatusDisplay> = {
    coding: { emoji: "🟢", text: "Đang fix bug" },
    meeting: { emoji: "🔴", text: "In a Meeting" },
    gaming: { emoji: "🎮", text: "Gaming" },
    deepwork: { emoji: "🔕", text: "Deep Work - DND" },
    afk: { emoji: "🌙", text: "Away (AFK)" },
    online: { emoji: "🟢", text: "Online" },
};

// ─── IDE & Game keywords for window title matching ───────────────────────────

const IDE_KEYWORDS = [
    "visual studio code",
    "vscode",
    "code -",
    "code.exe",
    "intellij",
    "webstorm",
    "phpstorm",
    "pycharm",
    "rider",
    "goland",
    "clion",
    "datagrip",
    "android studio",
    "neovim",
    "nvim",
    "vim",
    "sublime text",
    "cursor",
];

const GAME_KEYWORDS = [
    "steam",
    "league of legends",
    "valorant",
    "dota",
    "minecraft",
    "genshin",
    "counter-strike",
    "cs2",
    "overwatch",
    "fortnite",
    "apex legends",
    "pubg",
    "roblox",
    "epic games",
    "riot client",
    "battle.net",
    "game",
];

// ─── Constants ───────────────────────────────────────────────────────────────

const CHECK_INTERVAL_MS = 2 * 60 * 1000; // 2 phút
const AFK_THRESHOLD_MS = 15 * 60 * 1000;  // 15 phút

// ─── Dependency Injection Interface ──────────────────────────────────────────

export interface StatusSyncDeps {
    /** Lấy thông tin cửa sổ đang active */
    getCurrentActivity: () => Promise<{ appName: string; windowTitle: string }>;
    /** Lấy thời gian idle (ms) */
    getIdleMs: () => Promise<number>;
    /** FocusWarden có đang bật không */
    isFocusWardenActive: () => boolean;
    /** MeetingCopilot có đang hoạt động không */
    isMeetingActive: () => boolean;
    /** Gửi status update lên Telegram bio */
    sendTelegramStatus: (status: string) => Promise<void>;
}

// ─── CrossPlatformStatusSync Daemon ──────────────────────────────────────────

export class CrossPlatformStatusSync {
    #deps: StatusSyncDeps;
    #intervalRef: ReturnType<typeof setInterval> | null = null;
    #lastSyncedStatus: UserStatus = "online";
    #isProcessing = false;

    constructor(deps: StatusSyncDeps) {
        this.#deps = deps;
    }

    /**
     * Bắt đầu daemon theo dõi status. Timer dùng .unref() tránh zombie.
     */
    public start(): void {
        if (this.#intervalRef) return;

        logger.info(
            `[StatusSync] 🚀 Started — checking every ${CHECK_INTERVAL_MS / 1000}s, AFK threshold: ${AFK_THRESHOLD_MS / 1000}s`
        );

        // Chạy ngay lần đầu
        this.#tick().catch((err) => {
            const msg = err instanceof Error ? err.message : String(err);
            logger.warn(`[StatusSync] Initial tick error: ${msg}`);
        });

        this.#intervalRef = setInterval(() => {
            this.#tick().catch((err) => {
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[StatusSync] Tick error: ${msg}`);
            });
        }, CHECK_INTERVAL_MS);
        this.#intervalRef.unref(); // Tránh zombie timer
    }

    /**
     * Dọn dẹp daemon.
     */
    public dispose(): void {
        if (this.#intervalRef) {
            clearInterval(this.#intervalRef);
            this.#intervalRef = null;
        }
        this.#isProcessing = false;
        logger.info("[StatusSync] 🛑 Disposed.");
    }

    /**
     * Lấy status hiện tại (cho external query).
     */
    public getCurrentStatus(): UserStatus {
        return this.#lastSyncedStatus;
    }

    /**
     * Main tick — suy luận status và sync nếu thay đổi.
     */
    async #tick(): Promise<void> {
        if (this.#isProcessing) return;
        this.#isProcessing = true;

        try {
            const newStatus = await this.#inferStatus();

            // Chỉ sync khi status THAY ĐỔI (tránh API spam)
            if (newStatus === this.#lastSyncedStatus) {
                return;
            }

            const display = STATUS_MAP[newStatus];
            logger.info(
                `[StatusSync] 🔄 Status changed: ${this.#lastSyncedStatus} → ${newStatus} (${display.emoji} ${display.text})`
            );

            this.#lastSyncedStatus = newStatus;

            // Sync song song đến tất cả nền tảng (graceful — skip nếu lỗi)
            await Promise.allSettled([
                this.#syncTelegram(display),
                this.#syncSlack(display),
                this.#syncDiscord(display),
            ]);
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[StatusSync] Tick error: ${errMsg}`);
        } finally {
            this.#isProcessing = false;
        }
    }

    /**
     * Suy luận status từ context hệ thống.
     * Thứ tự ưu tiên: FocusWarden > Meeting > AFK > IDE > Game > Online
     */
    async #inferStatus(): Promise<UserStatus> {
        // 1. FocusWarden đang bật → Deep Work (ưu tiên cao nhất)
        if (this.#deps.isFocusWardenActive()) {
            return "deepwork";
        }

        // 2. MeetingCopilot đang hoạt động → Meeting
        if (this.#deps.isMeetingActive()) {
            return "meeting";
        }

        // 3. Kiểm tra idle time → AFK
        try {
            const idleMs = await this.#deps.getIdleMs();
            if (idleMs > AFK_THRESHOLD_MS) {
                return "afk";
            }
        } catch {
            // Không lấy được idle time — bỏ qua check AFK
        }

        // 4. Kiểm tra cửa sổ active → Coding / Gaming
        try {
            const activity = await this.#deps.getCurrentActivity();
            const combined = `${activity.appName} ${activity.windowTitle}`.toLowerCase();

            // Check IDE keywords
            for (const kw of IDE_KEYWORDS) {
                if (combined.includes(kw)) {
                    return "coding";
                }
            }

            // Check Game keywords
            for (const kw of GAME_KEYWORDS) {
                if (combined.includes(kw)) {
                    return "gaming";
                }
            }
        } catch {
            // Không lấy được activity — fallback online
        }

        // 5. Default → Online
        return "online";
    }

    // ─── Platform Sync Methods ───────────────────────────────────────────────

    /**
     * Sync status lên Telegram bio qua deps injection.
     */
    async #syncTelegram(display: StatusDisplay): Promise<void> {
        try {
            const statusText = `${display.emoji} ${display.text}`;
            await this.#deps.sendTelegramStatus(statusText);
            logger.info(`[StatusSync] ✅ Telegram bio updated: ${statusText}`);
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[StatusSync] ⚠️ Telegram sync failed: ${errMsg}`);
        }
    }

    /**
     * Sync status lên Slack profile (nếu SLACK_BOT_TOKEN tồn tại).
     */
    async #syncSlack(display: StatusDisplay): Promise<void> {
        const token = process.env.SLACK_BOT_TOKEN;
        if (!token) return; // Graceful skip — không cấu hình

        try {
            await safeFetch(
                "https://slack.com/api/users.profile.set",
                {
                    method: "POST",
                    headers: {
                        "Content-Type": "application/json; charset=utf-8",
                        Authorization: `Bearer ${token}`,
                    },
                    body: JSON.stringify({
                        profile: {
                            status_text: display.text,
                            status_emoji: display.emoji,
                            status_expiration: 0, // Không tự hết hạn
                        },
                    }),
                },
                10000
            );
            logger.info(`[StatusSync] ✅ Slack status updated: ${display.emoji} ${display.text}`);
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[StatusSync] ⚠️ Slack sync failed: ${errMsg}`);
        }
    }

    /**
     * Discord status sync — placeholder.
     * Discord Bot API không hỗ trợ trực tiếp thay đổi user status qua Bot token.
     * Cần Discord Rich Presence (IPC) hoặc selfbot (vi phạm TOS).
     * Ghi log placeholder để mở rộng sau.
     */
    async #syncDiscord(display: StatusDisplay): Promise<void> {
        const token = process.env.DISCORD_BOT_TOKEN;
        if (!token) return; // Graceful skip

        // Discord Bot API không cho phép thay đổi user presence qua REST.
        // Cần WebSocket Gateway với opcode 3 (Presence Update) — triển khai khi có discord.js hoặc custom WS.
        logger.debug(
            `[StatusSync] 📋 Discord sync skipped (placeholder) — would set: ${display.emoji} ${display.text}. ` +
            `Requires WebSocket Gateway implementation.`
        );
    }
}
