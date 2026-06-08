/**
 * EdgeTTSClient — Direct Node.js Edge-TTS Synthesis
 * ===================================================
 * [Optimization C1] Replaces Python voice_engine.py (port 8002).
 *
 * Benefits:
 *   - Eliminates Python process (~60MB RAM saved)
 *   - Removes WS relay overhead (Gateway → Python → Azure → Python → Gateway)
 *   - Direct Azure Speech CDN call from Node.js
 *   - Uses edge-tts-universal npm package (MIT, actively maintained)
 *
 * API: Uses `Communicate.stream()` async iterator for chunk-by-chunk audio.
 *
 * Design Rules (AI_CONTEXT.md):
 *   - Rule 4.2: True Private Fields (#voice, #rate)
 *   - Rule 4.4: No `any` — strict TypeScript
 *   - Anti-Pattern: Must not block Event Loop
 *
 * @module EdgeTTSClient
 */

import { logger } from "../utils/logger";

// Whitelist of allowed Edge-TTS voices (mirrors Python voice_engine.py)
const ALLOWED_VOICES = new Set([
    "vi-VN-HoaiMyNeural",
    "vi-VN-NamMinhNeural",
    "en-US-AvaMultilingualNeural",
    "en-US-AriaNeural",
    "en-US-JennyNeural",
    "en-US-MichelleNeural",
    "en-US-EmmaMultilingualNeural",
    "en-US-EmmaNeural",
    "en-US-AnaNeural",
    "ja-JP-NanamiNeural",
    "ko-KR-SunHiNeural",
    "zh-CN-XiaoxiaoNeural",
    "zh-CN-XiaoyiNeural",
]);

const DEFAULT_VOICE = "vi-VN-HoaiMyNeural";
const DEFAULT_RATE = "+15%";
const MAX_RETRIES = 2;

/** Type for the Communicate constructor from edge-tts-universal */
type CommunicateConstructor = new (text: string, options: { voice: string; rate?: string }) => {
    stream(): AsyncIterable<{ type: string; data?: Uint8Array; text?: string }>;
};

export class EdgeTTSClient {
    #voice: string = DEFAULT_VOICE;
    #rate: string = DEFAULT_RATE;
    // Lazy-loaded Communicate constructor from edge-tts-universal
    #CommunicateCtor: CommunicateConstructor | null = null;

    /**
     * Synthesize text to audio bytes (MP3 format).
     * Returns a Buffer containing the complete audio data, or null on failure.
     *
     * Uses retry logic with exponential backoff matching Python voice_engine.py.
     */
    public async synthesize(text: string): Promise<Buffer | null> {
        if (!text.trim()) return null;

        const Ctor = await this.#getModule();
        if (!Ctor) return null;

        for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
            try {
                const communicate = new Ctor(text, {
                    voice: this.#voice,
                    rate: this.#rate,
                });

                const chunks: Buffer[] = [];
                for await (const chunk of communicate.stream()) {
                    if (chunk.type === "audio" && chunk.data) {
                        chunks.push(Buffer.from(chunk.data));
                    }
                }

                if (chunks.length > 0) {
                    return Buffer.concat(chunks);
                }

                logger.warn("[EdgeTTSClient] Synthesis returned no audio chunks.");
                return null;
            } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e);
                if (attempt < MAX_RETRIES) {
                    logger.info(`[EdgeTTSClient] ⚠️ Azure connection error (retry ${attempt + 1}/${MAX_RETRIES})...`);
                    await new Promise(r => setTimeout(r, 500 * (attempt + 1)));
                } else {
                    logger.warn(`[EdgeTTSClient] TTS synthesis failed after ${MAX_RETRIES + 1} attempts: ${errMsg}`);
                    return null;
                }
            }
        }

        return null;
    }

    /**
     * Change voice at runtime. Validates against whitelist.
     */
    public setVoice(voiceId: string): boolean {
        if (!voiceId.trim()) {
            logger.warn("[EdgeTTSClient] setVoice: empty voice ID, ignoring.");
            return false;
        }
        if (!ALLOWED_VOICES.has(voiceId)) {
            logger.warn(`[EdgeTTSClient] setVoice: '${voiceId}' not in whitelist, ignoring.`);
            return false;
        }
        const old = this.#voice;
        this.#voice = voiceId;
        logger.info(`[EdgeTTSClient] 🎤 Voice changed: ${old} → ${voiceId}`);
        return true;
    }

    /**
     * Get current voice ID.
     */
    public get voice(): string {
        return this.#voice;
    }

    /**
     * Change speech rate (e.g., "+15%", "-10%", "+0%").
     */
    public setRate(rate: string): void {
        this.#rate = rate;
    }

    /**
     * Lazy-load the edge-tts-universal Communicate class.
     * Cached after first import to avoid repeated dynamic import overhead.
     */
    async #getModule(): Promise<CommunicateConstructor | null> {
        if (this.#CommunicateCtor) return this.#CommunicateCtor;

        try {
            // @ts-ignore
            const mod = await import("edge-tts-universal");
            this.#CommunicateCtor = mod.Communicate as CommunicateConstructor;
            logger.info("[EdgeTTSClient] ✅ edge-tts-universal module loaded.");
            return this.#CommunicateCtor;
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[EdgeTTSClient] ❌ Failed to import edge-tts-universal: ${errMsg}`);
            return null;
        }
    }
}
