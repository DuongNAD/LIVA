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
        it("should fast-track and emit natural language immediately without buffering", () => {
            const r1 = sanitizer.process("Hel");
            expect(r1.action).toBe("emit");
            expect(r1.cleanToken).toBe("Hel");
        });

        it("should buffer tokens starting with '<' or '{' until 10 chars accumulated", () => {
            const r1 = sanitizer.process("<t>Hel");
            expect(r1.action).toBe("buffer");

            const r2 = sanitizer.process("lo");
            expect(r2.action).toBe("buffer");

            const r3 = sanitizer.process(" W");
            expect(r3.action).toBe("emit");
            expect(r3.cleanToken).toContain("Hello W");
        });

        it("should emit buffer early on finish_reason", () => {
            const r = sanitizer.process("<t>Hi!", true);
            expect(r.action).toBe("emit");
            expect(r.cleanToken).toContain("Hi!");
        });
    });

    describe("Thinking Block Muting", () => {
        it("should emit styled thinking block when detected at buffer start", () => {
            const r1 = sanitizer.process("<thought>Let me think about this...");
            // Returns emit_thought with styled message to show thinking is in progress
            expect(r1.action).toBe("emit_thought");
            // Still shows thinking state
            expect(r1.cleanToken).toContain("[[SYS_THINKING]]");
        });

        it("should emit styled scratchpad block when detected at buffer start", () => {
            const r1 = sanitizer.process("<scratchpad>internal notes");
            expect(r1.action).toBe("emit_thought");
            expect(r1.cleanToken).toContain("[[SYS_THINKING]]");
        });

        it("should resume emitting after thinking block closes", () => {
            sanitizer.process("<thought>internal");
            sanitizer.process("</thought>Now I will answer.");
            // After close, passedBufferCheck resets — need 10 more chars
            const r = sanitizer.process("This is the real response that is long enough");
            expect(r.action).toBe("emit");
            expect(r.cleanToken).toContain("real response");
        });

        it("should emit styled thinking tag that appears mid-stream", () => {
            // First pass buffer check with normal content
            sanitizer.process("Normal response text that is long");

            // Then a thinking block appears mid-stream — returns emit_thought
            const r = sanitizer.process("<thought>");
            expect(r.action).toBe("emit_thought");
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
            // Returns emit_thought with styled message indicating skill usage
            expect(r.action).toBe("emit_thought");
        });

        it("should emit styled tool call when starting with JSON object", () => {
            const r = sanitizer.process('{"name": "web_search"}');
            // Returns emit_thought with styled message
            expect(r.action).toBe("emit_thought");
            expect(sanitizer.isToolCallMode).toBe(true);
        });

        it("should emit styled tool call when starting with multi-line JSON", () => {
            const r = sanitizer.process('{\n"name": "test"}');
            expect(r.action).toBe("emit_thought");
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
            
            // After tool_call is detected mid-stream, returns emit_thought with styled message
            const r = sanitizer.process('<tool_call>{"name": "test"}');
            expect(r.action).toBe("emit_thought");
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
