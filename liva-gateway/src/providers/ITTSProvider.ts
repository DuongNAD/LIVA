import type { EventEmitter } from "node:events";

/**
 * ITTSProvider — Contract for text-to-speech providers.
 * Extends EventEmitter to support typed audio events.
 * 
 * Events to emit:
 * - "audio_base64" (base64: string) → Emits audio chunk as a base64 string.
 * - "audio_buffer" (buffer: Buffer) → Emits raw audio buffer.
 */
export interface ITTSProvider extends EventEmitter {
    /**
     * Speak a text directly (one-shot synthesis).
     */
    speak(text: string): Promise<boolean>;

    /**
     * Feed token stream into TTS engine, chunking/formatting before synthesis.
     */
    pushTokens(token: string): void;

    /**
     * Flush remaining formatted text and synthesize.
     */
    flushTTS(): void;

    /**
     * Preempt / interrupt current speech synthesis and clear buffers.
     */
    preempt(): void;

    /**
     * Terminate the engine and release resources.
     */
    destroy(): Promise<void>;
}
