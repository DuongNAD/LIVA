/**
 * VirtualManager — Zero-VRAM Context Orchestration
 * ==================================================
 * Replaces the need for a 32B Manager model.
 * Queries SemanticRouter + LanceDB + StructuredMemory in PARALLEL
 * and packages a Context Workflow JSON for the Planner (Router Model).
 *
 * Architecture:
 *   - 0 VRAM — runs entirely on Node.js main thread
 *   - Promise.all() for parallel I/O (LanceDB + SQLite)
 *   - Chitchat Fast-Track Bypass (<1ms, skips all DB queries)
 *   - Graceful degradation — never crashes, always returns a workflow
 *
 * @module VirtualManager
 */

import { SemanticRouter, type MemoryRoute, type RouteResult } from "../memory/SemanticRouter";
import { LanceMemoryManager } from "../memory/LanceMemory";
import { StructuredMemory } from "../memory/StructuredMemory";
import { logger } from "../utils/logger";

// ===========================
// Types
// ===========================

export interface ContextWorkflow {
    /** The classified route for this query */
    route: MemoryRoute;
    /** Semantic anchors retrieved from LanceDB (episodic memories) */
    anchors: string[];
    /** Structured facts formatted for system prompt injection */
    facts: string;
    /** Unix timestamp when this workflow was constructed */
    timestamp: number;
    /** Time taken to build this workflow (ms) */
    buildTimeMs: number;
}

// ===========================
// Main Class
// ===========================

export class VirtualManager {
    readonly #semanticRouter: SemanticRouter;
    readonly #lanceMemory: LanceMemoryManager | null;
    readonly #structuredMemory: StructuredMemory;

    constructor(
        semanticRouter: SemanticRouter,
        structuredMemory: StructuredMemory,
        lanceMemory?: LanceMemoryManager | null,
    ) {
        this.#semanticRouter = semanticRouter;
        this.#structuredMemory = structuredMemory;
        this.#lanceMemory = lanceMemory ?? null;
    }

    /**
     * Build a Context Workflow for the Planner model.
     *
     * Flow:
     *   1. Route query via SemanticRouter (regex fast-track or cosine similarity)
     *   2. If chitchat → immediate bypass, zero DB queries
     *   3. If system_command → minimal context (structured facts only)
     *   4. Otherwise → parallel LanceDB + StructuredMemory queries via Promise.all()
     *
     * Performance:
     *   - Chitchat:        <1ms  (regex fast-track, zero I/O)
     *   - System command:  ~5ms  (SQLite only)
     *   - Full pipeline:   ~150ms max (parallel LanceDB + SQLite)
     *
     * @param userQuery  Raw user text input
     * @returns          Context workflow JSON for Planner consumption
     */
    public async buildContextWorkflow(userQuery: string): Promise<ContextWorkflow> {
        const startTime = performance.now();

        // 1. Route query (CPU-only: <1ms regex fast-track or <50ms embedding)
        let routeResult: RouteResult;
        try {
            routeResult = await this.#semanticRouter.route(userQuery);
        } catch (e: any) {
            logger.warn(`[VirtualManager] Router failed, fallback to deep_reasoning: ${e.message}`);
            routeResult = { route: "deep_reasoning", confidence: 0 };
        }

        // ⚡ FAST-TRACK BYPASS: Chitchat → ngắt mạch, không quét DB
        if (routeResult.route === "chitchat") {
            const buildTimeMs = performance.now() - startTime;
            logger.debug(`[VirtualManager] ⚡ Chitchat bypass (${buildTimeMs.toFixed(1)}ms)`);
            return {
                route: "chitchat",
                anchors: [],
                facts: "",
                timestamp: Date.now(),
                buildTimeMs,
            };
        }

        // ⚡ FAST-TRACK: system_command → chỉ cần structured facts, skip LanceDB
        if (routeResult.route === "system_command") {
            const facts = this.#structuredMemory.formatForSystemPrompt();
            const buildTimeMs = performance.now() - startTime;
            logger.debug(`[VirtualManager] ⚡ System command fast-track (${buildTimeMs.toFixed(1)}ms)`);
            return {
                route: "system_command",
                anchors: [],
                facts,
                timestamp: Date.now(),
                buildTimeMs,
            };
        }

        // 2. PARALLEL I/O — Promise.all() thay vì sequential await
        //    LanceDB ~150ms + SQLite ~5ms → chạy song song = max(150, 5) ≈ 150ms
        const [anchors, facts] = await Promise.all([
            this.#searchAnchors(userQuery),
            Promise.resolve(this.#structuredMemory.formatForSystemPrompt()),
        ]);

        const buildTimeMs = performance.now() - startTime;
        logger.debug(
            `[VirtualManager] 🧠 Full context built: route=${routeResult.route}, ` +
            `anchors=${anchors.length}, facts=${facts.length > 0 ? "yes" : "none"} (${buildTimeMs.toFixed(1)}ms)`
        );

        return {
            route: routeResult.route,
            anchors,
            facts,
            timestamp: Date.now(),
            buildTimeMs,
        };
    }

    /**
     * Search LanceDB for relevant episodic memories.
     * Graceful: returns [] on any failure (LanceDB not connected, etc.)
     */
    async #searchAnchors(query: string): Promise<string[]> {
        if (!this.#lanceMemory) return [];
        try {
            return await this.#lanceMemory.searchMemory(query, 5);
        } catch (e: any) {
            logger.warn(`[VirtualManager] LanceDB search failed (non-fatal): ${e.message}`);
            return [];
        }
    }
}
