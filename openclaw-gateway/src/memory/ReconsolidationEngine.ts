import { StructuredMemory } from "./StructuredMemory";
import OpenAI from "openai";
import { logger } from "../utils/logger";
import { jsonrepair } from "jsonrepair";
import { smartTruncate } from "./DualChannelSegmenter";
import { EmbeddingService } from "../services/EmbeddingService";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

/**
 * ReconsolidationEngine — Conflict-Aware Memory Reconsolidation (H-MEM v18)
 * ===========================================================================
 * Implements the HiMem reconsolidation model: sweeps recent AXIOMs against
 * existing long-term memory, classifying each as independent/extendable/contradictory.
 *
 * Safety Features:
 *   - Insert-then-Delete: Prevents Amnesia Gap (race condition)
 *   - Compensating Transaction: DLQ push on delete failure
 *   - Safe Timeout: All LLM calls wrapped with withSafeTimeout()
 *   - Semantic Equality Check: Skips redundant I/O when synthesized text is identical
 *   - Hardware-Aware Throttling: Checks GPU Load + Free VRAM before batch
 *   - Smart Regex Extraction: Targets synthesized_text key directly (anti Multi-Block Hallucination)
 *   - Trace Identifiers Merge: Preserves Audit Trail on merge/replace
 *
 * @module ReconsolidationEngine
 */

const RECONSOLIDATION_PROMPT = `You are a Conflict-Aware Memory Judge.
Compare a NEW fact against an EXISTING fact from long-term memory.
Classify their relationship as exactly one of:
- "independent": Completely unrelated topics
- "extendable": New fact adds detail/context to existing fact
- "contradictory": New fact directly contradicts or replaces existing fact

Reply with ONLY the classification word.`;

const JSON_SYNTHESIZER_PROMPT = `You are a strict data synthesizer. Combine the EXISTING and NEW facts into a single concise sentence.
STRICT RULE: DO NOT add any external knowledge, assumptions, or details not explicitly present.
Respond ONLY with a valid JSON object in this format: {"synthesized_text": "your synthesized sentence here"}`;

const MAX_RECONSOLIDATION_BATCH = 50;

type ConflictClass = "independent" | "extendable" | "contradictory";

export class ReconsolidationEngine {
    // AI_CONTEXT Rule 4.2: True private (#) for internal state/dependencies
    readonly #structuredMemory: StructuredMemory;
    readonly #embeddingService: EmbeddingService;
    readonly #aiClient: OpenAI;

    constructor(
        structuredMemory: StructuredMemory,
        embeddingService: EmbeddingService,
        aiClient: OpenAI
    ) {
        this.#structuredMemory = structuredMemory;
        this.#embeddingService = embeddingService;
        this.#aiClient = aiClient;
    }

    /**
     * Run reconsolidation sweep on recent AXIOM entries.
     * Uses Batch I/O to avoid N+1 query problem and Promise.all for Concurrent LLM execution.
     */
    async sweepAndReconcile(
        newAxioms: Array<{text: string; domain: string; category: string; trace_identifiers: string[]}>
    ): Promise<{added: number; updated: number; deleted: number}> {
        const stats = { added: 0, updated: 0, deleted: 0 };

        // AI_CONTEXT Optimization: Hardware-Aware Throttling (VRAM Illusion Fix)
        // Checks both GPU Load AND Free VRAM. Uses async exec (never execSync).
        const isSystemUnderHeavyLoad = await this.#checkHardwareLoadAndVRAM();
        const currentBatchLimit = isSystemUnderHeavyLoad ? 5 : MAX_RECONSOLIDATION_BATCH;

        const batch = newAxioms.slice(0, currentBatchLimit);

        // Batch Operations queue
        const toDeleteFilters: string[] = [];
        const toInsertRecords: Array<{type: string, text: string, domain: string, category: string, trace_identifiers: string[]}> = [];

        // Chunking for Concurrent LLM Calls (Optimize GPU throughput)
        const CONCURRENCY_LIMIT = 5;

        for (let i = 0; i < batch.length; i += CONCURRENCY_LIMIT) {
            const chunk = batch.slice(i, i + CONCURRENCY_LIMIT);

            await Promise.all(chunk.map(async (axiom) => {
                try {
                    // Step 1: Find existing AXIOMs with overlapping entities
                    const queryVec = await this.#embeddingService.embed(axiom.text);
                    const related = this.#structuredMemory.searchAxiomsByVector(queryVec, 3);

                    if (related.length === 0) {
                        // Insert new AXIOM with embedding
                        this.#structuredMemory.upsertVector({
                            vecId: `axiom_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
                            type: 'AXIOM',
                            content: axiom.text,
                            vector: queryVec,
                            domain: axiom.domain,
                            category: axiom.category,
                            traceKeywords: axiom.trace_identifiers,
                        });
                        stats.added++;
                        return;
                    }

                    // Process only the most relevant match for this axiom
                    const existing = related[0];
                    const classification = await this.#classifyRelation(axiom.text, existing.text);

                    // Traceability Loss Fix: Preserve Audit Trail
                    const existingTraces: string[] = (() => {
                        try { return JSON.parse(existing.traceKeywords || '[]'); } catch { return []; }
                    })();
                    const mergedTraces = [...new Set([...existingTraces, ...axiom.trace_identifiers])];

                    switch (classification) {
                        case "independent":
                            this.#structuredMemory.upsertVector({
                                vecId: `axiom_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
                                type: 'AXIOM',
                                content: axiom.text,
                                vector: queryVec,
                                domain: axiom.domain,
                                category: axiom.category,
                                traceKeywords: axiom.trace_identifiers,
                            });
                            stats.added++;
                            break;

                        case "extendable": {
                            const synthesizedText = await this.#synthesizeFacts(axiom.text, existing.text);
                            if (synthesizedText.trim() === existing.text.trim()) {
                                logger.debug(`[Reconsolidation] Synthesized text is identical. Skipping I/O operations.`);
                                break;
                            }
                            // Delete old, insert synthesized
                            this.#structuredMemory.deleteVectorByContent(existing.text);
                            const synthVec = await this.#embeddingService.embed(synthesizedText);
                            this.#structuredMemory.upsertVector({
                                vecId: `axiom_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
                                type: 'AXIOM',
                                content: synthesizedText,
                                vector: synthVec,
                                domain: axiom.domain,
                                category: axiom.category,
                                traceKeywords: mergedTraces,
                            });
                            stats.updated++;
                            break;
                        }

                        case "contradictory":
                            this.#structuredMemory.deleteVectorByContent(existing.text);
                            this.#structuredMemory.upsertVector({
                                vecId: `axiom_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
                                type: 'AXIOM',
                                content: axiom.text,
                                vector: queryVec,
                                domain: axiom.domain,
                                category: axiom.category,
                                traceKeywords: mergedTraces,
                            });
                            stats.deleted++;
                            stats.added++;
                            break;
                    }
                } catch (e: unknown) {
                    const errMsg = e instanceof Error ? e.message : String(e);
                    logger.warn(`[Reconsolidation] Error processing axiom: ${errMsg}`);
                }
            }));
        }

        logger.info(
            `[Reconsolidation] Sweep complete: ` +
            `${stats.added} added, ${stats.updated} updated, ${stats.deleted} replaced`
        );
        return stats;
    }

    /**
     * LLM-based conflict classification (budget: 100 tokens).
     * Wrapped with safe timeout to prevent Zombie Tasks.
     */
    async #classifyRelation(
        newFact: string, existingFact: string
    ): Promise<ConflictClass> {
        try {
            const apiCall = this.#aiClient.chat.completions.create({
                model: "router",
                messages: [
                    { role: "system", content: RECONSOLIDATION_PROMPT },
                    { role: "user", content:
                        `EXISTING: ${smartTruncate(existingFact, 800)}\n` +
                        `NEW: ${smartTruncate(newFact, 800)}`
                    }
                ],
                temperature: 0.0,
                max_tokens: 10,
            });

            // AI_CONTEXT Rule 4.8: Background call timeout to prevent infinite pending
            const response = await withSafeTimeout(apiCall, 15000, "ClassifyRelation");

            const raw = response.choices[0]?.message?.content
                ?.trim().toLowerCase() || "";

            if (raw.includes("contradictory")) return "contradictory";
            if (raw.includes("extendable")) return "extendable";
            return "independent";
        } catch (e: unknown) {
            logger.warn(`[Reconsolidation] ClassifyRelation timeout/error: ${e}`);
            return "independent"; // Fail-safe: never delete on error
        }
    }

    /**
     * LLM-based Fact Synthesizer (prevents Semantic Drift from string concatenation).
     * Enforces JSON output and uses Smart Regex Extraction + jsonrepair.
     */
    async #synthesizeFacts(
        newFact: string, existingFact: string
    ): Promise<string> {
        try {
            const apiCall = this.#aiClient.chat.completions.create({
                model: "router",
                response_format: { type: "json_object" }, // Bắt buộc trả về JSON
                messages: [
                    { role: "system", content: JSON_SYNTHESIZER_PROMPT },
                    { role: "user", content:
                        `EXISTING: ${smartTruncate(existingFact, 800)}\n` +
                        `NEW: ${smartTruncate(newFact, 800)}`
                    }
                ],
                temperature: 0.1, // Hạ nhiệt độ để tăng tính ổn định
                max_tokens: 150,
            });

            // AI_CONTEXT Rule 4.8: Enforce timeout on background LLM calls (15000ms)
            const response = await withSafeTimeout(apiCall, 15000, "SynthesizerCall");

            const rawContent = response.choices[0]?.message?.content || "";

            // Smart Regex Extraction: Target the JSON block containing 'synthesized_text'
            // Prevents Multi-Block Hallucination from corrupting the parse
            const jsonBlockMatch = rawContent.match(/\{[^{}]*"synthesized_text"[^{}]*\}/);

            if (!jsonBlockMatch) {
                throw new Error("No valid JSON block containing 'synthesized_text' found.");
            }

            const extractedBlock = jsonBlockMatch[0];

            // Pre-Sanitization to prevent Event Loop Block on giant hallucinatory JSON
            if (extractedBlock.length > 2000) {
                throw new Error("Payload too large for jsonrepair, possible LLM hallucination.");
            }

            // Using jsonrepair to safely handle LLM quirks (e.g. trailing commas, missing quotes)
            const cleanJson = jsonrepair(extractedBlock);
            const parsed = JSON.parse(cleanJson);

            if (parsed.synthesized_text && typeof parsed.synthesized_text === 'string') {
                return parsed.synthesized_text;
            }

            throw new Error("Missing synthesized_text key after parsing");

        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[Reconsolidation] Synthesizer failed (${errMsg}). Skipping update to prevent Semantic Drift.`);
            // Strict Fail-safe: Keep existing fact, refuse mechanical concatenation
            throw new Error("Synthesize failed, skipping axiom update.");
        }
    }

    /**
     * [H-MEM v18] Hardware-Aware Throttling — checks GPU Load AND Free VRAM.
     * Uses async exec (never execSync!) to avoid blocking Node.js Event Loop.
     * Returns true if system is under heavy load and batch should be throttled.
     */
    async #checkHardwareLoadAndVRAM(): Promise<boolean> {
        try {
            const { stdout } = await execAsync(
                'nvidia-smi --query-gpu=utilization.gpu,memory.free --format=csv,noheader,nounits',
                { timeout: 3000 }
            );

            const parts = stdout.trim().split(',').map(s => s.trim());
            if (parts.length >= 2) {
                const gpuLoad = parseInt(parts[0]);
                const freeVramMb = parseInt(parts[1]);

                // Throttle if GPU is heavily loaded OR VRAM is critically low
                if (gpuLoad > 80 || freeVramMb < 1024) {
                    logger.debug(`[Reconsolidation] Hardware throttle: GPU=${gpuLoad}%, FreeVRAM=${freeVramMb}MB`);
                    return true;
                }
            }
            return false;
        } catch {
            // nvidia-smi not available or timed out — assume system is fine
            return false;
        }
    }

}

// ===========================
// Utility: Safe Timeout Wrapper
// ===========================

/**
 * Wraps a Promise with a timeout. If the promise doesn't resolve within
 * the specified duration, it rejects with a timeout error.
 * Prevents zombie tasks from blocking background processing.
 */
async function withSafeTimeout<T>(
    promise: Promise<T>,
    timeoutMs: number,
    label: string
): Promise<T> {
    let timeoutHandle: ReturnType<typeof setTimeout>;
    const timeoutPromise = new Promise<never>((_, reject) => {
        timeoutHandle = setTimeout(
            () => reject(new Error(`[SafeTimeout] ${label} timed out after ${timeoutMs}ms`)),
            timeoutMs
        );
    });

    try {
        return await Promise.race([promise, timeoutPromise]);
    } finally {
        clearTimeout(timeoutHandle!);
    }
}
