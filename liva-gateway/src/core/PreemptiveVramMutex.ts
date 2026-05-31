/**
 * PreemptiveVramMutex — Priority-Based VRAM Lock (LIVA v25 Pillar 2)
 * ====================================================================
 * Replaces naive FIFO locks for VRAM access. Background tasks (Consolidation,
 * Shadow Digest) acquire low-priority locks. AgentLoop (user voice/chat)
 * acquires highest-priority lock and can INSTANTLY abort any lower-priority
 * holder via AbortController.
 *
 * Architecture Decision (v25):
 *   - NOT a traditional mutex (no queue). Only 1 holder at a time.
 *   - Higher priority PREEMPTS lower priority immediately (0ms latency).
 *   - Each lock holder receives an AbortSignal. When preempted, the signal
 *     fires → holder must clean up and release.
 *   - Priority 0 = highest (AgentLoop), Priority 10 = lowest (background).
 *
 * @module PreemptiveVramMutex
 */

import { logger } from "../utils/logger";

export interface VramLockHandle {
    /** Unique identifier for this lock */
    readonly id: string;
    /** Priority level (0 = highest) */
    readonly priority: number;
    /** Signal that fires when this lock is preempted by higher priority */
    readonly signal: AbortSignal;
    /** Release the lock manually when done */
    release: () => void;
}

interface ActiveLock {
    id: string;
    priority: number;
    controller: AbortController;
    acquiredAt: number;
}

/** Well-known priority levels */
export const VRAM_PRIORITY = {
    /** User interaction (voice, chat) — cannot be preempted */
    USER_INTERACTIVE: 0,
    /** System-critical tasks (anomaly recovery) */
    SYSTEM_CRITICAL: 1,
    /** Background intelligence (consolidation, reflection) */
    BACKGROUND_INTEL: 5,
    /** Proactive features (shadow digest, news synthesis) */
    PROACTIVE: 8,
    /** Lowest priority (telemetry, metrics) */
    TELEMETRY: 10,
} as const;

export class PreemptiveVramMutex {
    #activeLock: ActiveLock | null = null;

    /**
     * Attempt to acquire the VRAM lock.
     *
     * - If no lock is held → grant immediately.
     * - If current holder has LOWER priority (higher number) → PREEMPT it
     *   (abort its signal) and grant to new requester.
     * - If current holder has EQUAL or HIGHER priority → REJECT (return null).
     *
     * @param id       Human-readable identifier (e.g., "AgentLoop", "ConsolidationCron")
     * @param priority Priority level (0 = highest)
     * @returns        VramLockHandle if acquired, null if rejected
     */
    acquire(id: string, priority: number): VramLockHandle | null {
        // No active lock → grant immediately
        if (!this.#activeLock) {
            return this.#grant(id, priority);
        }

        // Same or higher priority already holds → reject
        if (this.#activeLock.priority <= priority) {
            logger.debug(`[VramMutex] ❌ Rejected "${id}" (p=${priority}) — "${this.#activeLock.id}" (p=${this.#activeLock.priority}) holds the lock.`);
            return null;
        }

        // Lower priority holds → PREEMPT
        const victim = this.#activeLock;
        logger.warn(`[VramMutex] ⚡ PREEMPTING "${victim.id}" (p=${victim.priority}) for "${id}" (p=${priority}) — instant VRAM reclaim.`);
        victim.controller.abort(`Preempted by higher-priority task: ${id}`);
        this.#activeLock = null;

        return this.#grant(id, priority);
    }

    /**
     * Check if the mutex is currently held.
     */
    isLocked(): boolean {
        return this.#activeLock !== null;
    }

    /**
     * Get info about the current lock holder (for telemetry).
     */
    getCurrentHolder(): { id: string; priority: number; heldMs: number } | null {
        if (!this.#activeLock) return null;
        return {
            id: this.#activeLock.id,
            priority: this.#activeLock.priority,
            heldMs: Date.now() - this.#activeLock.acquiredAt,
        };
    }

    #grant(id: string, priority: number): VramLockHandle {
        const controller = new AbortController();
        this.#activeLock = { id, priority, controller, acquiredAt: Date.now() };

        logger.info(`[VramMutex] 🔒 Granted lock to "${id}" (priority=${priority})`);

        const handle: VramLockHandle = {
            id,
            priority,
            signal: controller.signal,
            release: () => {
                if (this.#activeLock?.id === id) {
                    const heldMs = Date.now() - this.#activeLock.acquiredAt;
                    this.#activeLock = null;
                    logger.info(`[VramMutex] 🔓 Released lock from "${id}" (held ${heldMs}ms)`);
                }
            },
        };

        return handle;
    }
}
