import { jsonrepair } from "jsonrepair";
import { logger } from "../../utils/logger";

/**
 * ToolCallExtractor — Deterministic parser for LLM tool invocations.
 *
 * Extracted from AgentLoop.handleUserInput() to enable independent testing
 * and to centralize the parsing logic for XML <tool_call> and raw JSON formats.
 *
 * Supports:
 *   1. XML format: <tool_call>{"name":"...", "arguments":{...}}</tool_call>
 *   2. Raw JSON format: {"name":"...", "arguments":{...}} (with jsonrepair fallback)
 */

export interface ToolCall {
    name: string;
    arguments: any;
    requiresApproval?: boolean;
}

export interface ExtractionResult {
    /** Successfully parsed tool calls */
    readonly parsedToolCalls: ToolCall[];
    /** Content with tool call blocks removed */
    readonly cleanedContent: string;
}

export class ToolCallExtractor {
    /**
     * Extract tool calls from raw LLM output text.
     * Strips thinking blocks and stop sequences before parsing.
     */
    public extract(rawContent: string): ExtractionResult {
        // Pre-sanitize: strip thinking blocks and ALL model control tokens
        const thoughtCloseRegex = /<thought>[\s\S]*?(?:<\/thought>|<channel\|>|<\|channel\|>|<\|channel>|<\/channel>|<channel_thought>|\|\|channel\|\|)/g;
        const scratchCloseRegex = /<scratchpad>[\s\S]*?(?:<\/scratchpad>|<channel\|>|<\|channel\|>|<\|channel>|<\/channel>|<channel_thought>|\|\|channel\|\|)/g;

        let contentText = rawContent
            .replace(thoughtCloseRegex, "")
            .replace(scratchCloseRegex, "")
            .replace(/<\|channel>thought[\s\S]*?(?=<\/?tool_call>|{"name"|$)/g, "");

        // Handle unclosed thought/scratchpad blocks gracefully (fault-tolerant parsing)
        if (contentText.includes("<thought>") && !contentText.includes("</thought>") && !contentText.includes("<channel|>")) {
            contentText = contentText.replace(/<thought>[\s\S]*?(?=<tool_call>|{"name"|$)/g, "");
        }
        if (contentText.includes("<scratchpad>") && !contentText.includes("</scratchpad>") && !contentText.includes("<channel|>")) {
            contentText = contentText.replace(/<scratchpad>[\s\S]*?(?=<tool_call>|{"name"|$)/g, "");
        }

        contentText = contentText
            .replace(/<\/?end_of_turn>/g, "")
            .replace(/<\/?start_of_turn>/g, "")
            .replace(/<\|im_end\|>/g, "")
            .replace(/<\|eot_id\|>/g, "")
            .replace(/<eos>/g, "")
            .replace(/<\/s>/g, "")
            // Strip orphaned/partial tool_call tags (model sometimes emits closing tag without opening)
            .replace(/<\/?tool_call>/g, "")
            .trim();

        let parsedToolCalls: ToolCall[] = [];

        // Re-add <tool_call> wrapper for parsing if content contains JSON tool calls
        // This handles cases where model outputs: </tool_call> {"name":...} (orphaned closing tag)
        const hasToolCallXml = rawContent.includes("<tool_call>") && rawContent.includes("</tool_call>");

        // Strategy 1: XML <tool_call> blocks (from original raw content)
        if (hasToolCallXml) {
            try {
                const regex = /<tool_call>([\s\S]*?)<\/tool_call>/g;
                const matches = [...rawContent.matchAll(regex)];
                if (matches.length > 0) {
                    for (const match of matches) {
                        if (match[1]) {
                            const toolJson = JSON.parse(match[1].trim());
                            parsedToolCalls.push(toolJson);
                        }
                    }
                    // Clean from contentText (already stripped tags above, now strip JSON bodies)
                    for (const match of matches) {
                        if (match[1]) {
                            contentText = contentText.replace(match[1].trim(), "").trim();
                        }
                    }
                }
            } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e);
                logger.error("Lỗi Regex Parse Multi-Tool:" + " " + errMsg);
            }
        }
        // Strategy 2: Raw JSON {"name":...} (fallback via jsonrepair)
        // Catches orphaned tool calls that appear without <tool_call> wrapper
        if (parsedToolCalls.length === 0 && contentText.includes('{"name":') && contentText.includes("}")) {
            // 🔒 [Audit P0-1.2] Safe JSON Fallback via indexOf + jsonrepair (AI_CONTEXT §4.6)
            try {
                const firstIdx = contentText.indexOf('{"name":');
                const lastIdx = contentText.lastIndexOf("}");
                if (firstIdx !== -1 && lastIdx > firstIdx) {
                    const rawJson = contentText.substring(firstIdx, lastIdx + 1);
                    const toolJson = JSON.parse(jsonrepair(rawJson));
                    if (toolJson.name) parsedToolCalls = [toolJson];
                    contentText = contentText.replace(rawJson, "").trim();
                }
            } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e); void errMsg;
            }
        }

        // Final cleanup: strip any remaining model artifacts from visible content
        contentText = contentText
            .replace(/\{"name"\s*:\s*"[^"]*"\s*,\s*"arguments"\s*:\s*\{[\s\S]*?\}\s*\}/g, "")
            .replace(/<\/?tool_call>/g, "")
            .replace(/<\/?start_of_turn>/g, "")
            .replace(/<\/?end_of_turn>/g, "")
            .trim();

        return {
            parsedToolCalls,
            cleanedContent: contentText,
        };
    }

    /**
     * Parse tool call arguments from string to object.
     * Handles both pre-parsed objects and JSON strings with escape characters.
     */
    public parseArguments(functionName: string, rawArgs: any): any | null {
        if (typeof rawArgs !== "string") {
            return rawArgs;
        }

        try {
            const argsStr = rawArgs
                .replaceAll("\n", "\\n")
                .replaceAll("\r", "\\r")
                .replaceAll("\t", "\\t");
            return JSON.parse(argsStr);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error({ err: errMsg }, `Lỗi Parse JSON Argument định dạng hỏng kỹ năng ${functionName}`);
            return null;
        }
    }
}
