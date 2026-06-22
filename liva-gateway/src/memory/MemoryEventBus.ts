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
    on: <T>(topic: string, callback: (payload: T) => void) => globalEventBus.on(topic, callback as (payload: unknown) => void),
    removeListener: <T>(topic: string, callback: (payload: T) => void) => globalEventBus.off(topic, callback as (payload: unknown) => void),
    emit: (topic: string, payload?: unknown) => {
        return globalEventBus.emit(topic, payload);
    }
};

// Prevent unhandled error from crashing the process
memoryEvents.on('error', () => { /* swallow */ });
