/**
 * TTSFormatter — Semantic Clause Chunking for <300ms TTFS
 * =============================================================
 * [v22 Full-Duplex] Upgraded from sentence-level to clause-level splitting.
 *
 * Vietnamese sentences are typically 30-60 words long. Waiting for a period
 * before sending to TTS causes 1-3 second Time-To-First-Speech (TTFS).
 *
 * This formatter splits on:
 *   1. Sentence-ending punctuation (. ? ! \n) — primary boundary
 *   2. Vietnamese conjunctions (và, thì, mà, là, nhưng, vì, nên, hay) — semantic boundary
 *   3. Clause punctuation (, : ; —) — secondary boundary
 *   4. Word count overflow (>8 words without any boundary) — safety valve
 *
 * Result: User hears first clause within <300ms of first token.
 *
 * Design Rules (AI_CONTEXT.md):
 * - Rule 4.2: True Private Fields (#buffer)
 * - Rule 4.4: No `any` — strict TypeScript
 * - Anti-Pattern: TTS Word-by-Word Stuttering prevention
 */

// Unicode Emoji ranges (comprehensive — covers Emoji 15.0+)
const EMOJI_PATTERN = /[\u{1F600}-\u{1F64F}\u{1F300}-\u{1F5FF}\u{1F680}-\u{1F6FF}\u{1F1E0}-\u{1F1FF}\u{2600}-\u{26FF}\u{2700}-\u{27BF}\u{FE00}-\u{FE0F}\u{1F900}-\u{1F9FF}\u{1FA00}-\u{1FA6F}\u{1FA70}-\u{1FAFF}\u{200D}\u{20E3}\u{E0020}-\u{E007F}]/gu;

// Fenced code blocks: ```...``` (multi-line)
const CODE_BLOCK_PATTERN = /```[\s\S]*?```/g;

// Inline code: `...`
const INLINE_CODE_PATTERN = /`[^`]+`/g;

// URLs (http/https/ftp)
const URL_PATTERN = /https?:\/\/[^\s)>\]]+/gi;

// Markdown formatting: **bold**, *italic*, __underline__, ~~strikethrough~~, # headers, > blockquotes
const MARKDOWN_BOLD_ITALIC = /\*{1,3}([^*]+)\*{1,3}/g;
const MARKDOWN_UNDERLINE = /_{1,2}([^_]+)_{1,2}/g;
const MARKDOWN_STRIKE = /~~([^~]+)~~/g;
const MARKDOWN_HEADER = /^#{1,6}\s*/gm;
const MARKDOWN_BLOCKQUOTE = /^>\s*/gm;
const MARKDOWN_LIST = /^[-*+]\s+/gm;
const MARKDOWN_ORDERED_LIST = /^\d+\.\s+/gm;
const MARKDOWN_LINK = /\[([^\]]+)\]\([^)]+\)/g;

// Angle brackets (XML tags leak from LLM output)
const ANGLE_BRACKETS = /[<>]/g;

// Multiple spaces/newlines → single space
const MULTI_WHITESPACE = /\s{2,}/g;

// ============================================================
// [v22] SEMANTIC CLAUSE CHUNKING — Vietnamese-aware boundaries
// ============================================================

// Priority 1: Sentence-ending punctuation (.?!\n)
const SENTENCE_BOUNDARY = /([^.?!\n]*[.?!\n])\s*/;

// Priority 2: Vietnamese conjunctions — natural semantic break points
// Matches: "... và ", "... thì ", "... mà ", "... nhưng ", etc.
// Requires at least 4 words before conjunction to avoid splitting tiny fragments
const VN_CONJUNCTION_BOUNDARY = /^(.{15,}?\s+(?:và|thì|mà|nhưng|vì|nên|hay|hoặc|rồi|còn|do|bởi)\s)/u;

// Priority 3: Clause punctuation (comma, colon, semicolon, em-dash)
const CLAUSE_BOUNDARY = /([^,;:—]*[,;:—])\s*/;

// Minimum chars before allowing a clause split (prevents micro-fragments like "Dạ,")
const MIN_CLAUSE_LENGTH = 12;

// Word count overflow — force split after N words without any boundary
const MAX_WORDS_BEFORE_FORCE_SPLIT = 8;

// Maximum buffer before attempting comma split (lowered from 120 to 60 for faster TTFS)
const MAX_BUFFER_BEFORE_CLAUSE = 60;

export class TTSFormatter {
    #buffer: string = "";

    /**
     * Feed a streaming token into the buffer.
     * Returns a sanitized clause/sentence when a boundary is detected.
     * Returns null if more tokens are needed.
     *
     * [v22] Split order: Sentence > Conjunction > Clause > Word Overflow
     */
    pushToken(token: string): string | null {
        this.#buffer += token;

        // Priority 1: Check for sentence boundary (. ? ! \n)
        const sentenceMatch = SENTENCE_BOUNDARY.exec(this.#buffer);
        if (sentenceMatch && sentenceMatch.index !== undefined) {
            const sentence = sentenceMatch[1].trim();
            this.#buffer = this.#buffer.substring(sentenceMatch.index + sentenceMatch[0].length);

            const sanitized = this.#sanitize(sentence);
            return sanitized.length > 0 ? sanitized : null;
        }

        // Priority 2: Vietnamese conjunction split (semantic boundary)
        if (this.#buffer.length > MIN_CLAUSE_LENGTH) {
            const conjMatch = VN_CONJUNCTION_BOUNDARY.exec(this.#buffer);
            if (conjMatch) {
                const clause = conjMatch[1].trim();
                this.#buffer = this.#buffer.substring(conjMatch[0].length);

                const sanitized = this.#sanitize(clause);
                return sanitized.length > 0 ? sanitized : null;
            }
        }

        // Priority 3: Clause punctuation split (, : ; —) when buffer is getting long
        if (this.#buffer.length > MAX_BUFFER_BEFORE_CLAUSE) {
            const clauseMatch = CLAUSE_BOUNDARY.exec(this.#buffer);
            if (clauseMatch && clauseMatch.index !== undefined) {
                const clause = clauseMatch[1].trim();
                // Only split if the clause is substantial enough
                if (clause.length >= MIN_CLAUSE_LENGTH) {
                    this.#buffer = this.#buffer.substring(clauseMatch.index + clauseMatch[0].length);

                    const sanitized = this.#sanitize(clause);
                    return sanitized.length > 0 ? sanitized : null;
                }
            }
        }

        // Priority 4: Word count overflow — force split at last space
        const wordCount = this.#buffer.trim().split(/\s+/).length;
        if (wordCount >= MAX_WORDS_BEFORE_FORCE_SPLIT && this.#buffer.length > MIN_CLAUSE_LENGTH) {
            const lastSpace = this.#buffer.lastIndexOf(' ');
            if (lastSpace > MIN_CLAUSE_LENGTH) {
                const chunk = this.#buffer.substring(0, lastSpace).trim();
                this.#buffer = this.#buffer.substring(lastSpace + 1);

                const sanitized = this.#sanitize(chunk);
                return sanitized.length > 0 ? sanitized : null;
            }
        }

        return null;
    }

    /**
     * Flush any remaining text in the buffer (call at end of stream).
     * Returns sanitized remainder or null if empty.
     */
    flush(): string | null {
        if (!this.#buffer.trim()) {
            this.#buffer = "";
            return null;
        }

        const sanitized = this.#sanitize(this.#buffer.trim());
        this.#buffer = "";
        return sanitized.length > 0 ? sanitized : null;
    }

    /**
     * Reset the buffer (e.g., on preempt/interrupt).
     */
    reset(): void {
        this.#buffer = "";
    }

    /**
     * Strip all non-speakable artifacts from text.
     * Preserves Vietnamese characters, punctuation, and natural speech patterns.
     */
    #sanitize(text: string): string {
        let result = text;

        // 1. Remove fenced code blocks first (```...```)
        result = result.replace(CODE_BLOCK_PATTERN, "");

        // 2. Remove inline code (`...`)
        result = result.replace(INLINE_CODE_PATTERN, "");

        // 3. Remove URLs
        result = result.replace(URL_PATTERN, "");

        // 4. Extract text from Markdown links [text](url) → text
        result = result.replace(MARKDOWN_LINK, "$1");

        // 5. Strip Markdown formatting (preserve inner text)
        result = result.replace(MARKDOWN_BOLD_ITALIC, "$1");
        result = result.replace(MARKDOWN_UNDERLINE, "$1");
        result = result.replace(MARKDOWN_STRIKE, "$1");
        result = result.replace(MARKDOWN_HEADER, "");
        result = result.replace(MARKDOWN_BLOCKQUOTE, "");
        result = result.replace(MARKDOWN_LIST, "");
        result = result.replace(MARKDOWN_ORDERED_LIST, "");

        // 6. Replace angle brackets with spaces (XML/HTML tag leaks)
        result = result.replace(ANGLE_BRACKETS, " ");

        // 7. Remove all emoji
        result = result.replace(EMOJI_PATTERN, "");

        // 8. Collapse multiple whitespace into single space
        result = result.replace(MULTI_WHITESPACE, " ");

        return result.trim();
    }
}
