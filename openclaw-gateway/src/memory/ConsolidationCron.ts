import { StructuredMemory, type EventBrick } from "./StructuredMemory";
import { LanceMemoryManager } from "./LanceMemory";
import { BookIndex, type BookNode } from "./BookIndex";
import { logger } from "../utils/logger";
import { safeExtractJSON } from "../utils/JsonExtractor";
import OpenAI from "openai";
import { jsonrepair } from "jsonrepair";

/**
 * ConsolidationCron — Sleep-time Memory Consolidation
 * =====================================================
 * Periodically gathers unconsolidated event bricks from L1 (SQLite),
 * synthesizes them into macro narratives via LLM, embeds the summaries
 * into L2 (LanceDB), and extracts user insights for L3 (StructuredMemory KV).
 *
 * Trigger modes:
 *   - Idle-based: auto-triggers when no user interaction for 30+ minutes
 *   - Manual: via consolidateNow() (exposed as skill or debug)
 *   - Cold-start: checks for orphaned events on boot
 *
 * Safety Features:
 *   - Minimum event threshold (10 events) before triggering
 *   - Session grouping (events within 30 min = same session)
 *   - GC: deletes consolidated events older than 7 days
 *   - dispose(): timer cleanup for shutdown chain
 *
 * @module ConsolidationCron
 */

// ===========================
// Constants
// ===========================

/** Check for idle state every 5 minutes */
const IDLE_CHECK_INTERVAL_MS = 5 * 60 * 1000;

/** Minimum idle time before triggering consolidation (30 minutes) */
const IDLE_THRESHOLD_MS = 30 * 60 * 1000;

/** Minimum unconsolidated events before triggering */
const MIN_EVENTS_THRESHOLD = 10;

/** Events within this window are grouped as the same session */
const SESSION_GAP_MS = 30 * 60 * 1000; // 30 minutes

/** Retention period for consolidated events in L1 */
const EVENT_RETENTION_DAYS = 7;

// ===========================
// Synthesis Prompt
// ===========================

const MACRO_SYNTHESIS_PROMPT = `Bạn là hệ thống tổng hợp ký ức dài hạn. Phân tích chuỗi sự kiện sau và tạo:

1. **narrative_summary**: Tóm tắt tự sự (narrative) về phiên làm việc này (2-3 câu). Nêu bật: người dùng đã làm gì, kết quả ra sao, bối cảnh nào quan trọng.

2. **new_user_insights**: Danh sách phát hiện mới về người dùng (sở thích, thói quen, tính cách). Chỉ liệt kê nếu có bằng chứng rõ ràng, KHÔNG suy đoán.

TRẢ VỀ ĐÚNG JSON:
{"narrative_summary":"Người dùng đã...","new_user_insights":[{"key":"so_thich_x","value":"Thích lập trình Python","category":"Sở thích"}]}

Nếu không có insight mới: {"narrative_summary":"...","new_user_insights":[]}

QUAN TRỌNG: Trả về JSON thuần, KHÔNG markdown.`;

// ===========================
// Types
// ===========================

interface SessionGroup {
    events: EventBrick[];
    startTime: number;
    endTime: number;
}

interface SynthesisResult {
    narrative_summary: string;
    new_user_insights: Array<{ key: string; value: string; category: string }>;
}

// ===========================
// Main Class
// ===========================

export class ConsolidationCron {
    private readonly structuredMemory: StructuredMemory;
    private readonly lanceMemory: LanceMemoryManager;
    private readonly bookIndex: BookIndex;
    private readonly aiClient: OpenAI;
    private idleCheckTimer: NodeJS.Timeout | null = null;
    private lastInteractionTime: number = Date.now();
    private isRunning = false;

    constructor(
        structuredMemory: StructuredMemory,
        lanceMemory: LanceMemoryManager,
        bookIndex: BookIndex,
        aiClient: OpenAI
    ) {
        this.structuredMemory = structuredMemory;
        this.lanceMemory = lanceMemory;
        this.bookIndex = bookIndex;
        this.aiClient = aiClient;
    }

    /**
     * Start the idle-detection loop.
     * Checks every 5 minutes if the user has been idle for 30+ minutes.
     */
    public start(): void {
        if (this.idleCheckTimer) return; // Already running

        this.idleCheckTimer = setInterval(() => {
            const idleTime = Date.now() - this.lastInteractionTime;
            if (idleTime >= IDLE_THRESHOLD_MS) {
                this.consolidateNow().catch(e => {
                    logger.warn(`[ConsolidationCron] Auto-consolidation failed: ${e.message}`);
                });
            }
        }, IDLE_CHECK_INTERVAL_MS);

        logger.info("[ConsolidationCron] ✅ Idle-detection loop started (check every 5 min, trigger after 30 min idle).");
    }

    /**
     * Signal that user interaction occurred (resets idle timer).
     * Called from AgentLoop on every user message.
     */
    public touch(): void {
        this.lastInteractionTime = Date.now();
    }

    /**
     * Stop the idle-detection loop.
     */
    public stop(): void {
        if (this.idleCheckTimer) {
            clearInterval(this.idleCheckTimer);
            this.idleCheckTimer = null;
        }
    }

    /**
     * Clean up all timers. MUST be called in CoreKernel.shutdown().
     */
    public dispose(): void {
        this.stop();
        logger.info("[ConsolidationCron] Disposed. Timers cleared.");
    }

    /**
     * Cold-start Preflight Check — process orphaned events from previous sessions.
     * Called once during MemoryManager.initialize().
     */
    public async preflightCheck(): Promise<void> {
        const pending = this.structuredMemory.getUnconsolidatedCount();
        if (pending >= MIN_EVENTS_THRESHOLD) {
            logger.info(`[ConsolidationCron] 🔄 Cold-start: Found ${pending} orphaned events. Triggering consolidation...`);
            await this.consolidateNow();
        } else if (pending > 0) {
            logger.debug(`[ConsolidationCron] Cold-start: ${pending} pending events (below threshold of ${MIN_EVENTS_THRESHOLD}, skipping).`);
        }
    }

    /**
     * Manual consolidation trigger.
     * Returns count of events consolidated.
     */
    public async consolidateNow(): Promise<number> {
        if (this.isRunning) {
            logger.debug("[ConsolidationCron] Already running, skipping.");
            return 0;
        }

        this.isRunning = true;
        let totalConsolidated = 0;

        try {
            // --- ENERGY AWARENESS ---
            let isBattery = false;
            try {
                // eslint-disable-next-line @typescript-eslint/no-require-imports
                const fs = require('node:fs');
                // eslint-disable-next-line @typescript-eslint/no-require-imports
                const path = require('node:path');
                const hwState = JSON.parse(fs.readFileSync(path.join(process.cwd(), "data", "hardware_state.json"), "utf-8"));
                isBattery = hwState.is_battery === true;
            } catch { /* ignore */ }

            const dynamicThreshold = isBattery ? MIN_EVENTS_THRESHOLD * 5 : MIN_EVENTS_THRESHOLD;

            // 1. Fetch unconsolidated events
            const events = this.structuredMemory.getUnconsolidatedEvents();
            if (events.length < dynamicThreshold) {
                if (isBattery && events.length >= MIN_EVENTS_THRESHOLD) {
                    logger.debug(`🔋 [EnergyAwareness] Laptop đang dùng Pin! Hoãn tác vụ Consolidation ngầm (${events.length}/${dynamicThreshold} events) để tránh rút cạn pin.`);
                } else {
                    logger.debug(`[ConsolidationCron] Only ${events.length} events (need ${dynamicThreshold}), skipping.`);
                }
                return 0;
            }

            // 2. Group events into sessions (30 min gap = new session)
            const sessions = this.groupIntoSessions(events);
            logger.info(`[ConsolidationCron] Processing ${events.length} events across ${sessions.length} session(s)...`);

            // 3. Process each session
            for (const session of sessions) {
                try {
                    const count = await this.processSession(session);
                    totalConsolidated += count;
                } catch (e: unknown) {
                const errMsg = e instanceof Error ? errMsg : String(e);
                    logger.warn(`[ConsolidationCron] Session processing failed: ${errMsg}`);
                }
            }

            // 4. Garbage collect old consolidated events (>7 days)
            this.structuredMemory.gcOldEvents(EVENT_RETENTION_DAYS);

            logger.info(`[ConsolidationCron] ✅ Consolidated ${totalConsolidated} events total.`);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? errMsg : String(e);
            logger.error(`[ConsolidationCron] Consolidation failed: ${errMsg}`);
        } finally {
            this.isRunning = false;
        }

        return totalConsolidated;
    }

    /**
     * Group events into sessions based on time proximity.
     */
    private groupIntoSessions(events: EventBrick[]): SessionGroup[] {
        if (events.length === 0) return [];

        const sessions: SessionGroup[] = [];
        let currentSession: SessionGroup = {
            events: [events[0]],
            startTime: events[0].timestamp,
            endTime: events[0].timestamp,
        };

        for (let i = 1; i < events.length; i++) {
            const gap = events[i].timestamp - currentSession.endTime;
            if (gap > SESSION_GAP_MS) {
                // New session
                sessions.push(currentSession);
                currentSession = {
                    events: [events[i]],
                    startTime: events[i].timestamp,
                    endTime: events[i].timestamp,
                };
            } else {
                // Same session
                currentSession.events.push(events[i]);
                currentSession.endTime = events[i].timestamp;
            }
        }
        sessions.push(currentSession);

        return sessions;
    }

    /**
     * Process a single session: LLM synthesis → L2 + L3 storage.
     */
    private async processSession(session: SessionGroup): Promise<number> {
        // Build event context for LLM
        const eventSummary = session.events
            .map((e, i) => {
                const time = new Date(e.timestamp).toLocaleString("vi-VN");
                const facts = e.phi.facts.join(", ") || "N/A";
                const sentiment = e.psi.sentiment || "N/A";
                return `[${i + 1}] ${time} | Facts: ${facts} | Mood: ${sentiment}\nUser: ${e.rawUserMsg.substring(0, 200)}\nAI: ${e.rawAiReply.substring(0, 200)}`;
            })
            .join("\n\n");

        // Call LLM for Macro Synthesis
        const response = await this.aiClient.chat.completions.create({
            model: "router",
            messages: [
                { role: "system", content: MACRO_SYNTHESIS_PROMPT },
                { role: "user", content: eventSummary },
            ],
            temperature: 0.2,
            max_tokens: 600,
        });

        const raw = response.choices[0]?.message?.content?.trim();
        if (!raw) return 0;

        // Safe JSON extraction
        const result = safeExtractJSON<SynthesisResult>(raw);
        if (!result || !result.narrative_summary) {
            logger.warn(`[ConsolidationCron] Synthesis JSON parse failed: ${raw.substring(0, 100)}`);
            return 0;
        }

        // Store narrative summary in L2 (LanceDB)
        try {
            const eventIds = session.events.map(e => e.eventId);
            await this.lanceMemory.addSemanticAnchor(
                result.narrative_summary,
                eventIds,
                session.startTime
            );

            await this.lanceMemory.addMemory(
                "AXIOM",
                result.narrative_summary,
                `session_${new Date(session.startTime).toISOString().split("T")[0]}`
            );
            logger.info(`[ConsolidationCron] 📝 L2: Stored narrative & anchor: "${result.narrative_summary.substring(0, 80)}..."`);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? errMsg : String(e);
            logger.warn(`[ConsolidationCron] L2 write failed: ${errMsg}`);
        }

        // Store new user insights in L3 (StructuredMemory KV)
        if (result.new_user_insights && Array.isArray(result.new_user_insights)) {
            for (const insight of result.new_user_insights) {
                if (insight.key && insight.value) {
                    this.structuredMemory.setFact(insight.key, insight.value, {
                        source: "consolidation",
                        category: insight.category || "Chung",
                    });
                }
            }
            if (result.new_user_insights.length > 0) {
                logger.info(`[ConsolidationCron] 🧠 L3: Upserted ${result.new_user_insights.length} new user insight(s).`);
            }
        }

        // Mark events as consolidated
        const eventIds = session.events.map(e => e.eventId);
        this.structuredMemory.markConsolidated(eventIds);

        // [RAPTOR Phase 2A] Build Hierarchical Tree for this session
        try {
            await this.buildRaptorTree(session);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? errMsg : String(e);
            logger.warn(`[ConsolidationCron] RAPTOR Tree build failed: ${errMsg}`);
        }

        return session.events.length;
    }

    /**
     * [RAPTOR Phase 2A] Initialize leaf nodes and start recursive summarization.
     */
    private async buildRaptorTree(session: SessionGroup): Promise<void> {
        const leafNodes: BookNode[] = session.events.map(e => ({
            id: `leaf_${e.eventId}`,
            text: `[${new Date(e.timestamp).toLocaleString("vi-VN")}] User: ${e.rawUserMsg} | AI: ${e.rawAiReply} | Facts: ${e.phi.facts.join(", ")}`,
            level: 0,
            isSummary: false
        }));

        for (const node of leafNodes) {
            this.bookIndex.addNode(node);
        }

        logger.info(`[ConsolidationCron/RAPTOR] Started recursive summarization for ${leafNodes.length} leaf nodes...`);
        await this.recursiveSummarize(leafNodes, 1);
    }

    /**
     * [RAPTOR Phase 2A] Recursive Summarization
     * Groups nodes into chunks, summarizes each chunk, and recurses until a root node is formed.
     */
    private async recursiveSummarize(nodes: BookNode[], level: number): Promise<void> {
        if (nodes.length <= 1) {
            logger.info(`[ConsolidationCron/RAPTOR] Reached root at level ${level - 1}. Tree complete.`);
            return;
        }

        const CHUNK_SIZE = 5;
        const nextLevelNodes: BookNode[] = [];

        for (let i = 0; i < nodes.length; i += CHUNK_SIZE) {
            const chunk = nodes.slice(i, i + CHUNK_SIZE);
            const chunkText = chunk.map(n => `- ${n.text}`).join("\n");
            
            const prompt = `Tóm tắt nội dung của các đoạn hội thoại hoặc ký ức sau đây thành một đoạn văn súc tích (1-3 câu), giữ lại các thông tin cốt lõi nhất để phục vụ suy luận logic:
${chunkText}
Trích xuất tối đa các mối quan hệ (ví dụ: A là B, X thuộc Y). Không giải thích.`;

            try {
                const response = await this.aiClient.chat.completions.create({
                    model: "router",
                    messages: [{ role: "user", content: prompt }],
                    temperature: 0.1,
                    max_tokens: 300,
                });
                
                const summary = response.choices[0]?.message?.content?.trim() || "";
                if (summary) {
                    const parentId = `summary_L${level}_${Date.now()}_${i}`;
                    const parentNode: BookNode = {
                        id: parentId,
                        text: summary,
                        level,
                        isSummary: true
                    };
                    
                    this.bookIndex.addNode(parentNode);
                    
                    // Add edges to children
                    for (const child of chunk) {
                        this.bookIndex.addEdge(parentId, child.id);
                    }
                    
                    // Add to LanceDB as ANCHOR for multi-hop retrieval
                    await this.lanceMemory.addSemanticAnchor(summary, chunk.map(c => c.id), Date.now());
                    
                    nextLevelNodes.push(parentNode);
                }
            } catch (error: unknown) {
            const errMsg = error instanceof Error ? errMsg : String(error);
                logger.error(`[ConsolidationCron/RAPTOR] Summarization chunk failed at level ${level}: ${errMsg}`);
                // Fallback to avoid infinite loop or dropping completely
                nextLevelNodes.push(chunk[0]); 
            }
        }

        // Recurse to next level
        if (nextLevelNodes.length > 0 && nextLevelNodes.length < nodes.length) {
            await this.recursiveSummarize(nextLevelNodes, level + 1);
        }
    }
}
