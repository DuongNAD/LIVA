import { jsonrepair } from "jsonrepair";

/**
 * JsonExtractor — Safe JSON Extraction from LLM Output
 * =====================================================
 * Centralized utility for extracting JSON from potentially messy
 * LLM responses (markdown-wrapped, hallucinated prefixes, etc.).
 *
 * Uses indexOf/lastIndexOf + jsonrepair per AI_CONTEXT.md §4.6.
 * Previously duplicated in: ReflectionDaemon, ConsolidationCron,
 * PersonalKnowledgeExtractor.
 *
 * @module JsonExtractor
 */

/**
 * Safely extract a JSON object from a text string.
 * Handles:
 *   - Markdown code block wrapping (```json ... ```)
 *   - LLM explanatory text before/after JSON
 *   - Minor JSON syntax errors (via jsonrepair)
 *
 * @param text  Raw LLM output text
 * @returns     Parsed object, or null if extraction/parsing fails
 */
export function safeExtractJSON<T>(text: string): T | null {
    const first = text.indexOf("{");
    const last = text.lastIndexOf("}");
    if (first === -1 || last === -1 || last < first) return null;
    try {
        const raw = text.substring(first, last + 1);
        return JSON.parse(jsonrepair(raw));
    } catch {
        return null;
    }
}
