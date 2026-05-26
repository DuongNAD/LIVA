import { z } from "zod";
import { logger } from "@utils/logger";
import type { StructuredMemory } from "../../memory/StructuredMemory";

/**
 * [UHM-v3] AgeMem — Agentic Memory CRUD Skill
 * 
 * Allows the LLM Agent to ACTIVELY manage long-term memory (L1 KV Facts).
 * Unlike passive extraction (ReflectionDaemon), this gives the agent
 * direct control over add/update/delete/search operations.
 * 
 * 🚨 GUARDRAILS (Architect-mandated):
 * - Namespace Isolation: Only whitelisted categories are accessible.
 * - HITL Guard: `delete` action requires human approval (requires_hitl flag).
 * - Rate Limit: Max 5 mutations per tool invocation.
 * - Ebbinghaus Sync: `update` resets memory_strength to 1.0.
 * - Audit Trail: All mutations logged with source='agent_explicit'.
 */

/** Whitelisted categories — agent CANNOT create or access categories outside this list */
const ALLOWED_CATEGORIES = [
    "user_preferences",
    "relationships",
    "facts",
    "work_context",
    "personal_info",
] as const;

const ManageMemorySchema = z.object({
    action: z.enum(["add", "update", "delete", "search"]).describe(
        "Action to perform: add (new fact), update (modify existing), delete (remove), search (find matching facts)"
    ),
    key: z.string().min(1).max(100).describe("Unique key for the fact (e.g., 'favorite_language')"),
    value: z.string().max(500).optional().describe("Value for the fact (required for add/update)"),
    category: z.enum(ALLOWED_CATEGORIES).default("facts").describe("Category namespace"),
});

export const metadata = {
    name: "manage_memory",
    description: "[HITL_DELETE] Actively manage LIVA's long-term memory. Add, update, delete, or search facts about the user. Use when user explicitly tells you to remember, forget, or change something.",
    short_desc: "CRUD user memory facts",
    category: "personal" as const,
    semantic_tags: ["memory", "remember", "forget", "nhớ", "quên", "ghi nhớ", "xóa", "cập nhật"],
    search_keywords: ["nhớ", "quên", "remember", "forget", "ghi nhớ", "memory"],
    kit: "PERSONAL_KIT" as const,
    requires_hitl: true,  // delete action needs human approval
    parameters: {
        type: "object",
        properties: {
            action: { type: "string", enum: ["add", "update", "delete", "search"] },
            key: { type: "string" },
            value: { type: "string" },
            category: { type: "string", enum: [...ALLOWED_CATEGORIES] },
        },
        required: ["action", "key"],
    },
};

/** Max mutations per single tool invocation — prevents LLM spam */
const MAX_MUTATIONS_PER_CALL = 5;
let mutationCount = 0;

/** Lazy-injected StructuredMemory reference — set by CoreKernel after boot */
let memoryRef: StructuredMemory | null = null;

/**
 * Inject StructuredMemory reference. Called once during CoreKernel bootstrap.
 * This avoids circular dependencies and follows the DI pattern.
 */
export function setMemoryRef(mem: StructuredMemory): void {
    memoryRef = mem;
}

export const execute = async (argsObj: unknown): Promise<string> => {
    if (!memoryRef) {
        return "[MEMORY ERROR] Memory system not initialized. Try again later.";
    }

    try {
        const parsed = ManageMemorySchema.parse(argsObj);
        const { action, key, value, category } = parsed;
        const fullKey = `${category}:${key}`;

        switch (action) {
            case "search": {
                // Search is read-only — no rate limiting needed
                const allFacts = memoryRef.getAllFacts();
                const matches = allFacts.filter(f =>
                    f.key.includes(key) || f.value?.toLowerCase().includes(key.toLowerCase())
                );
                if (matches.length === 0) {
                    return `[MEMORY SEARCH] No facts found matching "${key}".`;
                }
                const formatted = matches.slice(0, 10).map(f =>
                    `• ${f.key} = ${f.value} (category: ${f.category || 'unknown'}, strength: ${f.memoryStrength?.toFixed(2) || '1.00'})`
                ).join("\n");
                return `[MEMORY SEARCH] Found ${matches.length} matching fact(s):\n${formatted}`;
            }

            case "add": {
                if (!value) return "[MEMORY ERROR] 'value' is required for add action.";
                if (++mutationCount > MAX_MUTATIONS_PER_CALL) {
                    return `[MEMORY RATE LIMIT] Maximum ${MAX_MUTATIONS_PER_CALL} mutations per turn reached.`;
                }

                // Check if key already exists
                const existing = memoryRef.getFact(fullKey);
                if (existing) {
                    return `[MEMORY CONFLICT] Fact "${fullKey}" already exists with value: "${existing}". Use 'update' action instead.`;
                }

                await memoryRef.setFact(fullKey, value, {
                    source: "agent_explicit",
                    category,
                });
                logger.info(`[AgeMem] ADD: ${fullKey} = "${value}" (source: agent_explicit)`);
                return `[MEMORY ADDED] Successfully stored: ${fullKey} = "${value}"`;
            }

            case "update": {
                if (!value) return "[MEMORY ERROR] 'value' is required for update action.";
                if (++mutationCount > MAX_MUTATIONS_PER_CALL) {
                    return `[MEMORY RATE LIMIT] Maximum ${MAX_MUTATIONS_PER_CALL} mutations per turn reached.`;
                }

                // [Ebbinghaus Sync] setFact with upsert automatically resets strength via MAX(old, 0.8)
                await memoryRef.setFact(fullKey, value, {
                    source: "agent_explicit",
                    category,
                });
                // Force full strength reset for explicit agent updates
                memoryRef.touchFact(fullKey);

                logger.info(`[AgeMem] UPDATE: ${fullKey} = "${value}" (strength reset to 1.0)`);
                return `[MEMORY UPDATED] ${fullKey} = "${value}" (memory strength reset)`;
            }

            case "delete": {
                if (++mutationCount > MAX_MUTATIONS_PER_CALL) {
                    return `[MEMORY RATE LIMIT] Maximum ${MAX_MUTATIONS_PER_CALL} mutations per turn reached.`;
                }

                const existingVal = memoryRef.getFact(fullKey);
                if (!existingVal) {
                    return `[MEMORY NOT FOUND] Fact "${fullKey}" does not exist.`;
                }

                await memoryRef.deleteFact(fullKey);
                logger.info(`[AgeMem] DELETE: ${fullKey} (was: "${existingVal}", source: agent_explicit)`);
                return `[MEMORY DELETED] Removed fact: ${fullKey}`;
            }

            default:
                return `[MEMORY ERROR] Unknown action: ${action}`;
        }
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[AgeMem] Error: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[MEMORY ERROR] Invalid parameters: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[MEMORY ERROR] ${errMsg}`;
    }
};

/**
 * Reset mutation counter — called at the start of each turn.
 */
export function resetMutationCounter(): void {
    mutationCount = 0;
}
