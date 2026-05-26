import { StructuredMemory } from "./StructuredMemory";
import { EmbeddingService } from "../services/EmbeddingService";
import { ReconsolidationEngine } from "./ReconsolidationEngine";
import { ContradictionResolver } from "./ContradictionResolver";
import { BookIndex } from "./BookIndex";
import { logger } from "../utils/logger";
import OpenAI from "openai";
import { memoryEvents } from "./MemoryEventBus";
import { TaskQueue, TaskPriority } from "../core/TaskQueue";
import { ConsolidationPipeline, type ConsolidationContext } from "./ConsolidationPipeline";
import { createConsolidationSteps, type StepDependencies } from "./ConsolidationSteps";

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


// [v27] SESSION_GAP_MS, EVENT_RETENTION_DAYS, SEED_DOMAINS, SessionGroup, SynthesisResult
// moved to ConsolidationSteps.ts

// [UHM] Passive Affective Trigger Constants (still used by ConsolidationCron)
const AFFECTIVE_DEBOUNCE_MS = 15_000;
const TOPIC_SHIFT_THRESHOLD = 3;
const UNCONSOLIDATED_EVENT_THRESHOLD = 20;

// ===========================
// Synthesis Prompt (shared with ConsolidationSteps via StepDependencies)
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
     * Cold-start Preflight Check — process orphaned events and resume interrupted pipelines.
     * Called once during MemoryManager.initialize().
     */
    public async preflightCheck(): Promise<void> {
        // [v27] Check for interrupted pipeline checkpoint first
        const pipeline = this.#createPipeline(false);
        const resumeCtx = pipeline.resumeFromCheckpoint('consolidation_main');
        if (resumeCtx) {
            logger.info(`[ConsolidationCron] 🔄 Resuming interrupted pipeline from step ${resumeCtx.currentStepIndex}/${pipeline.stepCount}...`);
            await TaskQueue.wrapMemoryTask(
                async () => {
                    this.isRunning = true;
                    try { await pipeline.run(resumeCtx); } finally { this.isRunning = false; }
                    return resumeCtx.totalConsolidated;
                },
                `ConsolidationCron-Resume-${Date.now()}`,
                TaskPriority.LOW
            );
            return;
        }

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
     * [v27] Create a ConsolidationPipeline wired with all 9 steps.
     */
    #createPipeline(force: boolean): ConsolidationPipeline {
        const deps: StepDependencies = {
            structuredMemory: this.structuredMemory,
            embeddingService: this.embeddingService,
            aiClient: this.aiClient,
            bookIndex: this.bookIndex,
            contradictionResolver: this.contradictionResolver,
            reconsolidationEngine: this.reconsolidationEngine,
            synthesisPrompt: MACRO_SYNTHESIS_PROMPT,
        };

        const db = this.structuredMemory.getDb();
        const pipeline = new ConsolidationPipeline(
            (sql) => db.exec(sql),
            (sql) => db.prepare(sql) as any,
            (sql) => db.prepare(sql) as any,
        );

        const steps = createConsolidationSteps(deps, force);
        for (const step of steps) {
            pipeline.addStep(step);
        }

        // Forward pipeline events to MemoryEventBus for observability
        pipeline.on('step_complete', ({ step, progress }) => {
            logger.debug(`[Pipeline] ✅ ${step} (${Math.round(progress * 100)}%)`);
        });
        pipeline.on('pipeline_complete', ({ totalConsolidated }) => {
            memoryEvents.emit('CONSOLIDATION_COMPLETE', totalConsolidated);
        });
        pipeline.on('pipeline_error', ({ step, error }) => {
            const errMsg = error instanceof Error ? (error as Error).message : String(error);
            logger.error(`[Pipeline] ❌ Failed at ${step}: ${errMsg}`);
        });

        return pipeline;
    }

    /**
     * Manual consolidation trigger.
     * [v27] Delegates to ConsolidationPipeline with checkpoint/resume.
     * Returns count of events consolidated.
     */
    public async consolidateNow(force: boolean = false): Promise<number> {
        if (this.isRunning) {
            logger.debug("[ConsolidationCron] Already running, skipping.");
            return 0;
        }

        this.isRunning = true;

        try {
            const pipeline = this.#createPipeline(force);
            const ctx: ConsolidationContext = {
                sessionId: 'consolidation_main',
                currentStepIndex: 0,
                totalConsolidated: 0,
                sharedState: {},
            };

            await pipeline.run(ctx);

            logger.info(`[ConsolidationCron] ✅ Pipeline complete. Consolidated ${ctx.totalConsolidated} events.`);
            return ctx.totalConsolidated;
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[ConsolidationCron] Consolidation pipeline failed: ${errMsg}`);
            return 0;
        } finally {
            this.isRunning = false;
        }
    }

    // [v27] All session processing, taxonomy, RAPTOR, and helper methods
    // have been extracted into ConsolidationSteps.ts for pipeline execution.
    // See: src/memory/ConsolidationSteps.ts
}
