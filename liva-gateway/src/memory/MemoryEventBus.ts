import { globalEventBus } from '../core/events/TypedEventBus';

/**
 * MemoryEventBus — Internal Node.js EventEmitter for decoupled memory signaling.
 * 
 * Prevents circular dependency between ReflectionDaemon and ConsolidationCron.
 * Signals are fire-and-forget (async, non-blocking).
 * 
 * Events:
 *   - TOPIC_SHIFT:       DualChannelSegmenter detected a topic boundary
 *   - NEW_TURN:          ReflectionDaemon processed a new conversation turn
 *   - DREAMING_COMPLETE: Fired when dreaming sequence completes with DreamingResult payload
 *   - DREAMING_APPROVED: Fired when dreaming proposed index is committed/approved
 * 
 * @module MemoryEventBus
 */
export const memoryEvents = {
    on: (topic: string, callback: any) => globalEventBus.on(topic as any, callback),
    removeListener: (topic: string, callback: any) => globalEventBus.off(topic as any, callback),
    emit: (topic: string, payload?: unknown) => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any -- bridge layer: TypedEventBus<any> conditional args
        return (globalEventBus as any).emit(topic, payload);
    }
};

// Prevent unhandled error from crashing the process
memoryEvents.on('error', () => { /* swallow */ });
