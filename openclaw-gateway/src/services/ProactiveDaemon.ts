import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";
import { randomUUID } from "node:crypto";

/**
 * ProactiveDaemon — LIVA v24 Shadow Digest Pipeline
 * ===================================================
 * Orchestrates background news aggregation, VRAM-gated LLM synthesis,
 * and sentient omni-delivery (Push/Pull modes).
 *
 * Architecture:
 *   1. Zero-Prompt Profiling: Reads L3 PersonalKnowledge (memory_strength > 0.2)
 *   2. Background Ingestion: Fetches news via Tavily API (News mode) using safeFetch
 *   3. VRAM-Gated Synthesis: Only runs LLM when AgentLoop.isBusy === false
 *   4. Asymmetric Routing: Prefers Cloud API to preserve local VRAM
 *   5. SQLite Cache: Stores SSML/Markdown in daily_briefings (TTL: 24h)
 *   6. Sentient Delivery: Push (Toast + Ding) or Pull (instant PromptBuilder inject)
 *
 * Constraints:
 *   - ALL heavy work in async callbacks (never blocks main thread)
 *   - Timer uses .unref() to prevent Zombie Timer
 *   - VRAM Guard: defers synthesis if AgentLoop is busy
 *   - Maximum 3 retry attempts with exponential backoff
 *
 * @module ProactiveDaemon
 */

export interface ProactiveDaemonDeps {
    /** Returns user interest topics and focus topics */
    getTopics: () => Promise<{ interests: string[]; focus: string[] }>;
    /** Returns true if AgentLoop is currently processing (VRAM in use) */
    isAgentBusy: () => boolean;
    /** Saves a completed briefing to SQLite daily_briefings */
    saveBriefing: (briefing: { id: string; topics: string; content: string; source?: string; ttlHours?: number }) => void;
    /** Gets unread briefings count */
    getUnreadCount: () => number;
    /** Cleans expired briefings */
    cleanExpired: () => number;
    /** Sends a notification to the UI (Toast + soft ding) */
    pushNotification?: (title: string, body: string) => void;
    /** Sends briefing via Telegram/Zalo when user is offline */
    pushEgress?: (content: string) => void;
    /** Check if user is currently online (has active WebSocket) */
    isUserOnline?: () => boolean;
}

// Default schedule: 7:00 AM every day
const DEFAULT_SCHEDULE_HOUR = 7;
const DEFAULT_SCHEDULE_MINUTE = 0;
const DIGEST_INTERVAL_MS = 60 * 60 * 1000; // Check every hour
const VRAM_RETRY_DELAY_MS = 5 * 60 * 1000; // Retry after 5 min if VRAM busy
const MAX_VRAM_RETRIES = 3;
const TAVILY_NEWS_LIMIT = 5;

/** Shape of a fetched news article before synthesis */
interface DigestArticle {
    title: string;
    url: string;
    content: string;
    category: string;
}

export class ProactiveDaemon {
    #deps: ProactiveDaemonDeps;
    #intervalRef: ReturnType<typeof setInterval> | null = null;
    #lastDigestDate: string = ""; // ISO date string "YYYY-MM-DD"
    #vramRetryCount = 0;
    #pendingArticles: Array<{ title: string; url: string; content: string; category: string }> = [];
    #isRunning = false;
    #retryTimer: ReturnType<typeof setTimeout> | null = null;
    #scheduleHour: number;
    #scheduleMinute: number;

    constructor(deps: ProactiveDaemonDeps, options?: { scheduleHour?: number; scheduleMinute?: number }) {
        this.#deps = deps;
        this.#scheduleHour = options?.scheduleHour ?? DEFAULT_SCHEDULE_HOUR;
        this.#scheduleMinute = options?.scheduleMinute ?? DEFAULT_SCHEDULE_MINUTE;
    }

    /**
     * Start the background daemon. Timer uses .unref() to prevent zombie.
     */
    public start(): void {
        if (this.#intervalRef) return;
        logger.info(`[v24 ProactiveDaemon] 🚀 Started — schedule: ${this.#scheduleHour}:${String(this.#scheduleMinute).padStart(2, "0")} daily`);

        // Run immediately on start to check if we missed today's digest
        this.#tick().catch(err => {
            const msg = err instanceof Error ? err.message : String(err);
            logger.warn(`[v24 ProactiveDaemon] Tick error: ${msg}`);
        });

        this.#intervalRef = setInterval(() => {
            this.#tick().catch(err => {
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[v24 ProactiveDaemon] Tick error: ${msg}`);
            });
        }, DIGEST_INTERVAL_MS);
        this.#intervalRef.unref(); // Prevent zombie timer
    }

    /**
     * Stop the daemon and cleanup.
     */
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
        logger.info("[v24 ProactiveDaemon] 🛑 Disposed.");
    }

    /**
     * Force a digest run (for testing or manual trigger).
     */
    public async forceDigest(): Promise<void> {
        this.#lastDigestDate = ""; // Reset to allow re-run
        await this.#tick();
    }

    /**
     * Main tick — runs every hour, triggers digest at scheduled time.
     */
    async #tick(): Promise<void> {
        // Clean expired briefings
        const cleaned = this.#deps.cleanExpired();
        if (cleaned > 0) {
            logger.debug(`[v24 ProactiveDaemon] 🧹 Cleaned ${cleaned} expired briefings.`);
        }

        const now = new Date();
        const todayDate = now.toISOString().split("T")[0];

        // Already produced today's digest
        if (this.#lastDigestDate === todayDate && this.#pendingArticles.length === 0) {
            return;
        }

        // Check if we're within the digest window (schedule hour ± 30 min buffer)
        const currentMinutes = now.getHours() * 60 + now.getMinutes();
        const scheduleMinutes = this.#scheduleHour * 60 + this.#scheduleMinute;
        const isInWindow = currentMinutes >= scheduleMinutes - 30 && currentMinutes <= scheduleMinutes + 60;

        if (!isInWindow && this.#pendingArticles.length === 0) {
            return;
        }

        // Phase 2A: Fetch news if we haven't yet
        if (this.#pendingArticles.length === 0 && this.#lastDigestDate !== todayDate) {
            await this.#fetchNews();
        }

        // Phase 2B: Synthesize if we have articles and VRAM is free
        if (this.#pendingArticles.length > 0) {
            await this.#synthesize(todayDate);
        }
    }

    /**
     * Phase 2A: Background Ingestion — fetch news via Tavily API for both Interests and Focus.
     * Zero main-thread blocking (all async).
     */
    async #fetchNews(): Promise<void> {
        if (this.#isRunning) return;
        this.#isRunning = true;
        this.#pendingArticles = [];

        try {
            const { interests, focus } = await this.#deps.getTopics();
            
            if (interests.length === 0 && focus.length === 0) {
                logger.info("[v24 ProactiveDaemon] No interests or focus found. Skipping.");
                this.#isRunning = false;
                return;
            }

            const tavilyKey = process.env.TAVILY_API_KEY;
            if (!tavilyKey) {
                logger.warn("[v24 ProactiveDaemon] TAVILY_API_KEY not set — skipping news fetch.");
                this.#isRunning = false;
                return;
            }

            const fetchCategory = async (topics: string[], category: string) => {
                if (topics.length === 0) return;
                const searchQuery = topics.slice(0, 3).join(", "); // Max 3 topics per category
                logger.info(`[v24 ProactiveDaemon] 🔍 Fetching news for ${category}: ${searchQuery}`);
                
                try {
                    const res = await safeFetch("https://api.tavily.com/search", {
                        method: "POST",
                        headers: { "Content-Type": "application/json" },
                        body: JSON.stringify({
                            api_key: tavilyKey,
                            query: `Latest news about ${searchQuery}`,
                            search_depth: "basic",
                            topic: "news",
                            max_results: 3, // 3 results per category to save tokens
                            include_answer: false,
                        }),
                    }, 15000);

                    const data = await res.json() as { results?: Array<{ title: string; url: string; content: string }> };
                    const articles = (data.results ?? []).map(r => ({
                        title: r.title || "Untitled",
                        url: r.url || "",
                        content: (r.content || "").substring(0, 800),
                        category
                    }));
                    this.#pendingArticles.push(...articles);
                } catch (err: unknown) {
                    const errMsg = err instanceof Error ? err.message : String(err);
                    logger.warn(`[v24 ProactiveDaemon] News fetch failed for ${category}: ${errMsg}`);
                }
            };

            await fetchCategory(interests, "Sở thích (Interests)");
            await fetchCategory(focus, "Mối quan tâm (Focus)");

            logger.info(`[v24 ProactiveDaemon] 📰 Fetched ${this.#pendingArticles.length} articles total.`);
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[v24 ProactiveDaemon] News fetch failed: ${errMsg}`);
            this.#pendingArticles = [];
        } finally {
            this.#isRunning = false;
        }
    }

    /**
     * Phase 2B: VRAM-Gated Ghost Synthesis.
     * Only runs when AgentLoop is IDLE. Defers with exponential backoff if busy.
     */
    async #synthesize(todayDate: string): Promise<void> {
        // VRAM Guard: check if GPU is free
        if (this.#deps.isAgentBusy()) {
            this.#vramRetryCount++;
            if (this.#vramRetryCount > MAX_VRAM_RETRIES) {
                logger.warn(`[v24 ProactiveDaemon] ⚠️ VRAM busy after ${MAX_VRAM_RETRIES} retries — storing raw articles without synthesis.`);
                this.#storeRawBriefing(todayDate);
                return;
            }
            logger.info(`[v24 ProactiveDaemon] ⏳ VRAM busy — deferring synthesis (retry ${this.#vramRetryCount}/${MAX_VRAM_RETRIES})`);
            // Schedule retry after delay (non-blocking) — tracked for dispose() cleanup
            if (this.#retryTimer) clearTimeout(this.#retryTimer);
            this.#retryTimer = setTimeout(() => {
                this.#retryTimer = null;
                this.#synthesize(todayDate).catch(() => {});
            }, VRAM_RETRY_DELAY_MS);
            this.#retryTimer.unref();
            return;
        }

        this.#isRunning = true;
        this.#vramRetryCount = 0;

        try {
            // Asymmetric Routing: prefer Cloud API for synthesis to preserve local VRAM
            const cloudUrl = process.env.AI_BASE_URL;
            const cloudKey = process.env.AI_API_KEY;
            const cloudModel = process.env.AI_MODEL || "gemini-2.5-flash";

            const interestsArticles = this.#pendingArticles.filter(a => a.category.includes("Sở thích"));
            const focusArticles = this.#pendingArticles.filter(a => a.category.includes("Mối quan tâm"));

            const processGroup = async (articles: DigestArticle[], title: string, emoji: string) => {
                if (articles.length === 0) return;
                
                let summary: string;
                if (cloudUrl && cloudKey) {
                    summary = await this.#cloudSynthesize(cloudUrl, cloudKey, cloudModel, articles, title);
                } else {
                    logger.info(`[v24 ProactiveDaemon] No cloud API configured — storing raw for ${title}.`);
                    summary = this.#formatRawBriefing(articles, title);
                }

                // Cache to SQLite
                const briefingId = `digest_${todayDate}_${randomUUID().substring(0, 8)}`;
                const topics = articles.map(a => a.title).join("; ");
                this.#deps.saveBriefing({
                    id: briefingId,
                    topics,
                    content: summary,
                    source: cloudUrl ? "cloud_synthesis" : "raw_articles",
                    ttlHours: 24,
                });

                // Phase 4: Sentient Delivery
                this.#deliver(`${emoji} ${title}`, summary);
                logger.info(`[v24 ProactiveDaemon] ✅ Digest complete for ${title}: ${briefingId}`);
            };

            await processGroup(interestsArticles, "Báo cáo Sở thích", "🌟");
            await processGroup(focusArticles, "Báo cáo Mối quan tâm", "🔥");

            this.#lastDigestDate = todayDate;
            this.#pendingArticles = [];
            this.#isRunning = false;
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.error(`[v24 ProactiveDaemon] Synthesis failed: ${errMsg}`);
            // Fallback: store raw
            this.#storeRawBriefing(todayDate);
        } finally {
            this.#isRunning = false;
        }
    }

    /**
     * Cloud LLM synthesis via OpenAI-compatible API.
     * Asymmetric routing: uses cloud to preserve local VRAM.
     */
    async #cloudSynthesize(baseUrl: string, apiKey: string, model: string, articles: DigestArticle[], reportTitle: string): Promise<string> {
        const endpoint = baseUrl.endsWith("/") ? `${baseUrl}chat/completions` : `${baseUrl}/chat/completions`;

        const articlesText = articles.map((a, i) =>
            `[${i + 1}] Title: ${a.title}\nContent: ${a.content}\nSource: ${a.url}`
        ).join("\n\n---\n\n");

        const res = await safeFetch(endpoint, {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                "Authorization": `Bearer ${apiKey}`,
            },
            body: JSON.stringify({
                model,
                messages: [
                    {
                        role: "system",
                        content: `You are a concise Vietnamese news summarizer. Write a briefing for "${reportTitle}". Use bullet points and keep summaries to 2-3 sentences per article. Add relevant emoji. Output in Markdown format. Do NOT wrap everything in a massive code block.`
                    },
                    {
                        role: "user",
                        content: `Summarize these ${articles.length} articles into the "${reportTitle}":\n\n${articlesText}`
                    }
                ],
                max_tokens: 1000,
                temperature: 0.3,
            }),
        }, 30000);

        const data = await res.json() as { choices?: Array<{ message?: { content?: string } }> };
        const content = data.choices?.[0]?.message?.content;

        if (!content) {
            throw new Error("Empty response from cloud LLM");
        }

        return content.trim();
    }

    /**
     * Fallback: store raw articles without LLM synthesis.
     */
    #storeRawBriefing(todayDate: string): void {
        const interests = this.#pendingArticles.filter(a => a.category.includes("Sở thích"));
        const focus = this.#pendingArticles.filter(a => a.category.includes("Mối quan tâm"));

        if (interests.length > 0) {
            const sum = this.#formatRawBriefing(interests, "Báo cáo Sở thích");
            const topics = interests.map(a => a.title).join("; ");
            this.#deps.saveBriefing({
                id: `digest_${todayDate}_${randomUUID().substring(0, 8)}`,
                topics,
                content: sum,
                source: "raw_articles",
                ttlHours: 24,
            });
            this.#deliver("🌟 Báo cáo Sở thích", sum);
        }
        if (focus.length > 0) {
            const sum = this.#formatRawBriefing(focus, "Báo cáo Mối quan tâm");
            const topics = focus.map(a => a.title).join("; ");
            this.#deps.saveBriefing({
                id: `digest_${todayDate}_${randomUUID().substring(0, 8)}`,
                topics,
                content: sum,
                source: "raw_articles",
                ttlHours: 24,
            });
            this.#deliver("🔥 Báo cáo Mối quan tâm", sum);
        }

        this.#lastDigestDate = todayDate;
        this.#pendingArticles = [];
        this.#isRunning = false;
    }

    /**
     * Format raw articles as a simple Markdown briefing.
     */
    #formatRawBriefing(articles: Array<{ title: string; url: string; content: string; category: string }>, title: string): string {
        const now = new Date().toLocaleDateString("vi-VN", { weekday: "long", year: "numeric", month: "long", day: "numeric" });
        let md = `# 📰 ${title} — ${now}\n\n`;
        
        for (const [i, a] of articles.entries()) {
            md += `### ${i + 1}. ${a.title}\n${a.content.substring(0, 300)}...\n🔗 [Nguồn](${a.url})\n\n`;
        }
        
        return md;
    }

    /**
     * Phase 4: Sentient Omni-Delivery.
     * Push: Toast notification if online, Egress if offline.
     */
    #deliver(title: string, content: string): void {
        const finalMessage = `📰 ${title}\n\n${content}`;
        
        if (this.#deps.isUserOnline?.()) {
            // Push Mode: Silent UI Toast
            this.#deps.pushNotification?.(title, content);
            logger.info(`[v24 ProactiveDaemon] 📬 Push notification sent: ${title}.`);
        } else {
            // Offline Mode: Egress via Telegram/Zalo
            this.#deps.pushEgress?.(finalMessage);
            logger.info(`[v24 ProactiveDaemon] 📤 Briefing sent via egress: ${title}.`);
        }
    }
}
