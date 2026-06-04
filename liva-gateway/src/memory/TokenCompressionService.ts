import { logger } from "../utils/logger.js";

/**
 * TokenCompressionService — Multi-Stage Context Compression (Phase 1)
 * ====================================================================
 * Compresses LLM context (RAG chunks, chat history, tool outputs) before
 * they reach the LLM to reduce VRAM usage for KV cache.
 *
 * Pipeline:
 *   Stage 1 — Structural Stripping (whitespace, comments, separators)
 *   Stage 2 — JSON/XML Condensation (schema summary for large arrays)
 *   Stage 3 — Sentence Deduplication (Jaccard similarity ≥ 0.8 → merge)
 *   Stage 4 — Token Budget Enforcement (head+tail truncation fallback)
 *
 * Constraints:
 *   - Token counting: word count × 1.5 (consistent with MemoryManager)
 *   - All I/O is async
 *   - No external dependencies beyond what LIVA already uses
 *   - Zero `any` types — uses `unknown` with narrowing
 *
 * @module TokenCompressionService
 */

// ─── Types ──────────────────────────────────────────────────────────

export interface CompressionResult {
    readonly originalTokens: number;
    readonly compressedTokens: number;
    readonly compressedText: string;
    readonly compressionRatio: number;
    /** Comma-separated list of stages that actually modified the text */
    readonly strategy: string;
}

interface StageResult {
    readonly text: string;
    readonly applied: boolean;
    readonly name: string;
}

// ─── Constants ──────────────────────────────────────────────────────

/** Jaccard similarity threshold — sentences above this are considered duplicates */
const DEDUP_SIMILARITY_THRESHOLD = 0.8;

/** Minimum word count for a sentence to be eligible for dedup (skip trivial lines) */
const MIN_SENTENCE_WORDS = 4;

/** JSON arrays with more than this many similar objects get condensed */
const JSON_ARRAY_CONDENSE_THRESHOLD = 5;

/** Sliding window size for sentence deduplication */
const DEDUP_WINDOW_SIZE = 50;

/** Default target compression ratio (0.6 = reduce to 60% of original tokens) */
const DEFAULT_TARGET_RATIO = 0.6;

// ─── Service ────────────────────────────────────────────────────────

export class TokenCompressionService {
    // AI_CONTEXT Rule 4.2: True private (#) for singleton + internal state
    static #instance: TokenCompressionService | null = null;

    private constructor() {
        logger.debug("[TokenCompression] Service instantiated.");
    }

    /** Singleton accessor — consistent with LIVA service pattern */
    public static getInstance(): TokenCompressionService {
        if (!TokenCompressionService.#instance) {
            TokenCompressionService.#instance = new TokenCompressionService();
        }
        return TokenCompressionService.#instance;
    }

    /**
     * Compress text through the multi-stage pipeline.
     *
     * @param text - Raw context text to compress
     * @param targetRatio - Desired compression ratio (0.0–1.0). Default 0.6.
     *                      0.6 means "reduce to 60% of original tokens".
     * @returns CompressionResult with compressed text and metadata
     */
    public async compress(
        text: string,
        targetRatio: number = DEFAULT_TARGET_RATIO,
    ): Promise<CompressionResult> {
        if (!text || text.trim().length === 0) {
            return {
                originalTokens: 0,
                compressedTokens: 0,
                compressedText: "",
                compressionRatio: 1,
                strategy: "none",
            };
        }

        const originalTokens = estimateTokens(text);
        const tokenBudget = Math.floor(originalTokens * targetRatio);
        const appliedStages: string[] = [];
        let current = text;

        // Stage 1: Structural stripping
        const s1 = this.#stageStructuralStrip(current);
        current = s1.text;
        if (s1.applied) appliedStages.push(s1.name);

        // Early exit if already within budget
        if (estimateTokens(current) <= tokenBudget) {
            return this.#buildResult(text, current, originalTokens, appliedStages);
        }

        // Stage 2: JSON/XML condensation
        const s2 = this.#stageJsonXmlCondense(current);
        current = s2.text;
        if (s2.applied) appliedStages.push(s2.name);

        if (estimateTokens(current) <= tokenBudget) {
            return this.#buildResult(text, current, originalTokens, appliedStages);
        }

        // Stage 3: Sentence deduplication
        const s3 = this.#stageSentenceDedup(current);
        current = s3.text;
        if (s3.applied) appliedStages.push(s3.name);

        if (estimateTokens(current) <= tokenBudget) {
            return this.#buildResult(text, current, originalTokens, appliedStages);
        }

        // Stage 4: Token budget enforcement (head+tail truncation)
        const s4 = this.#stageBudgetEnforce(current, tokenBudget);
        current = s4.text;
        if (s4.applied) appliedStages.push(s4.name);

        return this.#buildResult(text, current, originalTokens, appliedStages);
    }

    /** Shutdown chain compliance — no timers to clean, but follows pattern */
    public dispose(): void {
        logger.debug("[TokenCompression] Disposed.");
        TokenCompressionService.#instance = null;
    }

    // ─── Stage 1: Structural Stripping ──────────────────────────────

    /**
     * Remove excessive whitespace, empty lines, code comments in tool output,
     * and repeated separator lines (===, ---, ***).
     */
    #stageStructuralStrip(text: string): StageResult {
        const original = text;

        // 1a. Collapse runs of blank lines to a single newline
        let result = text.replace(/\n{3,}/g, "\n\n");

        // 1b. Strip trailing whitespace per line
        result = result.replace(/[ \t]+$/gm, "");

        // 1c. Collapse repeated separator lines (===, ---, ***) to one
        result = result.replace(/(?:^[ \t]*[-=*]{3,}[ \t]*$\n?){2,}/gm, (match) => {
            // Keep the first separator line
            const firstLine = match.split("\n").find((l) => l.trim().length > 0);
            return firstLine ? `${firstLine}\n` : "\n";
        });

        // 1d. Strip single-line comments (// ...) from tool/code output blocks
        //     Only strip when surrounded by code-like context (indented or in fenced blocks)
        result = result.replace(/^([ \t]+)\/\/[^\n]*$/gm, "");

        // 1e. Strip multi-line block comments (/* ... */) that span tool output
        result = result.replace(/\/\*[\s\S]*?\*\//g, "");

        // 1f. Collapse runs of spaces/tabs within lines to single space
        result = result.replace(/[ \t]{2,}/g, " ");

        // 1g. Clean up any blank lines created by comment stripping
        result = result.replace(/\n{3,}/g, "\n\n");

        return {
            text: result.trim(),
            applied: result.trim() !== original.trim(),
            name: "structural_strip",
        };
    }

    // ─── Stage 2: JSON/XML Condensation ─────────────────────────────

    /**
     * Detect JSON arrays with >5 similar objects → summarize as schema + first/last.
     * Detect verbose XML → strip boilerplate attributes.
     */
    #stageJsonXmlCondense(text: string): StageResult {
        const original = text;
        let result = text;

        // 2a. Find JSON arrays embedded in text and condense large ones
        result = this.#condenseJsonArrays(result);

        // 2b. Condense verbose XML blocks
        result = this.#condenseXmlBlocks(result);

        return {
            text: result,
            applied: result !== original,
            name: "json_xml_condense",
        };
    }

    /**
     * Find JSON arrays in text, parse them, and condense if they have
     * more than JSON_ARRAY_CONDENSE_THRESHOLD similar objects.
     */
    #condenseJsonArrays(text: string): string {
        // Match top-level JSON arrays: [ ... ]
        // Use a bracket-counting approach rather than greedy regex to avoid catastrophic backtracking
        const arrayPositions = this.#findJsonArrayPositions(text);

        if (arrayPositions.length === 0) return text;

        // Process in reverse order so indices remain valid
        let result = text;
        for (let i = arrayPositions.length - 1; i >= 0; i--) {
            const { start, end } = arrayPositions[i];
            const candidate = result.substring(start, end + 1);

            try {
                const parsed: unknown = JSON.parse(candidate);
                if (!Array.isArray(parsed) || parsed.length <= JSON_ARRAY_CONDENSE_THRESHOLD) {
                    continue;
                }

                // Check if array elements are objects with similar keys
                const objects = parsed.filter(
                    (item): item is Record<string, unknown> =>
                        typeof item === "object" && item !== null && !Array.isArray(item),
                );

                if (objects.length <= JSON_ARRAY_CONDENSE_THRESHOLD) continue;

                // Verify key similarity — at least 70% key overlap across elements
                const allKeys = objects.map((obj) => Object.keys(obj));
                const referenceKeys = new Set(allKeys[0]);
                const similarCount = allKeys.filter((keys) => {
                    const overlap = keys.filter((k) => referenceKeys.has(k)).length;
                    return overlap / Math.max(referenceKeys.size, keys.length) >= 0.7;
                }).length;

                if (similarCount < objects.length * 0.7) continue;

                // Build condensed summary
                const schema = this.#extractJsonSchema(objects[0]);
                const first = JSON.stringify(objects[0]);
                const last = JSON.stringify(objects[objects.length - 1]);
                const condensed = [
                    `[Array: ${parsed.length} items, schema: ${schema}]`,
                    `  first: ${first}`,
                    `  last: ${last}`,
                    `  ...${parsed.length - 2} similar items omitted`,
                ].join("\n");

                result = result.substring(0, start) + condensed + result.substring(end + 1);
            } catch {
                // Not valid JSON — skip
            }
        }

        return result;
    }

    /** Find positions of top-level JSON arrays using bracket counting */
    #findJsonArrayPositions(text: string): Array<{ start: number; end: number }> {
        const positions: Array<{ start: number; end: number }> = [];
        let i = 0;

        while (i < text.length) {
            if (text[i] === "[") {
                let depth = 1;
                let j = i + 1;
                let inString = false;
                let escapeNext = false;

                while (j < text.length && depth > 0) {
                    const ch = text[j];
                    if (escapeNext) {
                        escapeNext = false;
                        j++;
                        continue;
                    }
                    if (ch === "\\") {
                        escapeNext = true;
                        j++;
                        continue;
                    }
                    if (ch === '"') {
                        inString = !inString;
                    } else if (!inString) {
                        if (ch === "[") depth++;
                        else if (ch === "]") depth--;
                    }
                    j++;
                }

                if (depth === 0) {
                    // Verify this looks like JSON (starts with [{ or [" or [digit)
                    const after = text.substring(i + 1, i + 20).trimStart();
                    if (after.startsWith("{") || after.startsWith('"') || /^\d/.test(after)) {
                        positions.push({ start: i, end: j - 1 });
                    }
                }
                i = j;
            } else {
                i++;
            }
        }

        return positions;
    }

    /** Extract a human-readable schema from a JSON object */
    #extractJsonSchema(obj: Record<string, unknown>): string {
        const parts: string[] = [];
        for (const [key, value] of Object.entries(obj)) {
            if (value === null) parts.push(`${key}:null`);
            else if (Array.isArray(value)) parts.push(`${key}:array`);
            else parts.push(`${key}:${typeof value}`);
        }
        return `{${parts.join(", ")}}`;
    }

    /** Condense verbose XML blocks by stripping boilerplate attributes */
    #condenseXmlBlocks(text: string): string {
        // Match XML-like blocks (multi-line tags)
        // Strip commonly-boilerplate attributes: xmlns, xsi:*, schemaLocation, encoding
        const boilerplateAttrs =
            /\s+(?:xmlns(?::\w+)?|xsi:\w+|schemaLocation|encoding|standalone)\s*=\s*"[^"]*"/g;

        let result = text;

        // Only strip boilerplate attrs from lines that look like XML tags
        result = result.replace(/<[^>]{50,}>/g, (tag) => {
            const cleaned = tag.replace(boilerplateAttrs, "");
            return cleaned;
        });

        // Collapse self-closing tags with only whitespace content
        result = result.replace(/<(\w+)([^>]*)>\s*<\/\1>/g, "<$1$2 />");

        return result;
    }

    // ─── Stage 3: Sentence Deduplication ────────────────────────────

    /**
     * Sliding window Jaccard similarity on word sets.
     * Threshold ≥ 0.8 → merge duplicates, keeping the longer version.
     * Preserves lines containing errors, anomalies, or user-specific data.
     */
    #stageSentenceDedup(text: string): StageResult {
        const lines = text.split("\n");
        const kept: string[] = [];
        /** Track recent sentence word-sets for comparison */
        const recentWordSets: Array<{ words: Set<string>; index: number }> = [];
        let removedCount = 0;

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];
            const trimmed = line.trim();

            // Skip short lines, blank lines, headers, separators, code fences
            if (
                trimmed.length === 0 ||
                trimmed.startsWith("#") ||
                trimmed.startsWith("```") ||
                trimmed.startsWith("---") ||
                trimmed.startsWith("===")
            ) {
                kept.push(line);
                continue;
            }

            // Preserve lines with high-priority signals
            if (this.#isHighPriorityLine(trimmed)) {
                kept.push(line);
                recentWordSets.push({ words: this.#wordSet(trimmed), index: kept.length - 1 });
                continue;
            }

            const words = this.#wordSet(trimmed);
            if (words.size < MIN_SENTENCE_WORDS) {
                kept.push(line);
                continue;
            }

            // Check against recent sentences in sliding window
            let isDuplicate = false;
            for (const recent of recentWordSets) {
                const similarity = this.#jaccardSimilarity(words, recent.words);
                if (similarity >= DEDUP_SIMILARITY_THRESHOLD) {
                    // Keep the longer version — replace if current is longer
                    if (trimmed.length > (kept[recent.index]?.trim().length ?? 0)) {
                        kept[recent.index] = line;
                        recent.words = words;
                    }
                    isDuplicate = true;
                    removedCount++;
                    break;
                }
            }

            if (!isDuplicate) {
                kept.push(line);
                recentWordSets.push({ words, index: kept.length - 1 });

                // Sliding window eviction
                if (recentWordSets.length > DEDUP_WINDOW_SIZE) {
                    recentWordSets.shift();
                }
            }
        }

        const result = kept.join("\n");
        if (removedCount > 0) {
            logger.debug(`[TokenCompression/S3] Deduped ${removedCount} similar sentences.`);
        }

        return {
            text: result,
            applied: removedCount > 0,
            name: "sentence_dedup",
        };
    }

    /** Check if a line contains high-priority information that must be preserved */
    #isHighPriorityLine(line: string): boolean {
        const lower = line.toLowerCase();
        return (
            // Error signals
            lower.includes("error") ||
            lower.includes("exception") ||
            lower.includes("fail") ||
            lower.includes("crash") ||
            lower.includes("panic") ||
            lower.includes("fatal") ||
            // Anomalies
            lower.includes("warn") ||
            lower.includes("anomal") ||
            lower.includes("unexpected") ||
            lower.includes("critical") ||
            // User-specific data markers
            lower.includes("user:") ||
            lower.includes("@") ||
            lower.includes("password") ||
            lower.includes("token") ||
            lower.includes("api_key") ||
            lower.includes("secret")
        );
    }

    /** Extract normalized word set from text (lowercase, no punctuation) */
    #wordSet(text: string): Set<string> {
        return new Set(
            text
                .toLowerCase()
                .replace(/[^\p{L}\p{N}\s]/gu, " ")
                .split(/\s+/)
                .filter((w) => w.length > 1),
        );
    }

    /** Jaccard similarity: |A ∩ B| / |A ∪ B| */
    #jaccardSimilarity(a: Set<string>, b: Set<string>): number {
        if (a.size === 0 && b.size === 0) return 1;
        let intersection = 0;
        const smaller = a.size <= b.size ? a : b;
        const larger = a.size <= b.size ? b : a;
        for (const word of smaller) {
            if (larger.has(word)) intersection++;
        }
        const union = a.size + b.size - intersection;
        return union === 0 ? 0 : intersection / union;
    }

    // ─── Stage 4: Token Budget Enforcement ──────────────────────────

    /**
     * Head+tail truncation (smartTruncate pattern from DualChannelSegmenter).
     * Preserves the beginning (context) and end (most recent/conclusion).
     */
    #stageBudgetEnforce(text: string, tokenBudget: number): StageResult {
        const currentTokens = estimateTokens(text);
        if (currentTokens <= tokenBudget) {
            return { text, applied: false, name: "budget_enforce" };
        }

        // Estimate character budget from token budget (inverse of word×1.5)
        // avg word ≈ 5 chars + 1 space → ~6 chars/word → tokenBudget / 1.5 * 6
        const charBudget = Math.floor((tokenBudget / 1.5) * 6);
        const truncationMarker = "\n\n[...compressed: middle content omitted for token budget...]\n\n";
        const markerLen = truncationMarker.length;
        const available = charBudget - markerLen;

        if (available <= 0) {
            // Extreme case: budget too small — return just the head
            const result = text.substring(0, Math.max(charBudget, 100));
            return { text: result, applied: true, name: "budget_enforce" };
        }

        const headSize = Math.floor(available * 0.6); // 60% head (context)
        const tailSize = available - headSize;          // 40% tail (conclusion)

        const head = text.substring(0, headSize);
        const tail = text.substring(text.length - tailSize);

        const result = head + truncationMarker + tail;

        logger.debug(
            `[TokenCompression/S4] Budget enforcement: ${currentTokens} → ~${tokenBudget} tokens ` +
            `(head=${headSize}, tail=${tailSize} chars)`,
        );

        return { text: result, applied: true, name: "budget_enforce" };
    }

    // ─── Helpers ────────────────────────────────────────────────────

    #buildResult(
        originalText: string,
        compressedText: string,
        originalTokens: number,
        appliedStages: string[],
    ): CompressionResult {
        const compressedTokens = estimateTokens(compressedText);
        const compressionRatio =
            originalTokens === 0 ? 1 : compressedTokens / originalTokens;

        if (appliedStages.length > 0) {
            logger.info(
                `[TokenCompression] ${originalTokens} → ${compressedTokens} tokens ` +
                `(${(compressionRatio * 100).toFixed(1)}%), stages: ${appliedStages.join(", ")}`,
            );
        }

        return {
            originalTokens,
            compressedTokens,
            compressedText,
            compressionRatio,
            strategy: appliedStages.length > 0 ? appliedStages.join(", ") : "none",
        };
    }
}

// ─── Exported Utility ──────────────────────────────────────────────

/**
 * Token estimation consistent with MemoryManager.estimateTokens().
 * Uses word count × 1.5 heuristic.
 */
export function estimateTokens(text: string): number {
    if (!text) return 0;
    const words = text
        .trim()
        .split(/\s+/)
        .filter((w) => w.length > 0).length;
    return Math.ceil(words * 1.5);
}
