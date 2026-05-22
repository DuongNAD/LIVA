import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";

/**
 * MorningBriefingCast — LIVA Proactive Morning Briefing Daemon
 * ==============================================================
 * Delivers a daily morning briefing at 8:00 AM with:
 *   - Weather summary (via get_weather skill)
 *   - Calendar events (via calendar_scheduler skill)
 *   - Crypto prices (BTC, ETH from CoinGecko)
 *   - Unread message count
 *
 * VRAM Guard: Defers if AgentLoop is busy, retries up to 3 times (5 min apart).
 * Timer uses .unref() to prevent zombie.
 *
 * @module MorningBriefingCast
 */

// ============================================================
// Deps Interface
// ============================================================

export interface MorningBriefingDeps {
    /** Call an existing LIVA skill by name */
    executeSkill: (name: string, args: any) => Promise<string>;
    /** Text-to-speech output */
    speakTTS: (text: string) => Promise<void>;
    /** Push notification to UI */
    pushNotification: (title: string, body: string) => void;
    /** Returns true if AgentLoop is currently processing */
    isAgentBusy: () => boolean;
    /** Returns true if user has an active WebSocket connection */
    isUserOnline: () => boolean;
    /** Returns count of unread messages across all channels */
    getUnreadCount: () => number;
}

// ============================================================
// Constants
// ============================================================

const CHECK_INTERVAL_MS = 60_000; // Check every 60 seconds
const VRAM_RETRY_DELAY_MS = 5 * 60 * 1000; // 5 minutes between retries
const MAX_VRAM_RETRIES = 3;
const COINGECKO_API = "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin,ethereum&vs_currencies=usd&include_24hr_change=true";

// ============================================================
// Types
// ============================================================

interface CryptoPrice {
    usd: number;
    usd_24h_change: number;
}

interface CoinGeckoResponse {
    bitcoin?: CryptoPrice;
    ethereum?: CryptoPrice;
}

// ============================================================
// MorningBriefingCast Daemon
// ============================================================

export class MorningBriefingCast {
    #deps: MorningBriefingDeps;
    #intervalRef: ReturnType<typeof setInterval> | null = null;
    #retryTimer: ReturnType<typeof setTimeout> | null = null;
    #hasBriefedToday = false;
    #lastBriefingDate = "";
    #scheduleHour: number;
    #scheduleMinute: number;
    #vramRetryCount = 0;
    #isRunning = false;

    constructor(deps: MorningBriefingDeps, options?: { scheduleHour?: number; scheduleMinute?: number }) {
        this.#deps = deps;
        this.#scheduleHour = options?.scheduleHour ?? 8;
        this.#scheduleMinute = options?.scheduleMinute ?? 0;
    }

    // ---- Lifecycle ----

    public start(): void {
        if (this.#intervalRef) return;

        this.#intervalRef = setInterval(() => {
            this.#tick().catch(err => {
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[MorningBriefingCast] Tick error: ${msg}`);
            });
        }, CHECK_INTERVAL_MS);
        this.#intervalRef.unref(); // Prevent zombie timer

        logger.info(`[MorningBriefingCast] 🌅 Started — schedule: ${this.#scheduleHour}:${String(this.#scheduleMinute).padStart(2, "0")} daily`);
    }

    public dispose(): void {
        if (this.#intervalRef) {
            clearInterval(this.#intervalRef);
            this.#intervalRef = null;
        }
        if (this.#retryTimer) {
            clearTimeout(this.#retryTimer);
            this.#retryTimer = null;
        }
        this.#isRunning = false;
        logger.info("[MorningBriefingCast] 🛑 Disposed.");
    }

    /**
     * Force a briefing (manual trigger, e.g., from a skill or debug).
     */
    public async forceBriefing(): Promise<void> {
        this.#hasBriefedToday = false;
        this.#lastBriefingDate = "";
        this.#vramRetryCount = 0;
        await this.#deliverBriefing();
    }

    // ---- Internal Tick ----

    async #tick(): Promise<void> {
        const now = new Date();
        const todayDate = now.toISOString().split("T")[0];

        // Reset briefed flag at midnight
        if (this.#lastBriefingDate !== todayDate) {
            this.#hasBriefedToday = false;
            this.#vramRetryCount = 0;
        }

        // Already briefed today
        if (this.#hasBriefedToday) return;

        // Check if we're past the schedule time
        const currentMinutes = now.getHours() * 60 + now.getMinutes();
        const scheduleMinutes = this.#scheduleHour * 60 + this.#scheduleMinute;

        // Only trigger within a 2-hour window after schedule time
        if (currentMinutes < scheduleMinutes || currentMinutes > scheduleMinutes + 120) {
            return;
        }

        // User must be online
        if (!this.#deps.isUserOnline()) {
            logger.debug("[MorningBriefingCast] User offline — deferring briefing.");
            return;
        }

        // VRAM Guard
        if (this.#deps.isAgentBusy()) {
            this.#vramRetryCount++;
            if (this.#vramRetryCount > MAX_VRAM_RETRIES) {
                logger.warn(`[MorningBriefingCast] ⚠️ Agent busy after ${MAX_VRAM_RETRIES} retries — skipping today's briefing.`);
                this.#hasBriefedToday = true;
                this.#lastBriefingDate = todayDate;
                return;
            }
            logger.info(`[MorningBriefingCast] ⏳ Agent busy — deferring briefing (retry ${this.#vramRetryCount}/${MAX_VRAM_RETRIES})`);

            if (this.#retryTimer) clearTimeout(this.#retryTimer);
            this.#retryTimer = setTimeout(() => {
                this.#retryTimer = null;
                this.#tick().catch(() => {});
            }, VRAM_RETRY_DELAY_MS);
            this.#retryTimer.unref();
            return;
        }

        await this.#deliverBriefing();
    }

    // ---- Briefing Assembly ----

    async #deliverBriefing(): Promise<void> {
        if (this.#isRunning) return;
        this.#isRunning = true;

        try {
            const now = new Date();
            const todayDate = now.toISOString().split("T")[0];
            const dateStr = now.toLocaleDateString("vi-VN", {
                weekday: "long",
                year: "numeric",
                month: "long",
                day: "numeric",
            });

            // 1. Fetch weather (graceful)
            const weatherSummary = await this.#fetchWeather();

            // 2. Fetch calendar (graceful)
            const calendarSummary = await this.#fetchCalendar();

            // 3. Fetch crypto prices (graceful)
            const cryptoSummary = await this.#fetchCrypto();

            // 4. Get unread count
            let unreadCount = 0;
            try {
                unreadCount = this.#deps.getUnreadCount();
            } catch {
                // Non-critical
            }

            // 5. Compose briefing
            const lines: string[] = [
                `Chào buổi sáng anh Dương! Hôm nay ${dateStr}.`,
            ];

            if (weatherSummary) {
                lines.push(`🌤️ Thời tiết: ${weatherSummary}`);
            }

            if (cryptoSummary) {
                lines.push(`📊 Crypto: ${cryptoSummary}`);
            }

            if (calendarSummary) {
                lines.push(`📅 Lịch hôm nay: ${calendarSummary}`);
            }

            if (unreadCount > 0) {
                lines.push(`💬 Có ${unreadCount} tin nhắn chưa đọc.`);
            } else {
                lines.push(`💬 Không có tin nhắn chưa đọc.`);
            }

            lines.push("Chúc anh một ngày làm việc hiệu quả!");

            const briefingText = lines.join("\n");

            // 6. Deliver via TTS
            try {
                await this.#deps.speakTTS(briefingText);
            } catch (err: unknown) {
                const errMsg = err instanceof Error ? err.message : String(err);
                logger.warn(`[MorningBriefingCast] TTS failed: ${errMsg}`);
            }

            // 7. Push notification
            try {
                this.#deps.pushNotification("🌅 Briefing buổi sáng", briefingText);
            } catch (err: unknown) {
                const errMsg = err instanceof Error ? err.message : String(err);
                logger.warn(`[MorningBriefingCast] Push notification failed: ${errMsg}`);
            }

            // 8. Mark as briefed
            this.#hasBriefedToday = true;
            this.#lastBriefingDate = todayDate;
            this.#vramRetryCount = 0;

            logger.info("[MorningBriefingCast] ✅ Morning briefing delivered successfully.");
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.error(`[MorningBriefingCast] Briefing delivery failed: ${errMsg}`);
        } finally {
            this.#isRunning = false;
        }
    }

    // ---- Data Fetchers (all graceful — never throw) ----

    async #fetchWeather(): Promise<string> {
        try {
            const result = await this.#deps.executeSkill("get_weather", {});

            // Parse weather skill result — extract essential info
            if (result.includes("ERROR")) {
                logger.debug(`[MorningBriefingCast] Weather skill returned error: ${result.substring(0, 100)}`);
                return "";
            }

            // Extract key info from weather result
            const tempMatch = result.match(/(\d+)[°]?C/);
            const descMatch = result.match(/(?:thời tiết|weather|mô tả|description)[:\s]*([^\n]+)/i);

            if (tempMatch || descMatch) {
                const temp = tempMatch ? `${tempMatch[1]}°C` : "";
                const desc = descMatch ? descMatch[1].trim() : "";
                return [temp, desc].filter(Boolean).join(", ");
            }

            // Fallback: return first meaningful line
            const firstLine = result.split("\n").find(l => l.trim().length > 10);
            return firstLine?.substring(0, 100) ?? "Không lấy được dữ liệu thời tiết";
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.debug(`[MorningBriefingCast] Weather fetch failed: ${errMsg}`);
            return "";
        }
    }

    async #fetchCalendar(): Promise<string> {
        try {
            const result = await this.#deps.executeSkill("calendar_scheduler", { action: "list" });

            if (result.includes("ERROR")) {
                return "";
            }

            // Extract calendar events (strip prefix)
            const cleaned = result
                .replace(/\[CALENDAR.*?\]/g, "")
                .replace(/\(MOCK MODE\)/g, "")
                .trim();

            if (!cleaned || cleaned.length < 5) {
                return "Không có sự kiện nào.";
            }

            // Limit to first 200 chars
            return cleaned.substring(0, 200);
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.debug(`[MorningBriefingCast] Calendar fetch failed: ${errMsg}`);
            return "";
        }
    }

    async #fetchCrypto(): Promise<string> {
        try {
            const res = await safeFetch(COINGECKO_API, {}, 10_000);
            const data = await res.json() as CoinGeckoResponse;

            const parts: string[] = [];

            if (data.bitcoin) {
                const btcPrice = data.bitcoin.usd.toLocaleString("en-US", { maximumFractionDigits: 0 });
                const btcChange = data.bitcoin.usd_24h_change?.toFixed(1) ?? "N/A";
                const btcArrow = (data.bitcoin.usd_24h_change ?? 0) >= 0 ? "📈" : "📉";
                parts.push(`BTC $${btcPrice} (${btcArrow}${btcChange}%)`);
            }

            if (data.ethereum) {
                const ethPrice = data.ethereum.usd.toLocaleString("en-US", { maximumFractionDigits: 0 });
                const ethChange = data.ethereum.usd_24h_change?.toFixed(1) ?? "N/A";
                const ethArrow = (data.ethereum.usd_24h_change ?? 0) >= 0 ? "📈" : "📉";
                parts.push(`ETH $${ethPrice} (${ethArrow}${ethChange}%)`);
            }

            return parts.join(", ") || "Không lấy được giá crypto";
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.debug(`[MorningBriefingCast] Crypto fetch failed: ${errMsg}`);
            return "";
        }
    }
}
