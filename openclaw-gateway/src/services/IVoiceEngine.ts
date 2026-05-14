import type { EventEmitter } from "node:events";

/**
 * IVoiceEngine — Contract for TTS engines (VoiceEngine, KokoroVoiceEngine).
 * 
 * Extends EventEmitter to support typed event subscription (e.g., "audio_base64").
 * Import as TYPE to avoid bundle weight (pure interface, no runtime cost).
 */
export interface IVoiceEngine extends EventEmitter {
    speak(text: string): Promise<boolean>;
    pushTokens(token: string): void;
    flushTTS(): void;
    preempt(): void;
    destroy(): Promise<void>;
}
