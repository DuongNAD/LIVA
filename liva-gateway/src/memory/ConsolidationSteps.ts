/**
 * ConsolidationSteps — Decomposed Pipeline Steps
 * =================================================
 * [v27 Tech Debt] Each step extracted from the monolithic consolidateNow()
 * into a composable ConsolidationStep for the ConsolidationPipeline.
 *
 * Steps execute sequentially via ConsolidationPipeline.run().
 * Each step reads/writes to ConsolidationContext.sharedState for data passing.
 *
 * @module ConsolidationSteps
 */

import type { ConsolidationStep, ConsolidationContext } from './ConsolidationPipeline';
import type { StructuredMemory, EventBrick } from './StructuredMemory';
import type { EmbeddingService } from '../services/EmbeddingService';
import type { ReconsolidationEngine } from './ReconsolidationEngine';
import type { ContradictionResolver } from './ContradictionResolver';
import type { BookIndex, BookNode } from './BookIndex';
import type OpenAI from 'openai';
import { logger } from '../utils/logger';
import { safeExtractJSON } from '../utils/JsonExtractor';
import { promises as fsp } from 'node:fs';
import * as path from 'node:path';

// ===========================
// Constants (shared with ConsolidationCron)
// ===========================

const MIN_EVENTS_THRESHOLD = 10;
const SESSION_GAP_MS = 30 * 60 * 1000;
const EVENT_RETENTION_DAYS = 7;
const SEED_DOMAINS = new Set(["Development", "Personal", "Security", "Finance", "Entertainment", "General"]);

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
// Shared Dependencies Interface
// ===========================

/**
 * All external dependencies required by consolidation steps.
 * Injected once at pipeline construction to avoid tight coupling.
 */
export interface StepDependencies {
    structuredMemory: StructuredMemory;
    embeddingService: EmbeddingService;
    aiClient: OpenAI;
    bookIndex: BookIndex;
    contradictionResolver: ContradictionResolver;
    reconsolidationEngine: ReconsolidationEngine | null;
    synthesisPrompt: string;
}

// ===========================
// Helper: Session Grouping
// ===========================

function groupIntoSessions(events: EventBrick[]): SessionGroup[] {
    if (events.length === 0) return [];
    const sessions: SessionGroup[] = [];
    let current: SessionGroup = {
        events: [events[0]],
        startTime: events[0].timestamp,
        endTime: events[0].timestamp,
    };
    for (let i = 1; i < events.length; i++) {
        if (events[i].timestamp - current.endTime > SESSION_GAP_MS) {
            sessions.push(current);
            current = { events: [events[i]], startTime: events[i].timestamp, endTime: events[i].timestamp };
        } else {
            current.events.push(events[i]);
            current.endTime = events[i].timestamp;
        }
    }
    sessions.push(current);
    return sessions;
}

// ===========================
// Step 1: Fetch & Gate
// ===========================

/**
 * Reads unconsolidated events from L1 and applies energy-awareness gating.
 * Writes `events` and `sessions` to sharedState.
 */
export class FetchAndGateStep implements ConsolidationStep {
    readonly stepName = 'FetchAndGate';
    readonly #deps: StepDependencies;
    readonly #force: boolean;

    constructor(deps: StepDependencies, force: boolean = false) {
        this.#deps = deps;
        this.#force = force;
    }

    async execute(ctx: ConsolidationContext): Promise<void> {
        // Energy Awareness
        let isBattery = false;
        try {
            const hwStatePath = path.join(process.cwd(), "data", "hardware_state.json");
            const hwData = await fsp.readFile(hwStatePath, "utf-8");
            const hwState = JSON.parse(hwData);
            isBattery = hwState.is_battery === true;
        } catch { /* ignore — file may not exist */ }

        const dynamicThreshold = this.#force ? 1 : (isBattery ? MIN_EVENTS_THRESHOLD * 5 : MIN_EVENTS_THRESHOLD);
        const events = await this.#deps.structuredMemory.getUnconsolidatedEvents();

        if (events.length < dynamicThreshold) {
            if (isBattery && events.length >= MIN_EVENTS_THRESHOLD) {
                logger.debug(`🔋 [EnergyAwareness] Laptop đang dùng Pin! Hoãn tác vụ Consolidation ngầm (${events.length}/${dynamicThreshold} events).`);
            } else {
                logger.debug(`[Pipeline/FetchAndGate] Only ${events.length} events (need ${dynamicThreshold}), skipping.`);
            }
            // Store empty to signal downstream steps to skip
            ctx.sharedState.events = [];
            ctx.sharedState.sessions = [];
            return;
        }

        const sessions = groupIntoSessions(events);
        ctx.sharedState.events = events;
        ctx.sharedState.sessions = sessions;
        logger.info(`[Pipeline/FetchAndGate] ${events.length} events across ${sessions.length} session(s).`);
    }
}

// ===========================
// Step 2: Process Sessions (LLM Synthesis → L2 + L3)
// ===========================

/**
 * For each session: calls LLM for macro synthesis, stores narrative in L2 (sqlite-vec),
 * stores user insights in L3 (KV), and builds graph nodes/edges.
 */
export class ProcessSessionsStep implements ConsolidationStep {
    readonly stepName = 'ProcessSessions';
    readonly #deps: StepDependencies;

    constructor(deps: StepDependencies) {
        this.#deps = deps;
    }

    async execute(ctx: ConsolidationContext): Promise<void> {
        const sessions = ctx.sharedState.sessions as SessionGroup[] | undefined;
        if (!sessions || sessions.length === 0) return;

        for (const session of sessions) {
            try {
                const count = await this.#processOneSession(session);
                ctx.totalConsolidated += count;
            } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e);
                logger.warn(`[Pipeline/ProcessSessions] Session failed: ${errMsg}`);
                throw e; // Rethrow so pipeline catches it and writes to DLQ
            }
        }
    }

    async #processOneSession(session: SessionGroup): Promise<number> {
        const eventSummary = session.events
            .map((e, i) => {
                const time = new Date(e.timestamp).toLocaleString("vi-VN");
                const facts = e.phi.facts.join(", ") || "N/A";
                const sentiment = e.psi.sentiment || "N/A";
                return `[${i + 1}] ${time} | Facts: ${facts} | Mood: ${sentiment}\nUser: ${e.rawUserMsg.substring(0, 200)}\nAI: ${e.rawAiReply.substring(0, 200)}`;
            })
            .join("\n\n");

        // LLM Macro Synthesis
        const response = await this.#deps.aiClient.chat.completions.create({
            model: "router",
            messages: [
                { role: "system", content: this.#deps.synthesisPrompt },
                { role: "user", content: eventSummary },
            ],
            temperature: 0.2,
            max_tokens: 600,
        });

        const raw = response.choices[0]?.message?.content?.trim();
        if (!raw) return 0;

        const result = safeExtractJSON<SynthesisResult>(raw);
        if (!result || !result.narrative_summary) {
            logger.warn(`[Pipeline/ProcessSessions] Synthesis JSON parse failed: ${raw.substring(0, 100)}`);
            return 0;
        }

        // L2: Store narrative anchor
        await this.#storeNarrativeL2(session, result);

        // L3: Store user insights
        await this.#storeInsightsL3(result);

        // L3 Graph: Nodes + Edges + Contradiction
        await this.#storeGraphL3(result);

        // Mark events as consolidated
        const eventIds = session.events.map(e => e.eventId);
        await this.#deps.structuredMemory.markConsolidated(eventIds);

        // RAPTOR Tree
        try {
            await this.#buildRaptorTree(session);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[Pipeline/ProcessSessions] RAPTOR Tree build failed: ${errMsg}`);
        }

        return session.events.length;
    }

    async #storeNarrativeL2(session: SessionGroup, result: SynthesisResult): Promise<void> {
        try {
            const eventIds = session.events.map(e => e.eventId);
            const vector = await this.#deps.embeddingService.embed(result.narrative_summary);
            await this.#deps.structuredMemory.upsertVector({
                vecId: `anchor_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
                type: 'ANCHOR',
                content: result.narrative_summary,
                vector,
                domain: session.events[0]?.domain ?? 'General',
                category: session.events[0]?.category ?? 'Uncategorized',
                traceKeywords: [...new Set(session.events.flatMap(e => e.traceKeywords ?? []))],
                sourceEventIds: eventIds,
            });

            // Reconsolidation Engine or fallback
            if (this.#deps.reconsolidationEngine && result.narrative_summary) {
                try {
                    await this.#deps.reconsolidationEngine.sweepAndReconcile([{
                        text: result.narrative_summary,
                        domain: session.events[0]?.domain ?? "General",
                        category: session.events[0]?.category ?? "Uncategorized",
                        trace_identifiers: [...new Set(session.events.flatMap(e => e.traceKeywords ?? []))],
                    }]);
                } catch (reconErr: unknown) {
                    const errMsg = reconErr instanceof Error ? reconErr.message : String(reconErr);
                    logger.warn(`[Pipeline/ProcessSessions] Reconsolidation failed, fallback: ${errMsg}`);
                    const axiomVec = await this.#deps.embeddingService.embed(result.narrative_summary);
                    this.#deps.structuredMemory.upsertVector({
                        vecId: `axiom_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
                        type: 'AXIOM',
                        content: result.narrative_summary,
                        vector: axiomVec,
                        domain: session.events[0]?.domain ?? 'General',
                    });
                }
            } else {
                const axiomVec = await this.#deps.embeddingService.embed(result.narrative_summary);
                this.#deps.structuredMemory.upsertVector({
                    vecId: `axiom_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
                    type: 'AXIOM',
                    content: result.narrative_summary,
                    vector: axiomVec,
                    domain: session.events[0]?.domain ?? 'General',
                });
            }

            logger.info(`[Pipeline/ProcessSessions] 📝 L2: Stored narrative: "${result.narrative_summary.substring(0, 80)}..."`);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[Pipeline/ProcessSessions] L2 write failed: ${errMsg}`);
        }
    }

    async #storeInsightsL3(result: SynthesisResult): Promise<void> {
        if (result.new_user_insights && Array.isArray(result.new_user_insights)) {
            for (const insight of result.new_user_insights) {
                if (insight.key && insight.value) {
                    await this.#deps.structuredMemory.setFact(insight.key, insight.value, {
                        source: "consolidation",
                        category: insight.category || "Chung",
                    });
                }
            }
            if (result.new_user_insights.length > 0) {
                logger.info(`[Pipeline/ProcessSessions] 🧠 L3: Upserted ${result.new_user_insights.length} insight(s).`);
            }
        }
    }

    async #storeGraphL3(result: SynthesisResult): Promise<void> {
        if (result.graph_nodes && Array.isArray(result.graph_nodes)) {
            for (const node of result.graph_nodes) {
                if (node.id && node.label) {
                    await this.#deps.structuredMemory.graph.upsertNode(node);
                }
            }
        }
        if (result.graph_edges && Array.isArray(result.graph_edges)) {
            for (const edge of result.graph_edges) {
                if (edge.source && edge.target && edge.relation) {
                    const l3Edge = { ...edge, weight: 1.0, obsolete: 0 };
                    await this.#deps.structuredMemory.graph.upsertEdge(l3Edge);

                    const sNode = result.graph_nodes?.find(n => n.id === edge.source) || { id: edge.source, label: "ENTITY", properties: "{}" };
                    const tNode = result.graph_nodes?.find(n => n.id === edge.target) || { id: edge.target, label: "ENTITY", properties: "{}" };

                    this.#deps.contradictionResolver.resolve(l3Edge, sNode, tNode).catch(err => {
                        logger.error(`[Pipeline/ProcessSessions] ContradictionResolver failed: ${err}`);
                    });
                }
            }
            if (result.graph_edges.length > 0) {
                logger.info(`[Pipeline/ProcessSessions] 🕸️ L3 Graph: ${result.graph_nodes?.length || 0} nodes, ${result.graph_edges.length} edges.`);
            }
        }
    }

    async #buildRaptorTree(session: SessionGroup): Promise<void> {
        const leafNodes: BookNode[] = session.events.map(e => ({
            id: `leaf_${e.eventId}`,
            text: `[${new Date(e.timestamp).toLocaleString("vi-VN")}] User: ${e.rawUserMsg} | Assistant: ${e.rawAiReply} | Facts: ${e.phi.facts.join(", ")}`,
            level: 0,
            isSummary: false
        }));
        for (const node of leafNodes) {
            this.#deps.bookIndex.addNode(node);
        }
        logger.info(`[Pipeline/RAPTOR] Started recursive summarization for ${leafNodes.length} leaf nodes...`);
        await this.#recursiveSummarize(leafNodes, 1);
    }

    async #recursiveSummarize(nodes: BookNode[], level: number): Promise<void> {
        if (nodes.length <= 1) {
            logger.info(`[Pipeline/RAPTOR] Reached root at level ${level - 1}. Tree complete.`);
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
                const response = await this.#deps.aiClient.chat.completions.create({
                    model: "router",
                    messages: [{ role: "user", content: prompt }],
                    temperature: 0.1,
                    max_tokens: 300,
                });
                const summary = response.choices[0]?.message?.content?.trim() || "";
                if (summary) {
                    const parentId = `summary_L${level}_${Date.now()}_${i}`;
                    const parentNode: BookNode = { id: parentId, text: summary, level, isSummary: true };
                    this.#deps.bookIndex.addNode(parentNode);
                    for (const child of chunk) {
                        this.#deps.bookIndex.addEdge(parentId, child.id);
                    }
                    const anchorVec = await this.#deps.embeddingService.embed(summary);
                    this.#deps.structuredMemory.upsertVector({
                        vecId: `raptor_${parentId}`,
                        type: 'ANCHOR',
                        content: summary,
                        vector: anchorVec,
                    });
                    nextLevelNodes.push(parentNode);
                }
            } catch (error: unknown) {
                const errMsg = error instanceof Error ? error.message : String(error);
                logger.error(`[Pipeline/RAPTOR] Summarization chunk failed at level ${level}: ${errMsg}`);
                nextLevelNodes.push(chunk[0]);
            }
        }
        if (nextLevelNodes.length > 0 && nextLevelNodes.length < nodes.length) {
            await this.#recursiveSummarize(nextLevelNodes, level + 1);
        }
    }
}

// ===========================
// Step 3: GC Old Events
// ===========================

export class GCOldEventsStep implements ConsolidationStep {
    readonly stepName = 'GCOldEvents';
    readonly #deps: StepDependencies;

    constructor(deps: StepDependencies) { this.#deps = deps; }

    async execute(_ctx: ConsolidationContext): Promise<void> {
        const events = _ctx.sharedState.events as EventBrick[] | undefined;
        if (!events || events.length === 0) return;
        await this.#deps.structuredMemory.gcOldEvents(EVENT_RETENTION_DAYS);
    }
}

// ===========================
// Step 4: Dynamic Taxonomy
// ===========================

export class DynamicTaxonomyStep implements ConsolidationStep {
    readonly stepName = 'DynamicTaxonomy';
    readonly #deps: StepDependencies;

    constructor(deps: StepDependencies) { this.#deps = deps; }

    async execute(_ctx: ConsolidationContext): Promise<void> {
        const events = _ctx.sharedState.events as EventBrick[] | undefined;
        if (!events || events.length === 0) return;

        try {
            const db = this.#deps.structuredMemory.getDb();
            const unknownDomains = db.prepare(
                "SELECT domain, COUNT(*) as cnt, MIN(timestamp) as oldest, MAX(last_accessed_at) as last_touch FROM events WHERE domain LIKE 'Unknown_%' GROUP BY domain"
            ).all() as Array<{domain: string; cnt: number; oldest: number; last_touch: number}>;

            if (unknownDomains.length === 0) return;

            const normalizedGroups = new Map<string, {totalCount: number; originalTags: string[]; oldest: number; lastTouch: number}>();
            for (const row of unknownDomains) {
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
                        totalCount: row.cnt, originalTags: [row.domain],
                        oldest: row.oldest, lastTouch: row.last_touch,
                    });
                }
            }

            const now = Date.now();
            const sevenDaysMs = 7 * 24 * 60 * 60 * 1000;
            const twentyFourHoursMs = 24 * 60 * 60 * 1000;

            for (const [stemKey, group] of normalizedGroups) {
                if (group.totalCount >= 3) {
                    const officialName = stemKey.charAt(0).toUpperCase() + stemKey.slice(1);
                    if (!SEED_DOMAINS.has(officialName)) {
                        for (const tag of group.originalTags) {
                            db.prepare("UPDATE events SET domain = ? WHERE domain = ?").run(officialName, tag);
                        }
                        logger.info(`[Pipeline/Taxonomy] 🏷️ Promoted "${stemKey}" → "${officialName}" (${group.totalCount} axioms)`);
                    }
                } else {
                    const isStale = (now - group.oldest) > sevenDaysMs;
                    const isHotMemory = (now - group.lastTouch) < twentyFourHoursMs;
                    if (isStale && !isHotMemory) {
                        for (const tag of group.originalTags) {
                            db.prepare("UPDATE events SET domain = 'General' WHERE domain = ?").run(tag);
                        }
                        logger.info(`[Pipeline/Taxonomy] 🗑️ GC'd stale Unknown tag(s): ${group.originalTags.join(", ")}`);
                    }
                }
            }
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[Pipeline/Taxonomy] Error: ${errMsg}`);
        }
    }
}

// ===========================
// Step 5: WAL Checkpoint
// ===========================

export class WALCheckpointStep implements ConsolidationStep {
    readonly stepName = 'WALCheckpoint';
    readonly #deps: StepDependencies;

    constructor(deps: StepDependencies) { this.#deps = deps; }

    async execute(_ctx: ConsolidationContext): Promise<void> {
        const events = _ctx.sharedState.events as EventBrick[] | undefined;
        if (!events || events.length === 0) return;
        try {
            this.#deps.structuredMemory.getDb().exec("PRAGMA wal_checkpoint(PASSIVE)");
            logger.debug("[Pipeline/WALCheckpoint] WAL checkpoint (PASSIVE) completed.");
        } catch { /* non-critical */ }
    }
}

// ===========================
// Step 6: Process DLQ
// ===========================

export class ProcessDLQStep implements ConsolidationStep {
    readonly stepName = 'ProcessDLQ';
    readonly #deps: StepDependencies;

    constructor(deps: StepDependencies) { this.#deps = deps; }

    async execute(_ctx: ConsolidationContext): Promise<void> {
        const events = _ctx.sharedState.events as EventBrick[] | undefined;
        if (!events || events.length === 0) return;
        await this.#deps.structuredMemory.processDLQ();
    }
}

// ===========================
// Step 7: Ebbinghaus Decay
// ===========================

export class EbbinghausDecayStep implements ConsolidationStep {
    readonly stepName = 'EbbinghausDecay';
    readonly #deps: StepDependencies;

    constructor(deps: StepDependencies) { this.#deps = deps; }

    async execute(_ctx: ConsolidationContext): Promise<void> {
        const events = _ctx.sharedState.events as EventBrick[] | undefined;
        if (!events || events.length === 0) return;
        try {
            const decay = await this.#deps.structuredMemory.applyMemoryDecay();
            if (decay.decayed > 0 || decay.archived > 0) {
                logger.info(`[Pipeline/Ebbinghaus] Decayed: ${decay.decayed}, Archived: ${decay.archived}`);
            }
        } catch (decayErr: unknown) {
            const errMsg = decayErr instanceof Error ? decayErr.message : String(decayErr);
            logger.warn(`[Pipeline/Ebbinghaus] Decay failed (non-critical): ${errMsg}`);
        }
    }
}

// ===========================
// Step 8: GraphRAG Community Summaries
// ===========================

export class GraphRAGStep implements ConsolidationStep {
    readonly stepName = 'GraphRAGCommunity';
    readonly #deps: StepDependencies;

    constructor(deps: StepDependencies) { this.#deps = deps; }

    async execute(_ctx: ConsolidationContext): Promise<void> {
        const events = _ctx.sharedState.events as EventBrick[] | undefined;
        if (!events || events.length === 0) return;
        try {
            await this.#deps.structuredMemory.graph.buildCommunitySummaries(
                this.#deps.aiClient,
                this.#deps.embeddingService,
                (record) => this.#deps.structuredMemory.upsertVector(record)
            );
        } catch (graphErr: unknown) {
            const errMsg = graphErr instanceof Error ? graphErr.message : String(graphErr);
            logger.error(`[Pipeline/GraphRAG] Failed to build community summaries: ${errMsg}`);
        }
    }
}

// ===========================
// Step 9: Snapshot Backup
// ===========================

export class SnapshotBackupStep implements ConsolidationStep {
    readonly stepName = 'SnapshotBackup';
    readonly #deps: StepDependencies;

    constructor(deps: StepDependencies) { this.#deps = deps; }

    async execute(_ctx: ConsolidationContext): Promise<void> {
        const events = _ctx.sharedState.events as EventBrick[] | undefined;
        if (!events || events.length === 0) return;
        try {
            await this.#deps.structuredMemory.createSnapshotBackup();
        } catch { /* logged inside createSnapshotBackup */ }
    }
}

// ===========================
// Factory: Create full pipeline step array
// ===========================

/**
 * Creates the standard 9-step consolidation pipeline.
 * (FetchAndGate is step 0; the remaining 8 post-processing steps follow.)
 */
export function createConsolidationSteps(deps: StepDependencies, force: boolean = false): ConsolidationStep[] {
    return [
        new FetchAndGateStep(deps, force),       // Step 1: Fetch events + energy gating
        new ProcessSessionsStep(deps),            // Step 2: LLM synthesis → L2/L3
        new GCOldEventsStep(deps),                // Step 3: GC old events
        new DynamicTaxonomyStep(deps),            // Step 4: Taxonomy auto-expansion
        new WALCheckpointStep(deps),              // Step 5: WAL checkpoint
        new ProcessDLQStep(deps),                 // Step 6: Process DLQ
        new EbbinghausDecayStep(deps),            // Step 7: Ebbinghaus decay
        new GraphRAGStep(deps),                   // Step 8: GraphRAG community
        new SnapshotBackupStep(deps),             // Step 9: Snapshot backup
    ];
}
