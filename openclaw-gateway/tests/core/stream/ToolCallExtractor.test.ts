import { describe, it, expect, vi } from "vitest";
import { ToolCallExtractor } from "../../../src/core/stream/ToolCallExtractor";

vi.mock("../../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

describe("ToolCallExtractor", () => {
    const extractor = new ToolCallExtractor();

    describe("XML <tool_call> Parsing", () => {
        it("should extract a single XML tool call", () => {
            const input = 'Here is the plan. <tool_call>{"name": "web_search", "arguments": {"query": "test"}}</tool_call>';
            const result = extractor.extract(input);
            expect(result.parsedToolCalls).toHaveLength(1);
            expect(result.parsedToolCalls[0].name).toBe("web_search");
            expect(result.parsedToolCalls[0].arguments.query).toBe("test");
            expect(result.cleanedContent).toBe("Here is the plan.");
        });

        it("should extract multiple XML tool calls", () => {
            const input = '<tool_call>{"name": "a", "arguments": {}}</tool_call> text <tool_call>{"name": "b", "arguments": {}}</tool_call>';
            const result = extractor.extract(input);
            expect(result.parsedToolCalls).toHaveLength(2);
            expect(result.parsedToolCalls[0].name).toBe("a");
            expect(result.parsedToolCalls[1].name).toBe("b");
        });

        it("should handle malformed XML tool call gracefully", () => {
            const input = '<tool_call>not valid json</tool_call>';
            const result = extractor.extract(input);
            // Should log error but not crash
            expect(result.parsedToolCalls).toHaveLength(0);
        });
    });

    describe("Raw JSON Parsing", () => {
        it("should extract raw JSON tool call with jsonrepair", () => {
            const input = 'Let me search. {"name": "web_search", "arguments": {"query": "hello"}} done.';
            const result = extractor.extract(input);
            expect(result.parsedToolCalls).toHaveLength(1);
            expect(result.parsedToolCalls[0].name).toBe("web_search");
            expect(result.cleanedContent).not.toContain('{"name"');
        });

        it("should handle JSON without name field", () => {
            const input = '{"other": "data"}';
            const result = extractor.extract(input);
            // No {"name":... pattern match
            expect(result.parsedToolCalls).toHaveLength(0);
        });
    });

    describe("Thinking Block Sanitization", () => {
        it("should strip <thought> blocks before parsing", () => {
            const input = '<thought>internal reasoning</thought><tool_call>{"name": "test", "arguments": {}}</tool_call>';
            const result = extractor.extract(input);
            expect(result.parsedToolCalls).toHaveLength(1);
            expect(result.cleanedContent).not.toContain("internal reasoning");
        });

        it("should strip <scratchpad> blocks before parsing", () => {
            const input = '<scratchpad>notes</scratchpad>Final answer.';
            const result = extractor.extract(input);
            expect(result.parsedToolCalls).toHaveLength(0);
            expect(result.cleanedContent).toBe("Final answer.");
        });
    });

    describe("Stop Sequence Stripping", () => {
        it("should strip all stop sequences from content", () => {
            const input = 'Hello<end_of_turn><|im_end|><eos></s>';
            const result = extractor.extract(input);
            expect(result.cleanedContent).toBe("Hello");
        });
    });

    describe("No Tool Calls", () => {
        it("should return empty array when no tool calls found", () => {
            const input = "This is a normal response with no tool calls.";
            const result = extractor.extract(input);
            expect(result.parsedToolCalls).toHaveLength(0);
            expect(result.cleanedContent).toBe(input);
        });
    });

    describe("parseArguments", () => {
        it("should return object args as-is", () => {
            const args = { query: "test" };
            expect(extractor.parseArguments("test_tool", args)).toEqual(args);
        });

        it("should parse JSON string args", () => {
            const result = extractor.parseArguments("test_tool", '{"query": "hello"}');
            expect(result).toEqual({ query: "hello" });
        });

        it("should handle args with newlines", () => {
            const result = extractor.parseArguments("test_tool", '{"code": "line1\nline2"}');
            expect(result).not.toBeNull();
        });

        it("should return null for malformed JSON string args", () => {
            const result = extractor.parseArguments("test_tool", "not json at all");
            expect(result).toBeNull();
        });
    });
});
