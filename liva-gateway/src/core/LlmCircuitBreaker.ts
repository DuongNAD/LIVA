import { logger } from "../utils/logger";
import LRUCache from "lru-cache";

export type LlmCircuitState = "CLOSED" | "OPEN" | "HALF_OPEN";

interface LlmCircuitEntry {
    state: LlmCircuitState;
    consecutiveFailures: number;
    lastFailureTime: number;
    lastError: string;
}

const FAILURE_THRESHOLD = 3;
const HALF_OPEN_COOLDOWN_MS = 30 * 1000; // 30 seconds for LLM cooldown before retrying

export class LlmCircuitBreaker {
    private static instance: LlmCircuitBreaker;

    // [v27 FIX] Replace unbounded Map with LRUCache (AI_CONTEXT §6)
    // Prevents memory leak when dynamic model targets accumulate without eviction.
    private circuits = new LRUCache<string, LlmCircuitEntry>({
        max: 50,
        ttl: 5 * 60 * 1000, // Auto-evict after 5 minutes
    });

    private constructor() {}

    public static getInstance(): LlmCircuitBreaker {
        if (!LlmCircuitBreaker.instance) {
            LlmCircuitBreaker.instance = new LlmCircuitBreaker();
        }
        return LlmCircuitBreaker.instance;
    }

    public recordSuccess(target: string): void {
        const entry = this.circuits.get(target);
        if (!entry) return;

        if (entry.state === "HALF_OPEN") {
            logger.info(`[LlmCircuitBreaker] ✅ ${target}: Half-open probe succeeded → CLOSED`);
        }
        this.circuits.delete(target);
    }

    public recordFailure(target: string, errorMsg: string): void {
        const entry = this.circuits.get(target) ?? {
            state: "CLOSED" as LlmCircuitState,
            consecutiveFailures: 0,
            lastFailureTime: 0,
            lastError: "",
        };

        entry.consecutiveFailures++;
        entry.lastFailureTime = Date.now();
        entry.lastError = errorMsg;

        if (entry.consecutiveFailures >= FAILURE_THRESHOLD) {
            if (entry.state !== "OPEN") {
                logger.warn(`[LlmCircuitBreaker] 🔌 ${target}: OPEN — ${entry.consecutiveFailures} consecutive failures. Last error: ${errorMsg}`);
            }
            entry.state = "OPEN";
        }

        this.circuits.set(target, entry);
    }

    public canExecute(target: string): boolean {
        const entry = this.circuits.get(target);
        if (!entry) return true;

        if (entry.state === "CLOSED") return true;
        if (entry.state === "HALF_OPEN") return true;

        if (entry.state === "OPEN") {
            const elapsed = Date.now() - entry.lastFailureTime;
            if (elapsed >= HALF_OPEN_COOLDOWN_MS) {
                entry.state = "HALF_OPEN";
                logger.info(`[LlmCircuitBreaker] 🔄 ${target}: Cooldown elapsed → HALF_OPEN (probe allowed)`);
                return true;
            }
            return false;
        }

        return true;
    }
}

