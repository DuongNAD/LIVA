import { describe, expect, it, vi } from "vitest";
import { ToolParser } from "../../../src/core/ai/ToolParser";

vi.mock("../../../src/utils/logger", () => ({
    logger: {
        warn: vi.fn(),
        error: vi.fn(),
        info: vi.fn(),
        debug: vi.fn(),
        child: vi.fn().mockReturnThis(),
    },
}));

describe("ToolParser", () => {
    it("parses XML tool_call blocks and strips them from assistant text", () => {
        const parser = new ToolParser();
        const result = parser.parse(
            'prefix <tool_call>{"name":"read_local_file","arguments":{"filePath":"README.md"}}</tool_call> suffix',
        );

        expect(result.contentText).toBe("prefix  suffix");
        expect(result.toolCalls).toEqual([
            {
                name: "read_local_file",
                arguments: { filePath: "README.md" },
            },
        ]);
    });

    it("parses multiple XML tool calls", () => {
        const parser = new ToolParser();
        const result = parser.parse(
            '<tool_call>{"name":"get_system_info","arguments":{}}</tool_call><tool_call>{"name":"send_email","arguments":{"to":"a@example.com"},"requiresApproval":true}</tool_call>',
        );

        expect(result.contentText).toBe("");
        expect(result.toolCalls.map((toolCall) => toolCall.name)).toEqual(["get_system_info", "send_email"]);
        expect(result.toolCalls[1].requiresApproval).toBe(true);
    });

    it("uses safe JSON fallback without greedy regex parsing", () => {
        const parser = new ToolParser();
        const result = parser.parse(
            'before {"name":"send_zalo_bot","arguments":{"message":"ok"}} after text',
        );

        expect(result.contentText).toBe("before  after text");
        expect(result.toolCalls).toEqual([
            {
                name: "send_zalo_bot",
                arguments: { message: "ok" },
            },
        ]);
    });

    it("ignores malformed tool payloads instead of throwing", () => {
        const parser = new ToolParser();
        const result = parser.parse('<tool_call>{"arguments":{}}</tool_call> visible text');

        expect(result.contentText).toBe('<tool_call>{"arguments":{}}</tool_call> visible text');
        expect(result.toolCalls).toEqual([]);
    });
});
