import { StructuredMemory, type EventBrick } from "./StructuredMemory";
import { logger } from "../utils/logger";
import { safeExtractJSON } from "../utils/JsonExtractor";
import OpenAI from "openai";
import { v4 as uuidv4 } from "uuid";
import { jsonrepair } from "jsonrepair";
import { z } from "zod";

/**
 * ReflectionDaemon — Dual-Perspective Event Extraction (Φ/Ψ)
 * ============================================================
 * Runs asynchronously after conversation turns to extract structured
 * event bricks from raw dialogue. Uses debounced micro-batching to
 * prevent GPU/SQLite contention during rapid-fire messaging.
 *
 * Dual Perspectives:
 *   Φ (Phi) — Factual: objective facts, entities, timestamps
 *   Ψ (Psi) — Relational: sentiment, intent, psychological subtext
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
// Extraction Prompt
// ===========================

const DUAL_EXTRACTION_PROMPT = `Bạn là hệ thống trích xuất sự kiện kép (Dual-Perspective). Phân tích đoạn hội thoại sau và trích xuất 2 khía cạnh:

**Φ (Factual — Dữ kiện khách quan):**
- fact: Sự thật cụ thể được đề cập (ai, cái gì, khi nào, ở đâu)
- entity: Tên riêng, địa danh, tổ chức liên quan (nếu có)
- confidence: Độ tin cậy của thông tin này (0.0 - 1.0)

**Ψ (Relational — Tâm lý & Quan hệ):**
- relation: Mối quan hệ xã hội được nhắc đến
- sentiment: Cảm xúc tổng thể
- intent: Ý định ngầm của người dùng

TRẢ VỀ ĐÚNG JSON:
{"factual_entries":[{"fact":"...","entity":"...","confidence":0.9}],"relational_entries":[{"relation":"...","sentiment":"...","intent":"..."}]}

QUAN TRỌNG: Trả về JSON thuần, KHÔNG markdown, KHÔNG giải thích.`;

// ===========================
// Zod Schemas
// ===========================

const FactualEntrySchema = z.object({
    fact: z.string(),
    entity: z.string().optional(),
    confidence: z.number().min(0).max(1).default(0.8),
});

const RelationalEntrySchema = z.object({
    relation: z.string(),
    sentiment: z.string(),
    intent: z.string(),
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
    #pendingQueue: PendingTurn[] = [];
    #debounceTimer: NodeJS.Timeout | null = null;
    #isProcessing = false;

    constructor(structuredMemory: StructuredMemory, aiClient: OpenAI) {
        this.#structuredMemory = structuredMemory;
        this.#aiClient = aiClient;
    }

    /**
     * Queue a conversation turn for background reflection.
     * Does NOT process immediately — uses debounced micro-batching.
     * Called from AgentLoop after each turn completes.
     */
    public queueTurn(userMsg: string, aiReply: string): void {
        // Skip trivial messages
        if (!userMsg || userMsg.length < 10) return;
        if (/^(hi|hello|ok|oke|được|vâng|dạ)\s*$/i.test(userMsg.trim())) return;

        this.#pendingQueue.push({
            userMsg,
            aiReply,
            timestamp: Date.now(),
        });

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

            // Build conversation context for extraction
            const conversationText = batch
                .map((t, i) => `[Turn ${i + 1}]\nNgười dùng: ${t.userMsg}\nLIVA: ${t.aiReply.substring(0, 500)}`)
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
                logger.warn(`[ReflectionDaemon] Zod validation failed: ${parsed.error.message}`);
                return;
            }

            const extracted = parsed.data;

            // Create EventBrick for each turn in batch (shared Φ/Ψ from batch context)
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
                };

                this.#structuredMemory.insertEvent(event);
            }

            logger.info(`[ReflectionDaemon] ✅ Extracted Φ/Ψ from ${batch.length} turn(s). Facts: ${extracted.factual_entries.length}, Sentiment: ${extracted.relational_entries[0]?.sentiment}`);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            // Never crash main flow — reflection is best-effort
            logger.warn(`[ReflectionDaemon] Extraction error (non-critical): ${errMsg}`);
        } finally {
            this.#isProcessing = false;

            // If more items queued during processing, schedule next batch
            if (this.#pendingQueue.length > 0) {
                if (this.#debounceTimer) {
                    clearTimeout(this.#debounceTimer);
                }
                this.#debounceTimer = setTimeout(() => {
                    this.processBatch().catch(e => {
                        logger.warn(`[ReflectionDaemon] Follow-up batch failed: ${e.message}`);
                    });
                }, DEBOUNCE_MS);
            }
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
        logger.info("[ReflectionDaemon] Disposed. Timers cleared.");
    }

    /**
     * Get count of pending (unbatched) turns (for diagnostics).
     */
    public get pendingCount(): number {
        return this.#pendingQueue.length;
    }

}
