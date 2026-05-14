import { describe, it, expect, vi, beforeEach } from "vitest";
import { StreamSanitizer } from "../../../src/core/stream/StreamSanitizer";

vi.mock("../../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

describe("StreamSanitizer", () => {
    let sanitizer: StreamSanitizer;

    beforeEach(() => {
        sanitizer = new StreamSanitizer();
    });

    describe("Buffering Phase", () => {
        it("should buffer tokens until 10 chars accumulated", () => {
            const r1 = sanitizer.process("Hel");
            expect(r1.action).toBe("buffer");

            const r2 = sanitizer.process("lo W");
            expect(r2.action).toBe("buffer");

            // At 10+ chars, should emit the clean buffer
            const r3 = sanitizer.process("orld!");
            expect(r3.action).toBe("emit");
            expect(r3.cleanToken).toContain("Hello World!");
        });

        it("should emit buffer early on finish_reason", () => {
            const r = sanitizer.process("Hi!", true);
            expect(r.action).toBe("emit");
            expect(r.cleanToken).toContain("Hi!");
        });
    });

    describe("Thinking Block Muting", () => {
        it("should mute <thought> blocks detected at buffer start", () => {
            const r1 = sanitizer.process("<thought>Let me think about this...");
            expect(r1.action).toBe("mute");
            expect(sanitizer.process("still thinking")).toEqual({ action: "mute", cleanToken: "" });
        });

        it("should mute <scratchpad> blocks detected at buffer start", () => {
            const r1 = sanitizer.process("<scratchpad>internal notes");
            expect(r1.action).toBe("mute");
        });

        it("should resume emitting after thinking block closes", () => {
            sanitizer.process("<thought>internal");
            sanitizer.process("</thought>Now I will answer.");
            // After close, passedBufferCheck resets — need 10 more chars
            const r = sanitizer.process("This is the real response that is long enough");
            expect(r.action).toBe("emit");
            expect(r.cleanToken).toContain("real response");
        });

        it("should mute <thought> tags that appear mid-stream", () => {
            // First pass buffer check with normal content
            sanitizer.process("Normal response text that is long");

            // Then a thinking block appears mid-stream
            const r = sanitizer.process("<thought>");
            expect(r.action).toBe("mute");
        });
    });

    describe("Tool Call Detection", () => {
        it("should detect tool call starting with <to (tool_call tag)", () => {
            // <tool_call> gets stripped by stop sequence regex leaving just '{'
            // But the original AgentLoop checks trimmedBuf.startsWith("<to")
            // which means the raw buffer (before regex) must start with '<to'
            // This requires enough content to pass the 10-char check
            const r = sanitizer.process("<tool_call>{\"name\": \"test\"}");
            // After stop-sequence stripping, buffer becomes '{"name": "test"}'
            // which starts with '{"' — tool call detected
            expect(sanitizer.isToolCallMode).toBe(true);
        });

        it("should detect tool call starting with JSON object", () => {
            const r = sanitizer.process('{"name": "web_search"}');
            expect(r.action).toBe("tool_call_detected");
            expect(sanitizer.isToolCallMode).toBe(true);
        });

        it("should detect tool call starting with multi-line JSON", () => {
            const r = sanitizer.process('{\n"name": "test"}');
            expect(r.action).toBe("tool_call_detected");
            expect(sanitizer.isToolCallMode).toBe(true);
        });

        it("should mute all subsequent tokens after tool call detected", () => {
            sanitizer.process('{"name": "search"');
            const r = sanitizer.process(', "arguments": {}}');
            expect(r.action).toBe("mute");
        });

        it("should detect tool_call tags mid-stream", () => {
            // Pass buffer check with normal content
            sanitizer.process("Normal text that passes buffer");
            
            const r = sanitizer.process('<tool_call>{"name": "test"}');
            expect(r.action).toBe("mute");
            expect(sanitizer.isToolCallMode).toBe(true);
        });
    });

    describe("Stop Sequence Stripping", () => {
        it("should strip <end_of_turn> from tokens", () => {
            sanitizer.process("Hello world! This is a test");
            const r = sanitizer.process("goodbye<end_of_turn>");
            expect(r.cleanToken).toBe("goodbye");
        });

        it("should strip <|im_end|> from tokens", () => {
            sanitizer.process("Hello world! This is a test");
            const r = sanitizer.process("farewell<|im_end|>");
            expect(r.cleanToken).toBe("farewell");
        });

        it("should strip <eos> from tokens", () => {
            sanitizer.process("Hello world! This is a test");
            const r = sanitizer.process("end<eos>");
            expect(r.cleanToken).toBe("end");
        });

        it("should strip </s> from tokens", () => {
            sanitizer.process("Hello world! This is a test");
            const r = sanitizer.process("done</s>");
            expect(r.cleanToken).toBe("done");
        });
    });

    describe("Content Tracking", () => {
        it("should track full content including muted parts", () => {
            sanitizer.process("Hello ");
            sanitizer.process("World!");
            expect(sanitizer.getFullContent()).toBe("Hello World!");
        });
    });

    describe("Reset", () => {
        it("should reset all internal state", () => {
            sanitizer.process('{"name": "test"}');
            expect(sanitizer.isToolCallMode).toBe(true);

            sanitizer.reset();
            expect(sanitizer.isToolCallMode).toBe(false);
            expect(sanitizer.getFullContent()).toBe("");
            expect(sanitizer.streamStarted).toBe(false);
        });
    });

    describe("Stream Started", () => {
        it("should track stream started state", () => {
            expect(sanitizer.streamStarted).toBe(false);
            sanitizer.markStreamStarted();
            expect(sanitizer.streamStarted).toBe(true);
        });
    });
});
