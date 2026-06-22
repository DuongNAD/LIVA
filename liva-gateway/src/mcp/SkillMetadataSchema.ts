import { z } from "zod";
import { sanitizeMetadata } from "../utils/ZodHelper";

/**
 * SkillMetadataSchema — Zod validation gate for skill registration.
 *
 * Ensures every skill loaded into LocalMCPServer/SkillRegistry has:
 *   - A valid name (alphanumeric + underscores)
 *   - A non-empty description
 *   - Proper parameter schema structure
 *
 * Rejects malformed skills at load time instead of failing at runtime.
 */

export const SkillMetadataSchema = z.preprocess(
    (val) => sanitizeMetadata(val),
    z.object({
        name: z.string()
            .min(1, "Skill name must not be empty")
            .regex(/^[a-z][a-z0-9_]*$/, "Skill name must be lowercase alphanumeric with underscores"),
        description: z.string().min(5, "Description must be at least 5 characters"),
        // Parameters schema varies widely across skills — validate as opaque object
        parameters: z.any().optional(),
        search_keywords: z.array(z.string()).optional(),
        isCoreSkill: z.boolean().optional(),
        category: z.string().optional(),
        semantic_tags: z.array(z.string()).optional(),
        requires_hitl: z.boolean().optional(),
        is_cpu_heavy: z.boolean().optional(),
    }).passthrough()
).refine((data) => {
    try {
        const str = JSON.stringify(data);
        return Buffer.byteLength(str, 'utf-8') <= 50 * 1024;
    } catch {
        return false;
    }
}, { message: "Metadata payload size exceeds 50KB limit" });

export type ValidatedSkillMetadata = z.infer<typeof SkillMetadataSchema>;

export const AgentStateSchema = z.preprocess(
    (val) => sanitizeMetadata(val),
    z.object({
        sessionId: z.string().uuid(),
        agentId: z.string(),
        status: z.enum(['IDLE', 'THINKING', 'ACTING', 'ERROR']),
        context: z.record(z.string(), z.any()).optional().default({}),
        confidence: z.number().min(0).max(1).optional(),
        lastActionTimestamp: z.number().optional()
    }).passthrough()
).refine((data) => {
    try {
        const str = JSON.stringify(data);
        return Buffer.byteLength(str, 'utf-8') <= 50 * 1024;
    } catch {
        return false;
    }
}, { message: "Metadata payload size exceeds 50KB limit" });

export type AgentState = z.infer<typeof AgentStateSchema>;

/**
 * Validate a skill module's metadata before registration.
 * Returns the validated metadata or null (with logged reason).
 */
export function validateSkillMetadata(
    rawMetadata: unknown,
    _filePath: string
): ValidatedSkillMetadata | null {
    const sanitized = sanitizeMetadata(rawMetadata);
    const result = SkillMetadataSchema.safeParse(sanitized);
    if (!result.success) {
        // Return null — caller decides whether to log or skip
        return null;
    }
    return result.data;
}

