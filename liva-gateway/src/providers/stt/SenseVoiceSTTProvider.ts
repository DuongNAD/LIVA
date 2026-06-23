import { EventEmitter } from "node:events";
import { ISTTProvider } from "../ISTTProvider";

/**
 * SenseVoiceSTTProvider — Implements ISTTProvider as a simulated/wrapper STT.
 * Maintains real state and models behavior dynamically based on PCM Float32 audio chunk lengths.
 */
export class SenseVoiceSTTProvider extends EventEmitter implements ISTTProvider {
    private isReady = false;
    private circuitOpen = false;
    private audioChunks: Float32Array[] = [];
    private silenceTimer: NodeJS.Timeout | null = null;
    private isStreaming = false;

    constructor() {
        super();
    }

    public async initialize(): Promise<void> {
        this.isReady = true;
    }

    public pushAudioChunk(chunk: Float32Array): void {
        if (this.circuitOpen) return;
        this.pushAudioChunkOnly(chunk);

        if (this.silenceTimer) {
            clearTimeout(this.silenceTimer);
        }
        this.silenceTimer = setTimeout(() => {
            this.triggerTranscription();
        }, 800);
    }

    public pushAudioChunkOnly(chunk: Float32Array): void {
        if (this.circuitOpen) return;
        this.audioChunks.push(chunk);
        this.isStreaming = true;

        const totalSamples = this.audioChunks.reduce((acc, c) => acc + c.length, 0);
        // Simulate a partial transcription update every ~1 second of accumulated audio
        if (totalSamples > 0 && totalSamples % 16000 === 0) {
            const simulatedPartial = `[SenseVoice Partial: ${(totalSamples / 16000).toFixed(1)}s]`;
            this.emit("transcription_partial", simulatedPartial);
        }
    }

    public triggerTranscription(): void {
        if (this.silenceTimer) {
            clearTimeout(this.silenceTimer);
            this.silenceTimer = null;
        }

        if (!this.isStreaming || this.audioChunks.length === 0) {
            return;
        }

        const totalSamples = this.audioChunks.reduce((acc, c) => acc + c.length, 0);
        this.audioChunks = [];
        this.isStreaming = false;

        const durationSec = totalSamples / 16000;
        const text = `Simulated transcription for ${durationSec.toFixed(2)} seconds of audio.`;
        this.emit("transcription_ready", text);
    }

    public flush(): void {
        this.audioChunks = [];
        this.isStreaming = false;
        if (this.silenceTimer) {
            clearTimeout(this.silenceTimer);
            this.silenceTimer = null;
        }
    }

    public destroy(): void {
        this.flush();
        this.removeAllListeners();
    }

    public isCircuitOpen(): boolean {
        return this.circuitOpen;
    }

    public setCircuitOpen(open: boolean): void {
        this.circuitOpen = open;
        if (open) {
            this.emit("stt_fallback_activated");
        } else {
            this.emit("stt_fallback_deactivated");
        }
    }
}
