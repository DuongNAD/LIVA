import { EventEmitter } from 'node:events';

/**
 * MemoryEventBus — Internal Node.js EventEmitter for decoupled memory signaling.
 * 
 * Prevents circular dependency between ReflectionDaemon and ConsolidationCron.
 * Signals are fire-and-forget (async, non-blocking).
 * 
 * Events:
 *   - TOPIC_SHIFT: DualChannelSegmenter detected a topic boundary
 *   - NEW_TURN:    ReflectionDaemon processed a new conversation turn
 * 
 * @module MemoryEventBus
 */
export const memoryEvents = new EventEmitter();

// Prevent unhandled error from crashing the process
memoryEvents.on('error', () => { /* swallow */ });
