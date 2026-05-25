import { StructuredMemory, type EventBrick } from "./StructuredMemory";
import { EmbeddingService } from "../services/EmbeddingService";
import { ReconsolidationEngine } from "./ReconsolidationEngine";
import { ContradictionResolver } from "./ContradictionResolver";
import { BookIndex, type BookNode } from "./BookIndex";
import { logger } from "../utils/logger";
import { safeExtractJSON } from "../utils/JsonExtractor";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import OpenAI from "openai";
import { jsonrepair } from "jsonrepair";
import { memoryEvents } from "./MemoryEventBus";
import { TaskQueue, TaskPriority } from "../core/TaskQueue";

/**
 * ConsolidationCron — Sleep-time Memory Consolidation
 * =====================================================
 * Periodically gathers unconsolidated event bricks from L1 (SQLite),
 * synthesizes them into macro narratives via LLM, embeds the summaries
 * into L2 (sqlite-vec), and extracts user insights for L3 (StructuredMemory KV).
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

/** Seed Domains for Dynamic Taxonomy (H-MEM v18) */
const SEED_DOMAINS = new Set(["Development", "Personal", "Security", "Finance", "Entertainment", "General"]);

// [UHM] Passive Affective Trigger Constants
/** Debounce delay before checking for affective trigger (15 seconds) */
const AFFECTIVE_DEBOUNCE_MS = 15_000;

/** Topic shifts in sliding window to trigger early consolidation */
const TOPIC_SHIFT_THRESHOLD = 3;

/** Unconsolidated event count to trigger early consolidation */
const UNCONSOLIDATED_EVENT_THRESHOLD = 20;

// ===========================
// Synthesis Prompt
// ===========================

const MACRO_SYNTHESIS_PROMPT = `You are a long-term memory synthesis system. Analyze the following event sequence and generate:

1. **narrative_summary**: A brief narrative summary of this session (2-3 sentences). Highlight: what the user did, the outcomes, and important context.

2. **new_user_insights**: A list of newly discovered insights about the user (hobbies, habits, personality traits). Only list if there is clear evidence, DO NOT guess.

7. **graph_nodes**: Entities found in the session (id, label, properties).
8. **graph_edges**: Relationships between nodes (source, target, relation).

Output EXACTLY this JSON structure:
{"narrative_summary":"User did...","new_user_insights":[{"key":"hobby_x","value":"Likes Python programming","category":"Hobby"}], "graph_nodes": [{"id":"User", "label":"PERSON", "properties":"{}"}], "graph_edges": [{"source":"User", "target":"ProjectX", "relation":"WORKING_ON"}]}

[CRITICAL] Extract relationships and factual logic in English, but you MUST PRESERVE all original Vietnamese proper nouns, entities, local concepts, and direct quotes exactly as they appeared in the text.

[CRITICAL] ENTITY & COREFERENCE RESOLUTION:
You MUST resolve ambiguous references, pronouns, and general roles (e.g., "my nephew", "that coworker", "my brother") to their specific names if they are mentioned in the conversation context (e.g., instead of node id "Nephew", use the resolved name like "NephewTom" or "Tom" with label "PERSON" and relationship properties). If the exact name is not present, bind them to a specific context target to prevent merging distinct people into a single generic role.

If no new insights or graph data: {"narrative_summary":"...","new_user_insights":[],"graph_nodes":[],"graph_edges":[]}

IMPORTANT: Return raw JSON, NO markdown.`;

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
    graph_nodes: Array<{ id: string; label: string; properties: string }>;
    graph_edges: Array<{ source: string; target: string; relation: string }>;
}

// ===========================
// Main Class
// ===========================

export class ConsolidationCron {
    private readonly structuredMemory: StructuredMemory;
    private readonly embeddingService: EmbeddingService;
    private readonly reconsolidationEngine: ReconsolidationEngine | null;
    private readonly bookIndex: BookIndex;
    private readonly aiClient: OpenAI;
    private readonly contradictionResolver: ContradictionResolver;
    #idleCheckTimer: NodeJS.Timeout | null = null;
    private lastInteractionTime: number = Date.now();
    private isRunning = false;

    // [UHM] Passive Affective Trigger State
    private affectiveDebounceTimer: NodeJS.Timeout | null = null;
    private topicShiftCount: number = 0;
    private agentLoopStateGetter: (() => string) | null = null;
    #onNewTurn: (() => void) | null = null;
    #onTopicShift: (() => void) | null = null;

    constructor(
        structuredMemory: StructuredMemory,
        embeddingService: EmbeddingService,
        bookIndex: BookIndex,
        aiClient: OpenAI,
        reconsolidationEngine?: ReconsolidationEngine
    ) {
        this.structuredMemory = structuredMemory;
        this.embeddingService = embeddingService;
        this.bookIndex = bookIndex;
        this.aiClient = aiClient;
        this.reconsolidationEngine = reconsolidationEngine ?? null;
        this.contradictionResolver = new ContradictionResolver(structuredMemory, embeddingService, aiClient);

        // [UHM] Subscribe to MemoryEventBus — decoupled from ReflectionDaemon
        this.#onNewTurn = () => this.recordActivity('NEW_TURN');
        this.#onTopicShift = () => this.recordActivity('TOPIC_SHIFT');
        memoryEvents.on('NEW_TURN', this.#onNewTurn);
        memoryEvents.on('TOPIC_SHIFT', this.#onTopicShift);
    }

    /**
     * Start the idle-detection loop.
     * Checks every 5 minutes if the user has been idle for 30+ minutes.
     */
    public start(): void {
        if (this.#idleCheckTimer) return; // Already running

        this.#idleCheckTimer = setInterval(() => {
            const idleTime = Date.now() - this.lastInteractionTime;
            if (idleTime >= IDLE_THRESHOLD_MS) {
                TaskQueue.wrapMemoryTask(
                    () => this.consolidateNow(),
                    `ConsolidationCron-Idle-${Date.now()}`,
                    TaskPriority.LOW
                ).catch(e => {
                    logger.warn(`[ConsolidationCron] Auto-consolidation failed: ${e.message}`);
                });
            }
        }, IDLE_CHECK_INTERVAL_MS);
        this.#idleCheckTimer.unref(); // Don't prevent process exit

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
     * [UHM] Inject AgentLoop state getter to enable VRAM guard.
     * ConsolidationCron MUST NOT trigger while LLM is streaming.
     * Called once during BootstrapManager initialization.
     */
    public setAgentLoopStateGetter(getter: () => string): void {
        this.agentLoopStateGetter = getter;
    }

    /**
     * [UHM] Record a passive activity signal (topic shift or new turn).
     * Uses passive signals ONLY — zero LLM calls, zero sentiment analysis.
     * Triggers: topicShiftCount >= 3 OR unconsolidatedCount >= 20.
     */
    public recordActivity(signal: 'TOPIC_SHIFT' | 'NEW_TURN'): void {
        if (signal === 'TOPIC_SHIFT') {
            this.topicShiftCount++;
        }
        this.scheduleAffectiveCheck();
    }

    /**
     * [UHM] Debounced passive trigger: waits 15s after last activity signal.
     * Guards via BOTH isRunning AND agentLoop state (VRAM protection).
     */
    private scheduleAffectiveCheck(): void {
        if (this.affectiveDebounceTimer) {
            clearTimeout(this.affectiveDebounceTimer);
        }

        this.affectiveDebounceTimer = setTimeout(async () => {
            this.affectiveDebounceTimer = null;

            // [VRAM Guard] Block if consolidation already running
            if (this.isRunning) {
                logger.debug("[ConsolidationCron/Affective] Skipped: consolidation already running.");
                return;
            }

            // [VRAM Guard] Block if AgentLoop is NOT idle (LLM streaming/thinking)
            if (this.agentLoopStateGetter && this.agentLoopStateGetter() !== 'IDLE') {
                logger.debug("[ConsolidationCron/Affective] Skipped: AgentLoop busy, deferring.");
                return;
            }

            if (await this.shouldTriggerAffective()) {
                logger.info("[ConsolidationCron/Affective] 🔥 Passive trigger fired! Starting early consolidation...");
                this.topicShiftCount = 0; // Reset after firing
                TaskQueue.wrapMemoryTask(
                    () => this.consolidateNow(),
                    `ConsolidationCron-Affective-${Date.now()}`,
                    TaskPriority.LOW
                ).catch(e => {
                    logger.warn(`[ConsolidationCron/Affective] Early consolidation failed: ${e.message}`);
                });
            }
        }, AFFECTIVE_DEBOUNCE_MS);
    }

    /**
     * [UHM] Determine if passive conditions warrant early consolidation.
     * Checks: (1) topicShiftCount >= 3 OR (2) unconsolidatedCount >= 20.
     * Zero LLM calls — entirely data-driven.
     */
    public async shouldTriggerAffective(): Promise<boolean> {
        if (this.topicShiftCount >= TOPIC_SHIFT_THRESHOLD) return true;
        if (await this.structuredMemory.getUnconsolidatedCount() >= UNCONSOLIDATED_EVENT_THRESHOLD) return true;
        return false;
    }

    /**
     * [UHM] Get current affective state for testing/monitoring.
     */
    public async getAffectiveState(): Promise<{ topicShiftCount: number; unconsolidatedCount: number }> {
        return {
            topicShiftCount: this.topicShiftCount,
            unconsolidatedCount: await this.structuredMemory.getUnconsolidatedCount(),
        };
    }

    /**
     * Stop the idle-detection loop.
     */
    public stop(): void {
        if (this.#idleCheckTimer) {
            clearInterval(this.#idleCheckTimer);
            this.#idleCheckTimer = null;
        }
    }

    /**
     * Clean up all timers. MUST be called in CoreKernel.shutdown().
     */
    public dispose(): void {
        this.stop();
        // [UHM] Clear affective timer
        if (this.affectiveDebounceTimer) {
            clearTimeout(this.affectiveDebounceTimer);
            this.affectiveDebounceTimer = null;
        }
        this.topicShiftCount = 0;
        // [UHM] Unsubscribe from EventBus to prevent zombie listeners
        if (this.#onNewTurn) memoryEvents.removeListener('NEW_TURN', this.#onNewTurn);
        if (this.#onTopicShift) memoryEvents.removeListener('TOPIC_SHIFT', this.#onTopicShift);
        this.#onNewTurn = null;
        this.#onTopicShift = null;
        logger.info("[ConsolidationCron] Disposed. Timers cleared.");
    }

    /**
     * Cold-start Preflight Check — process orphaned events from previous sessions.
     * Called once during MemoryManager.initialize().
     */
    public async preflightCheck(): Promise<void> {
        const pending = await this.structuredMemory.getUnconsolidatedCount();
        if (pending >= MIN_EVENTS_THRESHOLD) {
            logger.info(`[ConsolidationCron] 🔄 Cold-start: Found ${pending} orphaned events. Triggering consolidation...`);
            await TaskQueue.wrapMemoryTask(
                () => this.consolidateNow(),
                `ConsolidationCron-ColdStart-${Date.now()}`,
                TaskPriority.LOW
            );
        } else if (pending > 0) {
            logger.debug(`[ConsolidationCron] Cold-start: ${pending} pending events (below threshold of ${MIN_EVENTS_THRESHOLD}, skipping).`);
        }
    }

    /**
     * Manual consolidation trigger.
     * Returns count of events consolidated.
     */
    public async consolidateNow(force: boolean = false): Promise<number> {
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
                const hwStatePath = path.join(process.cwd(), "data", "hardware_state.json");
                const hwData = await fsp.readFile(hwStatePath, "utf-8");
                const hwState = JSON.parse(hwData);
                isBattery = hwState.is_battery === true;
            } catch { /* ignore — file may not exist */ }

            const dynamicThreshold = force ? 1 : (isBattery ? MIN_EVENTS_THRESHOLD * 5 : MIN_EVENTS_THRESHOLD);

            // 1. Fetch unconsolidated events
            const events = await this.structuredMemory.getUnconsolidatedEvents();
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
                const errMsg = e instanceof Error ? e.message : String(e);
                    logger.warn(`[ConsolidationCron] Session processing failed: ${errMsg}`);
                }
            }

            // 4. Garbage collect old consolidated events (>7 days)
            await this.structuredMemory.gcOldEvents(EVENT_RETENTION_DAYS);

            // [H-MEM v18] 5. Dynamic Taxonomy Auto-Expansion with Semantic Normalization
            this.#processUnknownTaxonomy();

            // [H-MEM v18] 6. WAL Checkpoint PASSIVE — dọn dẹp WAL mà không block Read/Write
            try {
                this.structuredMemory.getDb().exec("PRAGMA wal_checkpoint(PASSIVE)");
                logger.debug("[ConsolidationCron] WAL checkpoint (PASSIVE) completed.");
            } catch { /* non-critical */ }

            // [v19] 7. Process DLQ entries
            await this.structuredMemory.processDLQ();

            // [UHM] 8. Apply Ebbinghaus memory decay to facts (async with G11 chunking)
            try {
                const decay = await this.structuredMemory.applyMemoryDecay();
                if (decay.decayed > 0 || decay.archived > 0) {
                    logger.info(`[ConsolidationCron/Ebbinghaus] Decayed: ${decay.decayed}, Archived: ${decay.archived}`);
                }
            } catch (decayErr: unknown) {
                const errMsg = decayErr instanceof Error ? decayErr.message : String(decayErr);
                logger.warn(`[ConsolidationCron/Ebbinghaus] Decay failed (non-critical): ${errMsg}`);
            }

            // [Phase 3] 9. Build GraphRAG community summaries
            try {
                await this.structuredMemory.graph.buildCommunitySummaries(
                    this.aiClient,
                    this.embeddingService,
                    (record) => this.structuredMemory.upsertVector(record)
                );
            } catch (graphErr: unknown) {
                const errMsg = graphErr instanceof Error ? graphErr.message : String(graphErr);
                logger.error(`[ConsolidationCron/GraphRAG] Failed to build community summaries: ${errMsg}`);
            }

            logger.info(`[ConsolidationCron] ✅ Consolidated ${totalConsolidated} events total.`);
            memoryEvents.emit('CONSOLIDATION_COMPLETE', totalConsolidated);

            // [UHM-v3] 10. Atomic snapshot backup (VACUUM INTO — non-critical)
            try {
                await this.structuredMemory.createSnapshotBackup();
            } catch { /* logged inside createSnapshotBackup */ }
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
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

        // Store narrative summary in L2 (sqlite-vec)
        try {
            const eventIds = session.events.map(e => e.eventId);
            // Generate embedding for narrative summary
            const vector = await this.embeddingService.embed(result.narrative_summary);
            const anchorId = `anchor_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`;
            this.structuredMemory.upsertVector({
                vecId: anchorId,
                type: 'ANCHOR',
                content: result.narrative_summary,
                vector,
                domain: session.events[0]?.domain ?? 'General',
                category: session.events[0]?.category ?? 'Uncategorized',
                traceKeywords: [...new Set(session.events.flatMap(e => e.traceKeywords ?? []))],
                sourceEventIds: eventIds,  // [UHM] L2→L1 positional pointer
            });

            // [H-MEM v18] Route AXIOMs through ReconsolidationEngine for conflict-aware storage
            if (this.reconsolidationEngine && result.narrative_summary) {
                try {
                    const sessionDomain = session.events[0]?.domain ?? "General";
                    const sessionCategory = session.events[0]?.category ?? "Uncategorized";
                    const sessionTraces = session.events.flatMap(e => e.traceKeywords ?? []);

                    await this.reconsolidationEngine.sweepAndReconcile([{
                        text: result.narrative_summary,
                        domain: sessionDomain,
                        category: sessionCategory,
                        trace_identifiers: [...new Set(sessionTraces)],
                    }]);
                } catch (reconErr: unknown) {
                    const errMsg = reconErr instanceof Error ? reconErr.message : String(reconErr);
                    logger.warn(`[ConsolidationCron] Reconsolidation failed, falling back to direct write: ${errMsg}`);
                    // Fallback: direct write
                    const axiomVec = await this.embeddingService.embed(result.narrative_summary);
                    this.structuredMemory.upsertVector({
                        vecId: `axiom_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
                        type: 'AXIOM',
                        content: result.narrative_summary,
                        vector: axiomVec,
                        domain: session.events[0]?.domain ?? 'General',
                    });
                }
            } else {
                // No reconsolidation engine — direct write
                const axiomVec = await this.embeddingService.embed(result.narrative_summary);
                this.structuredMemory.upsertVector({
                    vecId: `axiom_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
                    type: 'AXIOM',
                    content: result.narrative_summary,
                    vector: axiomVec,
                    domain: session.events[0]?.domain ?? 'General',
                });
            }

            logger.info(`[ConsolidationCron] 📝 L2: Stored narrative & anchor: "${result.narrative_summary.substring(0, 80)}..."`);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
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

        // [Phase 2] Store Graph Nodes & Edges, then trigger ContradictionResolver
        if (result.graph_nodes && Array.isArray(result.graph_nodes)) {
            for (const node of result.graph_nodes) {
                if (node.id && node.label) {
                    await this.structuredMemory.graph.upsertNode(node);
                }
            }
        }

        if (result.graph_edges && Array.isArray(result.graph_edges)) {
            for (const edge of result.graph_edges) {
                if (edge.source && edge.target && edge.relation) {
                    const l3Edge = { ...edge, weight: 1.0, obsolete: 0 };
                    await this.structuredMemory.graph.upsertEdge(l3Edge);
                    
                    // Trigger ContradictionResolver in background to not block consolidation
                    const sNode = result.graph_nodes?.find(n => n.id === edge.source) || { id: edge.source, label: "ENTITY", properties: "{}" };
                    const tNode = result.graph_nodes?.find(n => n.id === edge.target) || { id: edge.target, label: "ENTITY", properties: "{}" };
                    
                    this.contradictionResolver.resolve(l3Edge, sNode, tNode).catch(err => {
                        logger.error(`[ConsolidationCron] ContradictionResolver background task failed: ${err}`);
                    });
                }
            }
            if (result.graph_edges.length > 0) {
                logger.info(`[ConsolidationCron] 🕸️ L3 Graph: Upserted ${result.graph_nodes?.length || 0} nodes and ${result.graph_edges.length} edges.`);
            }
        }

        // Mark events as consolidated
        const eventIds2 = session.events.map(e => e.eventId);
        await this.structuredMemory.markConsolidated(eventIds2);

        // [RAPTOR Phase 2A] Build Hierarchical Tree for this session
        try {
            await this.buildRaptorTree(session);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
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
            text: `[${new Date(e.timestamp).toLocaleString("vi-VN")}] User: ${e.rawUserMsg} | Assistant: ${e.rawAiReply} | Facts: ${e.phi.facts.join(", ")}`,
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
            
            const prompt = `Summarize the following conversations or memories into a concise paragraph (1-3 sentences), retaining the most core information for logical reasoning:
[CRITICAL] Extract relationships and factual logic in English, but you MUST PRESERVE all original Vietnamese proper nouns, entities, local concepts, and direct quotes exactly as they appeared in the text.

${chunkText}`;

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
                    
                    // Add to sqlite-vec as ANCHOR for multi-hop retrieval
                    const anchorVec = await this.embeddingService.embed(summary);
                    this.structuredMemory.upsertVector({
                        vecId: `raptor_${parentId}`,
                        type: 'ANCHOR',
                        content: summary,
                        vector: anchorVec,
                    });
                    
                    nextLevelNodes.push(parentNode);
                }
            } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
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

    // ===========================
    // [H-MEM v18] Dynamic Taxonomy Management
    // ===========================

    /**
     * Process Unknown_* taxonomy tags:
     * 1. Semantic Normalization: group similar Unknown_* tags
     * 2. Auto-Expansion: promote to official domain when ≥ 3 axioms
     * 3. Garbage Collection: clean up stale Unknown_* tags (>7 days, not recently accessed)
     */
    #processUnknownTaxonomy(): void {
        try {
            const db = this.structuredMemory.getDb();

            // Get all Unknown_* domain counts
            const unknownDomains = db.prepare(
                "SELECT domain, COUNT(*) as cnt, MIN(timestamp) as oldest, MAX(last_accessed_at) as last_touch FROM events WHERE domain LIKE 'Unknown_%' GROUP BY domain"
            ).all() as Array<{domain: string; cnt: number; oldest: number; last_touch: number}>;

            if (unknownDomains.length === 0) return;

            // Semantic Normalization: group similar Unknown_* tags by stem
            const normalizedGroups = new Map<string, {totalCount: number; originalTags: string[]; oldest: number; lastTouch: number}>();
            for (const row of unknownDomains) {
                // Extract keyword, lowercase, and stem (simple: take first 5 chars as stem key)
                const keyword = row.domain.replace('Unknown_', '').toLowerCase();
                const stemKey = keyword.substring(0, Math.min(keyword.length, 5));

                const existing = normalizedGroups.get(stemKey);
                if (existing) {
                    existing.totalCount += row.cnt;
                    existing.originalTags.push(row.domain);
                    existing.oldest = Math.min(existing.oldest, row.oldest);
                    existing.lastTouch = Math.max(existing.lastTouch, row.last_touch);
                } else {
                    normalizedGroups.set(stemKey, {
                        totalCount: row.cnt,
                        originalTags: [row.domain],
                        oldest: row.oldest,
                        lastTouch: row.last_touch,
                    });
                }
            }

            const now = Date.now();
            const sevenDaysMs = 7 * 24 * 60 * 60 * 1000;
            const twentyFourHoursMs = 24 * 60 * 60 * 1000;

            for (const [stemKey, group] of normalizedGroups) {
                if (group.totalCount >= 3) {
                    // Auto-Expansion: promote to official domain
                    const officialName = stemKey.charAt(0).toUpperCase() + stemKey.slice(1);
                    if (!SEED_DOMAINS.has(officialName)) {
                        // Update all matching events to the new official domain
                        for (const tag of group.originalTags) {
                            db.prepare("UPDATE events SET domain = ? WHERE domain = ?").run(officialName, tag);
                        }
                        logger.info(`[ConsolidationCron/Taxonomy] 🏷️ Promoted "${stemKey}" → "${officialName}" (${group.totalCount} axioms)`);
                    }
                } else {
                    // Garbage Collection: clean up stale Unknown_* (>7 days old, not recently accessed)
                    const isStale = (now - group.oldest) > sevenDaysMs;
                    const isHotMemory = (now - group.lastTouch) < twentyFourHoursMs;

                    if (isStale && !isHotMemory) {
                        // Safe to garbage collect — not recently accessed
                        for (const tag of group.originalTags) {
                            db.prepare("UPDATE events SET domain = 'General' WHERE domain = ?").run(tag);
                        }
                        logger.info(`[ConsolidationCron/Taxonomy] 🗑️ GC'd stale Unknown tag(s): ${group.originalTags.join(", ")}`);
                    }
                }
            }
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[ConsolidationCron/Taxonomy] Error: ${errMsg}`);
        }
    }

}
