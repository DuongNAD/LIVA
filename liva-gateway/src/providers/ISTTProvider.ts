import type { EventEmitter } from "node:events";

/**
 * ISTTProvider — Contract for speech recognition providers.
 * Extends EventEmitter to handle transcription and fallback events.
 * 
 * Events to emit:
 * - "transcription_partial" (text: string) → Streaming partial text in real-time.
 * - "transcription_ready" (text: string) → Final complete transcription.
 * - "stt_fallback_activated" → Emitted when the circuit breaker opens.
 * - "stt_fallback_deactivated" → Emitted when the circuit breaker resets.
 */
export interface ISTTProvider extends EventEmitter {
    /**
     * Initialize the STT provider, loading any models or worker threads.
     */
    initialize(): Promise<void>;

    /**
     * Push audio chunk with a silence/VAD timeout mechanism.
     */
    pushAudioChunk(chunk: Float32Array): void;

    /**
     * Push audio chunk only, without silence timeout (delegating timing to VAD).
     */
    pushAudioChunkOnly(chunk: Float32Array): void;

    /**
     * Force final transcription of currently buffered audio.
     */
    triggerTranscription(): void;

    /**
     * Discard all currently buffered audio chunks.
     */
    flush(): void;

    /**
     * Clean up and release worker threads/resources.
     */
    destroy(): void;

    /**
     * Check if the circuit breaker is open (disabled).
     */
    isCircuitOpen(): boolean;
}
