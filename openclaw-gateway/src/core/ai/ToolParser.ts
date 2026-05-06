import { jsonrepair } from "jsonrepair";
import { z } from "zod";
import { logger } from "../../utils/logger";

const ToolCallSchema = z.object({
    name: z.string().min(1),
    arguments: z.unknown().optional(),
    requiresApproval: z.boolean().optional(),
}).passthrough();

export type ParsedToolCall = z.infer<typeof ToolCallSchema>;

export interface ToolParseResult {
    readonly contentText: string;
    readonly toolCalls: ParsedToolCall[];
}

export class ToolParser {
    public parse(rawText: string): ToolParseResult {
        let contentText = rawText || "";

        if (contentText.includes("<tool_call>")) {
            const toolCalls = this.#parseXmlToolCalls(contentText);
            if (toolCalls.length > 0) {
                contentText = contentText.replaceAll(/<tool_call>[\s\S]*?<\/tool_call>/g, "").trim();
            }
            return { contentText, toolCalls };
        }

        if (contentText.includes('{"name":') && contentText.includes("}")) {
            return this.#parseJsonFallback(contentText);
        }

        return { contentText, toolCalls: [] };
    }

    #parseXmlToolCalls(contentText: string): ParsedToolCall[] {
        const matches = [...contentText.matchAll(/<tool_call>([\s\S]*?)<\/tool_call>/g)];
        const toolCalls: ParsedToolCall[] = [];

        for (const match of matches) {
            const rawJson = match[1]?.trim();
            if (!rawJson) {
                continue;
            }

            const toolCall = this.#parseToolJson(rawJson, "XML tool_call");
            if (toolCall) {
                toolCalls.push(toolCall);
            }
        }

        return toolCalls;
    }

    #parseJsonFallback(contentText: string): ToolParseResult {
        const firstIndex = contentText.indexOf('{"name":');
        const lastIndex = contentText.lastIndexOf("}");

        if (firstIndex === -1 || lastIndex <= firstIndex) {
            return { contentText, toolCalls: [] };
        }

        const rawJson = contentText.substring(firstIndex, lastIndex + 1);
        const toolCall = this.#parseToolJson(rawJson, "JSON fallback");
        if (!toolCall) {
            return { contentText, toolCalls: [] };
        }

        return {
            contentText: contentText.replace(rawJson, "").trim(),
            toolCalls: [toolCall],
        };
    }

    #parseToolJson(rawJson: string, context: string): ParsedToolCall | null {
        try {
            const parsed = JSON.parse(jsonrepair(rawJson)) as unknown;
            const result = ToolCallSchema.safeParse(parsed);
            if (!result.success) {
                logger.warn(`[ToolParser] Invalid ${context}: ${result.error.issues.map((issue) => issue.message).join("; ")}`);
                return null;
            }
            return result.data;
        } catch (error: unknown) {
            const message = error instanceof Error ? error.message : String(error);
            logger.error(`[ToolParser] Failed to parse ${context}: ${message}`);
            return null;
        }
    }
}
