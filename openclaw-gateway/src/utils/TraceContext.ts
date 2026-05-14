import { AsyncLocalStorage } from "node:async_hooks";
import { randomUUID } from "node:crypto";

/**
 * TraceContext — Request-scoped trace IDs via AsyncLocalStorage.
 *
 * Provides automatic trace-ID propagation through async call chains
 * without manually threading `traceId` parameters through every function.
 *
 * Usage:
 *   1. Wrap the entry point of each request/turn in `TraceContext.run()`
 *   2. Call `TraceContext.getTraceId()` anywhere in the async chain
 *   3. The pino mixin automatically injects `traceId` into every log line
 *
 * Example:
 * ```typescript
 * TraceContext.run(() => {
 *     logger.info("This log line will have a traceId field");
 *     await someDeepFunction(); // traceId propagates automatically
 * });
 * ```
 */

interface TraceStore {
    traceId: string;
    /** Optional user/session context */
    userId?: string;
    channel?: string;
}

const storage = new AsyncLocalStorage<TraceStore>();

export const TraceContext = {
    /**
     * Run a function within a new trace context.
     * All async operations within `fn` will have access to the trace ID.
     */
    run<T>(fn: () => T, overrideTraceId?: string): T {
        const store: TraceStore = {
            traceId: overrideTraceId || randomUUID().substring(0, 8),
        };
        return storage.run(store, fn);
    },

    /**
     * Run with additional context (userId, channel).
     */
    runWithContext<T>(fn: () => T, context: Partial<TraceStore>): T {
        const store: TraceStore = {
            traceId: context.traceId || randomUUID().substring(0, 8),
            userId: context.userId,
            channel: context.channel,
        };
        return storage.run(store, fn);
    },

    /**
     * Get the current trace ID, or "no-trace" if outside a trace context.
     */
    getTraceId(): string {
        return storage.getStore()?.traceId || "no-trace";
    },

    /**
     * Get the full trace store (for advanced use).
     */
    getStore(): TraceStore | undefined {
        return storage.getStore();
    },

    /**
     * Pino mixin function — automatically injects trace fields into every log line.
     * Use as: `pino({ mixin: TraceContext.pinoMixin })`
     */
    pinoMixin(): Record<string, string> {
        const store = storage.getStore();
        if (!store) return {};
        const result: Record<string, string> = { traceId: store.traceId };
        if (store.userId) result.userId = store.userId;
        if (store.channel) result.channel = store.channel;
        return result;
    },
};
