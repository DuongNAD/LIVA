/**
 * StructuredExtractor: Robust JSON Extraction & Validation Engine
 * ================================================================
 * Multi-layer extraction pipeline that replaces fragile brace-matching:
 * 
 * Layer 1: Strip <think> blocks and markdown noise
 * Layer 2: Extract from ```json fences (highest confidence)
 * Layer 3: Brace-matching fallback
 * Layer 4: jsonrepair as last resort
 * Layer 5: Zod schema validation (type-safe guarantee)
 */

import { z, ZodSchema, ZodError } from "zod";
import { jsonrepair } from "jsonrepair";

// =============================================================================
// Zod Schemas for Evolution System
// =============================================================================

/** Schema for a single mutation action */
const MutationSchema = z.object({
    type: z.enum(["modify", "create"]),
    filePath: z.string().min(1),
    className: z.string().optional(),
    methodName: z.string().optional(),
    code: z.string().min(1),
});

/** Schema for a population candidate */
const CandidateSchema = z.object({
    id: z.string().min(1),
    mutations: z.array(MutationSchema).min(1),
});

/** Schema for the full population response from the Darwinian Coder */
export const PopulationSchema = z.object({
    population: z.array(CandidateSchema).min(1).max(5),
});

/** Schema for QualityChecker response */
export const QualityAssessmentSchema = z.object({
    pass: z.boolean(),
    feedback: z.string().min(1),
});

export type PopulationPayload = z.infer<typeof PopulationSchema>;
export type QualityAssessmentPayload = z.infer<typeof QualityAssessmentSchema>;

// =============================================================================
// Multi-Layer JSON Extraction Engine
// =============================================================================

export interface ExtractionResult<T> {
    success: boolean;
    data: T | null;
    method: "json_fence" | "brace_match" | "jsonrepair" | "failed";
    errors: string[];
    rawText: string;
}

/**
 * Extract and validate structured JSON from raw LLM output.
 * 
 * @param rawText - Raw text from LLM (may contain <think> blocks, markdown, etc.)
 * @param schema - Zod schema for validation
 * @returns Validated data or detailed error report
 */
export function extractAndValidate<T>(
    rawText: string, 
    schema: ZodSchema<T>
): ExtractionResult<T> {
    const errors: string[] = [];

    // =================================================
    // Layer 0: Pre-processing — strip noise
    // =================================================
    let cleaned = rawText;

    // Remove <think>...</think> blocks (can contain { } that break extraction)
    cleaned = cleaned.replace(/<think>[\s\S]*?<\/think>/gi, "");

    // Remove leading/trailing whitespace
    cleaned = cleaned.trim();

    // =================================================
    // Layer 1: Extract from ```json ... ``` fences (highest confidence)
    // =================================================
    const jsonFenceMatch = cleaned.match(/```(?:json)?\s*\n([\s\S]*?)\n\s*```/);
    if (jsonFenceMatch) {
        const fenceContent = jsonFenceMatch[1].trim();
        const result = tryParseAndValidate(fenceContent, schema);
        if (result.success) {
            return { success: true, data: result.data, method: "json_fence", errors: [], rawText };
        }
        errors.push(`[Layer 1 - JSON fence] Parse/validate failed: ${result.error}`);
    }

    // =================================================
    // Layer 2: Brace-matching (find outermost { ... })
    // =================================================
    const braceJson = extractOutermostBraces(cleaned);
    if (braceJson) {
        const result = tryParseAndValidate(braceJson, schema);
        if (result.success) {
            return { success: true, data: result.data, method: "brace_match", errors: [], rawText };
        }
        errors.push(`[Layer 2 - Brace match] Parse/validate failed: ${result.error}`);

        // =================================================
        // Layer 3: jsonrepair as last resort
        // =================================================
        try {
            const repaired = jsonrepair(braceJson);
            const result2 = tryParseAndValidate(repaired, schema);
            if (result2.success) {
                console.warn("[StructuredExtractor] ⚠️ JSON was repaired — verify output quality");
                return { success: true, data: result2.data, method: "jsonrepair", errors: [], rawText };
            }
            errors.push(`[Layer 3 - jsonrepair] Validate failed after repair: ${result2.error}`);
        } catch (repairErr: any) {
            errors.push(`[Layer 3 - jsonrepair] Repair itself failed: ${repairErr.message}`);
        }
    } else {
        errors.push("[Layer 2 - Brace match] No valid brace pair found in text");
    }

    // All layers failed
    return { 
        success: false, 
        data: null, 
        method: "failed", 
        errors, 
        rawText: rawText.slice(0, 1000) 
    };
}

// =============================================================================
// Helper Functions
// =============================================================================

/**
 * Extract the outermost balanced { ... } from text.
 * More robust than indexOf/lastIndexOf — actually counts brace depth.
 */
function extractOutermostBraces(text: string): string | null {
    let depth = 0;
    let startIdx = -1;
    let inString = false;
    let escapeNext = false;

    for (let i = 0; i < text.length; i++) {
        const ch = text[i];

        if (escapeNext) {
            escapeNext = false;
            continue;
        }

        if (ch === "\\") {
            escapeNext = true;
            continue;
        }

        if (ch === '"') {
            inString = !inString;
            continue;
        }

        if (inString) continue;

        if (ch === "{") {
            if (depth === 0) startIdx = i;
            depth++;
        } else if (ch === "}") {
            depth--;
            if (depth === 0 && startIdx !== -1) {
                return text.substring(startIdx, i + 1);
            }
        }
    }

    return null;
}

/**
 * Try to parse JSON string and validate against Zod schema.
 */
function tryParseAndValidate<T>(
    jsonStr: string, 
    schema: ZodSchema<T>
): { success: true; data: T } | { success: false; error: string } {
    let parsed: any;
    try {
        parsed = JSON.parse(jsonStr);
    } catch (e: any) {
        return { success: false, error: `JSON.parse failed: ${e.message}` };
    }

    try {
        const validated = schema.parse(parsed);
        return { success: true, data: validated };
    } catch (e) {
        if (e instanceof ZodError) {
            const issues = e.issues.map(i => `  ${i.path.join(".")}: ${i.message}`).join("\n");
            return { success: false, error: `Zod validation failed:\n${issues}` };
        }
        return { success: false, error: `Validation error: ${e}` };
    }
}
