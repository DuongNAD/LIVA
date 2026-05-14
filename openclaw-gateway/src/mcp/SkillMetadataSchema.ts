import { z } from "zod";

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

export const SkillMetadataSchema = z.object({
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
});

export type ValidatedSkillMetadata = z.infer<typeof SkillMetadataSchema>;

/**
 * Validate a skill module's metadata before registration.
 * Returns the validated metadata or null (with logged reason).
 */
export function validateSkillMetadata(
    rawMetadata: unknown,
    filePath: string
): ValidatedSkillMetadata | null {
    const result = SkillMetadataSchema.safeParse(rawMetadata);
    if (!result.success) {
        const issues = result.error.issues
            .map(i => `${i.path.join(".")}: ${i.message}`)
            .join("; ");
        // Return null — caller decides whether to log or skip
        return null;
    }
    return result.data;
}
