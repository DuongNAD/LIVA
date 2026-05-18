/**
 * SkillCircuitBreaker — v25 Passive Circuit Breaker for Skills
 * =============================================================
 * Wraps SkillRegistry.executeSkill() to track consecutive failures
 * per skill. After 3 consecutive errors, the circuit OPENS and the
 * skill is pruned from PromptBuilder's <tools> XML.
 *
 * Architecture Decision (v25):
 *   - PASSIVE: Only counts errors when user actually triggers a skill.
 *   - NO active probing — zero API quota waste.
 *   - Context Pruning: PromptBuilder reads `getOpenCircuits()` to
 *     exclude dead skills from LLM prompt → prevents hallucinated calls.
 *   - Half-Open: After cooldown (5 min), allows 1 probe call. If it
 *     succeeds → circuit CLOSES. If it fails → circuit stays OPEN.
 *
 * @module SkillCircuitBreaker
 */

import { logger } from "../utils/logger";

export type CircuitState = "CLOSED" | "OPEN" | "HALF_OPEN";

interface CircuitEntry {
    state: CircuitState;
    consecutiveFailures: number;
    lastFailureTime: number;
    lastError: string;
}

/** Number of consecutive failures before circuit opens */
const FAILURE_THRESHOLD = 3;

/** Cooldown before allowing a half-open probe (5 minutes) */
const HALF_OPEN_COOLDOWN_MS = 5 * 60 * 1000;

export class SkillCircuitBreaker {
    #circuits: Map<string, CircuitEntry> = new Map();

    /**
     * Record a successful skill execution → reset circuit to CLOSED.
     */
    recordSuccess(skillName: string): void {
        const entry = this.#circuits.get(skillName);
        if (!entry) return; // No circuit entry = never failed = CLOSED

        if (entry.state === "HALF_OPEN") {
            logger.info(`[CircuitBreaker] ✅ ${skillName}: Half-open probe succeeded → CLOSED`);
        }
        // Reset to pristine state
        this.#circuits.delete(skillName);
    }

    /**
     * Record a failed skill execution → increment failure count.
     * After FAILURE_THRESHOLD consecutive failures → OPEN circuit.
     */
    recordFailure(skillName: string, errorMsg: string): void {
        const entry = this.#circuits.get(skillName) ?? {
            state: "CLOSED" as CircuitState,
            consecutiveFailures: 0,
            lastFailureTime: 0,
            lastError: "",
        };

        entry.consecutiveFailures++;
        entry.lastFailureTime = Date.now();
        entry.lastError = errorMsg;

        if (entry.consecutiveFailures >= FAILURE_THRESHOLD) {
            if (entry.state !== "OPEN") {
                logger.warn(`[CircuitBreaker] 🔌 ${skillName}: OPEN — ${entry.consecutiveFailures} consecutive failures. Last error: ${errorMsg}`);
            }
            entry.state = "OPEN";
        }

        this.#circuits.set(skillName, entry);
    }

    /**
     * Check if a skill should be allowed to execute.
     * - CLOSED: always allow
     * - OPEN: block (unless cooldown elapsed → transition to HALF_OPEN, allow 1 probe)
     * - HALF_OPEN: allow (1 probe call to test recovery)
     *
     * @returns true if execution is allowed, false if blocked
     */
    canExecute(skillName: string): boolean {
        const entry = this.#circuits.get(skillName);
        if (!entry) return true; // Never failed = CLOSED

        if (entry.state === "CLOSED") return true;
        if (entry.state === "HALF_OPEN") return true;

        // OPEN state: check if cooldown has elapsed
        if (entry.state === "OPEN") {
            const elapsed = Date.now() - entry.lastFailureTime;
            if (elapsed >= HALF_OPEN_COOLDOWN_MS) {
                entry.state = "HALF_OPEN";
                logger.info(`[CircuitBreaker] 🔄 ${skillName}: Cooldown elapsed → HALF_OPEN (probe allowed)`);
                return true;
            }
            return false; // Still in cooldown
        }

        return true;
    }

    /**
     * Get set of skill names with OPEN circuits.
     * Used by PromptBuilder to exclude dead skills from <tools> XML.
     */
    getOpenCircuits(): Set<string> {
        const open = new Set<string>();
        for (const [name, entry] of this.#circuits) {
            if (entry.state === "OPEN") {
                // Check cooldown — transition to HALF_OPEN if elapsed
                const elapsed = Date.now() - entry.lastFailureTime;
                if (elapsed < HALF_OPEN_COOLDOWN_MS) {
                    open.add(name);
                }
            }
        }
        return open;
    }

    /**
     * Get human-readable status for all tracked circuits (for dashboard/telemetry).
     */
    getStatus(): Array<{ name: string; state: CircuitState; failures: number; lastError: string }> {
        const result: Array<{ name: string; state: CircuitState; failures: number; lastError: string }> = [];
        for (const [name, entry] of this.#circuits) {
            result.push({
                name,
                state: entry.state,
                failures: entry.consecutiveFailures,
                lastError: entry.lastError,
            });
        }
        return result;
    }

    /**
     * Get the error message for an open circuit (for LLM context injection).
     */
    getCircuitError(skillName: string): string | null {
        const entry = this.#circuits.get(skillName);
        if (!entry || entry.state === "CLOSED") return null;
        return entry.lastError;
    }
}
