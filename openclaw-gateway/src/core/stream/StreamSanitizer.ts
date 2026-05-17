import { logger } from "../../utils/logger";

/**
 * StreamSanitizer — State machine for LLM token stream filtering.
 * 
 * Extracted from AgentLoop.generateText() to enable independent testing
 * and reuse across different streaming contexts.
 * 
 * Responsibilities:
 *   1. Buffer first 10 chars to detect <thought>, <scratchpad>, <tool_call>, {"
 *   2. Mute thinking blocks entirely (insideThinkingBlock state)
 *   3. Strip stop sequences (<end_of_turn>, <|im_end|>, <eos>, </s>)
 *   4. Detect tool call mode to suppress UI streaming
 */

export type SanitizeAction = "emit" | "mute" | "buffer" | "tool_call_detected" | "emit_thought";

export interface SanitizeResult {
    readonly action: SanitizeAction;
    readonly cleanToken: string;
}

// Stop-sequence patterns shared between buffer and streaming phases
const STOP_SEQUENCE_REGEX = /(<\/?end_of_turn>|<\/?start_of_turn>|<\|im_end\|>|<\|eot_id\|>|<eos>|<\/s>|<\/?tool_call>)/g;

export class StreamSanitizer {
    #buffer = "";
    #fullContent = "";
    #passedBufferCheck = false;
    #isToolCallMode = false;
    #insideThinkingBlock = false;
    #thinkingCloseTag = "";
    #streamStarted = false;
    /** Edge buffer: holds trailing '<' or partial tags to merge with next token */
    #pendingEdge = "";
    #stripNextLeadingNewline = false;

    public get isToolCallMode(): boolean {
        return this.#isToolCallMode;
    }

    public get streamStarted(): boolean {
        return this.#streamStarted;
    }

    public markStreamStarted(): void {
        this.#streamStarted = true;
    }

    /**
     * Process a single raw token from the LLM stream.
     * Returns an action indicating what the caller should do with the token.
     */
    public process(rawToken: string, isFinishReason: boolean = false): SanitizeResult {
        this.#fullContent += rawToken;

        // Merge any pending edge from previous token
        const merged = this.#pendingEdge + rawToken;
        this.#pendingEdge = "";

        // If token ends with '<' or a partial tag opener, hold it for next merge
        // This prevents '<' being emitted before we know if it starts a control tag
        const trailingLt = merged.lastIndexOf('<');
        let tokenToProcess = merged;
        if (trailingLt >= 0 && trailingLt > merged.length - 20) {
            const tail = merged.substring(trailingLt);
            // If tail looks like start of a known control tag, buffer it
            if (!tail.includes('>')) {
                this.#pendingEdge = tail;
                tokenToProcess = merged.substring(0, trailingLt);
            }
        }

        // Strip stop sequences from the visible token
        let token = tokenToProcess.replace(STOP_SEQUENCE_REGEX, "");

        // [STATE: INSIDE THINKING BLOCK] Accumulate silently until close tag
        if (this.#insideThinkingBlock) {
            if (this.#fullContent.includes(this.#thinkingCloseTag)) {
                this.#insideThinkingBlock = false;
                // Extract only content AFTER the close tag
                const afterClose = this.#fullContent.split(this.#thinkingCloseTag).pop() || "";
                // Reset buffer for post-thinking content
                this.#buffer = afterClose;
                this.#passedBufferCheck = false;
                this.#stripNextLeadingNewline = true;
                
                const safeToken = token.replace("</thought>", "").replace("</scratchpad>", "").replace(/^>/, "");
                return { action: "emit_thought", cleanToken: `${safeToken}</i><br/>` };
            }
            return { action: "emit_thought", cleanToken: token.replace(/^>/, "") };
        }

        // [STATE: BUFFERING] Accumulate first 10 chars to classify stream type
        if (!this.#passedBufferCheck) {
            this.#buffer += token;

            if (this.#buffer.length >= 10 || isFinishReason) {
                this.#passedBufferCheck = true;
                const trimmedBuf = this.#buffer.trim();

                // Detect thinking blocks — mute entirely
                if (trimmedBuf.startsWith("<thought") || trimmedBuf.startsWith("<scratchpad")) {
                    this.#insideThinkingBlock = true;
                    this.#thinkingCloseTag = trimmedBuf.startsWith("<thought") ? "</thought>" : "</scratchpad>";
                    logger.info("[Stream Filter] 🧠 Phát hiện khối suy luận nội bộ, chuyển sang UI-only...");
                    
                    const tag = trimmedBuf.startsWith("<thought") ? "<thought>" : "<scratchpad>";
                    const tagIdx = this.#buffer.indexOf(tag);
                    let remainingText = "";
                    if (tagIdx !== -1) {
                        remainingText = this.#buffer.substring(tagIdx + tag.length)
                            .replace(/^>/, "")
                            .replace(/^\n/, "")
                            .replace("</thought>", "")
                            .replace("</scratchpad>", "");
                    }
                    return { action: "emit_thought", cleanToken: `<br/><i style="opacity: 0.7; font-size: 0.9em; color: gray;">💭 LIVA đang suy nghĩ:<br/>${remainingText}` };
                }

                // Detect tool calls — suppress stream, collect for JSON parsing
                if (trimmedBuf.startsWith("<to") || trimmedBuf.startsWith('{"') || trimmedBuf.startsWith('{\n')) {
                    this.#isToolCallMode = true;
                    logger.info("[Stream Mute] 🤫 LIVA đang nhẩm tính lệnh Kỹ năng ngầm...");
                    return { action: "emit_thought", cleanToken: `<br/><i style="opacity: 0.7; font-size: 0.9em; color: gray;">💭 LIVA đang dùng kỹ năng...</i><br/>` };
                }

                // Clean text content — emit buffer to UI
                const cleanBuffer = this.#buffer.replace(STOP_SEQUENCE_REGEX, "").trim();
                if (cleanBuffer) {
                    return { action: "emit", cleanToken: this.#buffer.replace(STOP_SEQUENCE_REGEX, "") };
                }
                return { action: "buffer", cleanToken: "" };
            }

            return { action: "buffer", cleanToken: "" };
        }

        // [STATE: STREAMING] Past buffer check — process token-by-token
        if (this.#isToolCallMode || !token) {
            return { action: "mute", cleanToken: "" };
        }

        if (this.#stripNextLeadingNewline) {
            token = token.replace(/^\s+/, "");
            if (token) this.#stripNextLeadingNewline = false;
        }

        // Runtime filter: catch thinking tags that appear mid-stream
        // [v23 FIX] Split text before the tag — emit the valid portion, mute thinking block
        const thoughtIdx = token.indexOf("<thought>");
        const scratchIdx = token.indexOf("<scratchpad>");
        const thinkTagIdx = thoughtIdx >= 0 ? thoughtIdx : scratchIdx;
        if (thinkTagIdx >= 0) {
            this.#insideThinkingBlock = true;
            this.#thinkingCloseTag = thoughtIdx >= 0 ? "</thought>" : "</scratchpad>";
            // Emit any text BEFORE the thinking tag (e.g., "Xong rồi ạ" from "Xong rồi ạ<thought>...")
            const beforeTag = token.substring(0, thinkTagIdx).replace(STOP_SEQUENCE_REGEX, "").trim();
            if (beforeTag) {
                return { action: "emit", cleanToken: beforeTag + `\n\n<i style="opacity: 0.7; font-size: 0.9em; color: gray;">💭 LIVA đang suy nghĩ:\n` };
            }
            return { action: "emit_thought", cleanToken: `\n\n<i style="opacity: 0.7; font-size: 0.9em; color: gray;">💭 LIVA đang suy nghĩ:\n` };
        }

        // Catch tool_call tags that appear mid-stream (after thinking blocks)
        if (token.includes("<tool_call>") || token.includes("</tool_call>") || token.includes('{"name"')) {
            this.#isToolCallMode = true;
            logger.info("[Stream Mute] 🤫 LIVA đang nhẩm tính lệnh Kỹ năng ngầm...");
            
            const toolIdx = token.indexOf("<tool_call>") !== -1 ? token.indexOf("<tool_call>") : token.indexOf('{"name"');
            const beforeTool = toolIdx !== -1 ? token.substring(0, toolIdx).replace(STOP_SEQUENCE_REGEX, "").trim() : "";
            
            if (this.#insideThinkingBlock) {
                 this.#insideThinkingBlock = false;
                 return { action: "emit_thought", cleanToken: beforeTool.replace(/^>/, "").replace("</thought>", "") + "</i><br/>" };
            }
            if (beforeTool) {
                return { action: "emit", cleanToken: beforeTool };
            }
            return { action: "emit_thought", cleanToken: `<br/><i style="opacity: 0.7; font-size: 0.9em; color: gray;">💭 LIVA đang dùng kỹ năng...</i><br/>` };
        }

        // Also mute if fullContent has entered a tool_call block
        if (this.#fullContent.includes("<tool_call>") && !this.#fullContent.includes("</tool_call>")) {
            this.#isToolCallMode = true;
            return { action: "mute", cleanToken: "" };
        }

        const cleanToken = token.replace(STOP_SEQUENCE_REGEX, "");
        if (cleanToken) {
            return { action: "emit", cleanToken };
        }

        return { action: "mute", cleanToken: "" };
    }

    /**
     * Get the full accumulated content (including thinking blocks, stop sequences, etc.)
     */
    public getFullContent(): string {
        return this.#fullContent;
    }

    /**
     * Reset internal state for a new stream.
     */
    public reset(): void {
        this.#buffer = "";
        this.#fullContent = "";
        this.#passedBufferCheck = false;
        this.#isToolCallMode = false;
        this.#insideThinkingBlock = false;
        this.#thinkingCloseTag = "";
        this.#streamStarted = false;
        this.#pendingEdge = "";
        this.#stripNextLeadingNewline = false;
    }
}
