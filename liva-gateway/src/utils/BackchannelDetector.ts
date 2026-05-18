/**
 * BackchannelDetector — Classifies STT output as backchannel noise vs real speech
 * ==================================================================================
 * [v23 Sentient Omni-Duplex — Pillar 1: Two-Stage Semantic Barge-in]
 *
 * Problem: If we abort LLM on every speech_start, coughs/filler words ("ừm", "ok")
 * waste 100% of VRAM computation. If we wait for full transcription, AI talks over
 * the user for 1-2 seconds creating terrible UX.
 *
 * Solution: Two-stage approach:
 *   Stage 1 (speech_start) → Audio Ducking only (reduce TTS volume 20%)
 *   Stage 2 (transcription_ready) → Classify text:
 *     - Backchannel → Restore volume, AI continues
 *     - Real speech → Hard abort (kill LLM, truncate memory)
 *
 * Classification rules:
 *   1. Empty/whitespace → backchannel
 *   2. < 3 words AND matches filler pattern → backchannel
 *   3. Single Vietnamese interjection → backchannel
 *   4. Everything else → real speech (triggers hard abort)
 */

// Vietnamese filler words, interjections, and backchannel signals
const BACKCHANNEL_EXACT = new Set([
    // Vietnamese fillers
    "ừm", "ừ", "ờ", "à", "ạ", "ok", "oke", "okay", "uh", "uhm", "hmm",
    "uh huh", "hả", "hở", "hớ", "ơ", "ê", "ê ê", "vâng", "dạ", "rồi",
    "tiếp đi", "tiếp", "nói đi", "nói tiếp", "tiếp tục", "cứ nói",
    // English fillers
    "yeah", "yes", "yep", "no", "nope", "right", "sure", "mm", "mhm",
    "i see", "got it", "go on", "continue",
]);

// Patterns that indicate filler/noise even in multi-word context
const FILLER_PATTERNS = /^(ừm+|à+|ờ+|hm+|uh+|mm+|ah+|oh+|ơ+|ê+)[.!?,\s]*$/i;

export function isBackchannel(text: string): boolean {
    const trimmed = text.trim().toLowerCase();

    // Rule 1: Empty → backchannel
    if (!trimmed) return true;

    // Rule 2: Exact match against known fillers
    if (BACKCHANNEL_EXACT.has(trimmed)) return true;

    // Rule 3: Single filler sound pattern (ừừừm, àààà, etc.)
    if (FILLER_PATTERNS.test(trimmed)) return true;

    // Rule 4: Word count check — < 3 words AND very short
    const words = trimmed.split(/\s+/);
    if (words.length < 3 && trimmed.length < 10) {
        // Check if ALL words are fillers
        const allFiller = words.every(w => BACKCHANNEL_EXACT.has(w) || FILLER_PATTERNS.test(w));
        if (allFiller) return true;
    }

    // Default: real speech — trigger hard abort
    return false;
}
