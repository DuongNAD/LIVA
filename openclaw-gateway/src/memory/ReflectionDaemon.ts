import { StructuredMemory, type EventBrick } from "./StructuredMemory";
import { DualChannelSegmenter } from "./DualChannelSegmenter";
import { EmbeddingService } from "../services/EmbeddingService";
import { logger } from "../utils/logger";
import { safeExtractJSON } from "../utils/JsonExtractor";
import OpenAI from "openai";
import { v4 as uuidv4 } from "uuid";
import { jsonrepair } from "jsonrepair";
import { z } from "zod";
import { memoryEvents } from "./MemoryEventBus";

/**
 * ReflectionDaemon — Dual-Perspective Event Extraction (Φ/Ψ) [H-MEM v18]
 * =========================================================================
 * Runs asynchronously after conversation turns to extract structured
 * event bricks from raw dialogue. Uses topic-aware DualChannelSegmenter
 * for intelligent episode boundary detection.
 *
 * Dual Perspectives:
 *   Φ (Phi) — Factual: objective facts, entities, timestamps, domain classification
 *   Ψ (Psi) — Relational: sentiment, intent, psychological subtext, topic summary
 *
 * H-MEM v18 Enhancements:
 *   - DualChannelSegmenter integration for topic-aware episode boundaries
 *   - Domain classification with Seed Domain list (Dynamic Taxonomy)
 *   - Category routing tags for hierarchical search
 *   - Trace identifiers for audit trail
 *   - Strict Zod validation (no .default() for classification — anti Lazy LLM)
 *
 * Safety Features:
 *   - Debounced: waits 12s idle before processing batch
 *   - Micro-batching: groups multiple turns into single LLM call
 *   - Fire-and-forget: never blocks main conversation flow
 *   - flushPending(): graceful shutdown support (CoreKernel.shutdown)
 *   - dispose(): timer cleanup to prevent zombie intervals
 *
 * @module ReflectionDaemon
 */

// ===========================
// Seed Domain Registry (H-MEM v18 Dynamic Taxonomy)
// ===========================

const SEED_DOMAINS = ["Development", "Personal", "Security", "Finance", "Entertainment", "General"];

// ===========================
// Extraction Prompt (H-MEM v18)
// ===========================

const DUAL_EXTRACTION_PROMPT = `You are a dual-perspective event extraction system. Analyze the conversation and extract 2 aspects:

**Φ (Factual — Objective Data):**
- fact: Specific fact mentioned (who, what, when, where)
- entity: Proper nouns, place names, organizations (if any)
- confidence: Information confidence (0.0 - 1.0)
- domain_classification: Classify into ONE of these domains: ${SEED_DOMAINS.join(", ")}. If none fit, use "Unknown_{keyword}" (e.g. "Unknown_Health").
- category_routing_tag: Sub-category within the domain (e.g. "TypeScript", "Diet", "Git")
- trace_identifiers: Array of key terms that prove this fact's origin (e.g. ["project LIVA", "RTX 5060 Ti"])

**Ψ (Relational — Psychology & Relations):**
- relation: Social relationship mentioned
- sentiment: Overall emotional tone
- intent: User's underlying intent
- topic_summary: One-sentence summary of the conversation topic

RETURN EXACT JSON:
{"factual_entries":[{"fact":"...","entity":"...","confidence":0.9,"domain_classification":"Development","category_routing_tag":"TypeScript","trace_identifiers":["LIVA","Gateway"]}],"relational_entries":[{"relation":"...","sentiment":"...","intent":"...","topic_summary":"..."}]}

IMPORTANT: Return pure JSON only, NO markdown, NO explanation. You MUST fill domain_classification and category_routing_tag with meaningful values — do NOT leave them empty.`;

// ===========================
// Zod Schemas (H-MEM v18 — No .default() for classification to prevent Lazy LLM)
// ===========================

const FactualEntrySchema = z.object({
    fact: z.string(),
    entity: z.string().optional(),
    confidence: z.number().min(0).max(1).default(0.8),
    domain_classification: z.string().min(1),
    category_routing_tag: z.string().min(1),
    trace_identifiers: z.array(z.string()).default([]),
});

const RelationalEntrySchema = z.object({
    relation: z.string(),
    sentiment: z.string(),
    intent: z.string(),
    topic_summary: z.string().optional(),
});

const DualExtractionSchema = z.object({
    factual_entries: z.array(FactualEntrySchema),
    relational_entries: z.array(RelationalEntrySchema),
});

// ===========================
// Constants
// ===========================

/** Wait 12 seconds of idle before processing batch */
const DEBOUNCE_MS = 12_000;

/** Maximum messages to batch in a single extraction call */
const MAX_BATCH_SIZE = 5;

// ===========================
// Types
// ===========================

interface PendingTurn {
    userMsg: string;
    aiReply: string;
    timestamp: number;
}

// ===========================
// Main Class
// ===========================

export class ReflectionDaemon {
    readonly #structuredMemory: StructuredMemory;
    readonly #aiClient: OpenAI;
    readonly #segmenter: DualChannelSegmenter | null;
    readonly #embeddingService: EmbeddingService;
    #pendingQueue: PendingTurn[] = [];
    #currentEpisode: PendingTurn[] = [];
    #debounceTimer: NodeJS.Timeout | null = null;
    #isProcessing = false;

    constructor(
        structuredMemory: StructuredMemory,
        aiClient: OpenAI,
        segmenter?: DualChannelSegmenter,
        embeddingService?: EmbeddingService
    ) {
        this.#structuredMemory = structuredMemory;
        this.#aiClient = aiClient;
        this.#segmenter = segmenter ?? null;
        this.#embeddingService = embeddingService ?? EmbeddingService.getInstance();
    }

    /**
     * Queue a conversation turn for background reflection.
     * If DualChannelSegmenter is available, uses topic-aware episode boundaries.
     * Otherwise falls back to debounced micro-batching.
     * Called from AgentLoop after each turn completes.
     */
    public queueTurn(userMsg: string, aiReply: string): void {
        // Skip trivial messages
        if (!userMsg || userMsg.length < 10) return;
        if (/^(hi|hello|ok|oke|được|vâng|dạ)\s*$/i.test(userMsg.trim())) return;

        const turn: PendingTurn = {
            userMsg,
            aiReply,
            timestamp: Date.now(),
        };

        // [H-MEM v18] If segmenter is available, check for episode boundary
        if (this.#segmenter) {
            this.#currentEpisode.push(turn);

            // Fire-and-forget: Check episode boundary asynchronously
            this.#checkEpisodeBoundary(turn).catch(e => {
                logger.warn(`[ReflectionDaemon] Episode boundary check failed: ${e.message}`);
            });
        } else {
            // Legacy mode: debounced micro-batching
            this.#pendingQueue.push(turn);
            this.#scheduleBatch();
        }
    }

    /**
     * [H-MEM v18] Check if the new turn creates an episode boundary.
     * If so, flush the current episode and start a new one.
     */
    async #checkEpisodeBoundary(turn: PendingTurn): Promise<void> {
        if (!this.#segmenter) return;

        try {
            const embedding = await this.#embeddingService.embed(turn.userMsg);
            const recentContext = this.#currentEpisode
                .slice(-3)
                .map(t => `User: ${t.userMsg}\nAI: ${t.aiReply.substring(0, 300)}`)
                .join("\n");

            const shouldSplit = await this.#segmenter.shouldCreateNewEpisode(
                turn.userMsg,
                embedding,
                recentContext,
                'user' // User message triggers boundary detection
            );

            if (shouldSplit && this.#currentEpisode.length > 1) {
                // Extract from the completed episode (excluding the boundary turn)
                const completedEpisode = this.#currentEpisode.slice(0, -1);
                this.#currentEpisode = [turn]; // Start new episode with boundary turn

                // Reset segmenter cluster
                this.#segmenter.resetCluster(embedding);

                // Process the completed episode
                this.#pendingQueue.push(...completedEpisode);
                await this.processBatch();
            }
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[ReflectionDaemon] Episode check error (non-critical): ${errMsg}`);
        }
    }

    #scheduleBatch(): void {
        // Reset debounce timer (zombie timer prevention)
        if (this.#debounceTimer) {
            clearTimeout(this.#debounceTimer);
            this.#debounceTimer = null;
        }

        this.#debounceTimer = setTimeout(() => {
            this.processBatch().catch(e => {
                logger.warn(`[ReflectionDaemon] Background batch failed: ${e.message}`);
            });
        }, DEBOUNCE_MS);
    }

    /**
     * Process pending turns in batch.
     * Groups up to MAX_BATCH_SIZE turns into a single LLM extraction call.
     */
    private async processBatch(): Promise<void> {
        if (this.#isProcessing) {
            return;
        }
        this.#isProcessing = true;

        try {
            // Take batch from queue
            const batch = this.#pendingQueue.splice(0, MAX_BATCH_SIZE);
            if (batch.length === 0) return;

            // Build conversation context for extraction
            const conversationText = batch
                .map((t, i) => `[Turn ${i + 1}]\nUser: ${t.userMsg}\nLIVA: ${t.aiReply.substring(0, 500)}`)
                .join("\n\n");

            // Call Router LLM for dual extraction
            const response = await this.#aiClient.chat.completions.create({
                model: "router",
                messages: [
                    { role: "system", content: DUAL_EXTRACTION_PROMPT },
                    { role: "user", content: conversationText },
                ],
                temperature: 0.1,
                max_tokens: 500,
            });

            const raw = response.choices[0]?.message?.content?.trim();
            if (!raw || raw.length < 5) {
                logger.debug("[ReflectionDaemon] LLM returned empty extraction, skipping batch.");
                return;
            }

            // Safe JSON extraction (handles markdown wrapping + LLM hallucination)
            const extractedJson = safeExtractJSON<any>(raw);
            if (!extractedJson) {
                logger.warn(`[ReflectionDaemon] JSON extraction failed, skipping: ${raw.substring(0, 100)}`);
                return;
            }

            const parsed = DualExtractionSchema.safeParse(extractedJson);
            if (!parsed.success) {
                // [H-MEM v18] Fallback: try with default domain/category if Zod validation fails
                // This handles the case where local LLM omits the new fields
                const fallbackJson = {
                    ...extractedJson,
                    factual_entries: (extractedJson.factual_entries || []).map((f: any) => ({
                        ...f,
                        domain_classification: f.domain_classification || "General",
                        category_routing_tag: f.category_routing_tag || "Uncategorized",
                        trace_identifiers: f.trace_identifiers || [],
                    })),
                };
                const retryParse = DualExtractionSchema.safeParse(fallbackJson);
                if (!retryParse.success) {
                    logger.warn(`[ReflectionDaemon] Zod validation failed after fallback: ${retryParse.error.message}`);
                    return;
                }
                // Use fallback parse result
                this.#createEventBricks(retryParse.data, batch);
                return;
            }

            this.#createEventBricks(parsed.data, batch);

            logger.info(`[ReflectionDaemon] ✅ Extracted Φ/Ψ from ${batch.length} turn(s). Facts: ${parsed.data.factual_entries.length}, Domain: ${parsed.data.factual_entries[0]?.domain_classification ?? 'N/A'}`);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            // Never crash main flow — reflection is best-effort
            logger.warn(`[ReflectionDaemon] Extraction error (non-critical): ${errMsg}`);
        } finally {
            this.#isProcessing = false;

            // If more items queued during processing, schedule next batch
            if (this.#pendingQueue.length > 0) {
                this.#scheduleBatch();
            }
        }
    }

    /**
     * Create and persist EventBrick records from parsed extraction data.
     */
    #createEventBricks(
        extracted: z.infer<typeof DualExtractionSchema>,
        batch: PendingTurn[]
    ): void {
        for (const turn of batch) {
            const event: EventBrick = {
                eventId: uuidv4(),
                timestamp: turn.timestamp,
                phi: {
                    facts: extracted.factual_entries.map(f => f.fact),
                    entities: extracted.factual_entries.map(f => f.entity || "").filter(Boolean),
                },
                psi: {
                    sentiment: extracted.relational_entries[0]?.sentiment || "bình thường",
                    intent: extracted.relational_entries[0]?.intent || "chitchat",
                    relational: extracted.relational_entries[0]?.relation || "",
                },
                rawUserMsg: turn.userMsg.substring(0, 2000),
                rawAiReply: turn.aiReply.substring(0, 2000),
                // [H-MEM v18] Hierarchical metadata
                domain: extracted.factual_entries[0]?.domain_classification || "General",
                category: extracted.factual_entries[0]?.category_routing_tag || "Uncategorized",
                traceKeywords: extracted.factual_entries.flatMap(f => f.trace_identifiers),
            };

            this.#structuredMemory.insertEvent(event);

            // [UHM] Emit passive activity signal via EventBus (decoupled from ConsolidationCron)
            memoryEvents.emit('NEW_TURN');
        }
    }

    /**
     * Flush all pending turns immediately (for graceful shutdown).
     * Called from CoreKernel.shutdown() to prevent data loss.
     */
    public async flushPending(): Promise<void> {
        if (this.#debounceTimer) {
            clearTimeout(this.#debounceTimer);
            this.#debounceTimer = null;
        }

        // Also flush current episode if using segmenter
        if (this.#currentEpisode.length > 0) {
            this.#pendingQueue.push(...this.#currentEpisode);
            this.#currentEpisode = [];
        }

        if (this.#pendingQueue.length > 0) {
            logger.info(`[ReflectionDaemon] Flushing ${this.#pendingQueue.length} pending turn(s) before shutdown...`);
            await this.processBatch();
        }
    }

    /**
     * Clean up timers to prevent zombie intervals.
     * MUST be called in CoreKernel.shutdown().
     */
    public dispose(): void {
        if (this.#debounceTimer) {
            clearTimeout(this.#debounceTimer);
            this.#debounceTimer = null;
        }
        this.#pendingQueue = [];
        this.#currentEpisode = [];
        logger.info("[ReflectionDaemon] Disposed. Timers cleared.");
    }

    /**
     * Get count of pending (unbatched) turns (for diagnostics).
     */
    public get pendingCount(): number {
        return this.#pendingQueue.length + this.#currentEpisode.length;
    }

}
