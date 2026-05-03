/**
 * PersonalKnowledgeExtractor.test.ts — v4.0 Enterprise Tests
 * Tests micro-batching, route filtering, fact reconciliation, and dispose
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock StructuredMemory
const mockSetFact = vi.fn();
const mockSetFactImportance = vi.fn();
const mockStructuredMemory = {
    setFact: mockSetFact,
    setFactImportance: mockSetFactImportance,
} as any;

// Mock OpenAI client
const mockCreate = vi.fn();
const mockAiClient = {
    chat: {
        completions: {
            create: mockCreate,
        },
    },
} as any;

import { PersonalKnowledgeExtractor } from "../../src/memory/PersonalKnowledgeExtractor";

describe("PersonalKnowledgeExtractor", () => {
    let pke: PersonalKnowledgeExtractor;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        pke = new PersonalKnowledgeExtractor(mockStructuredMemory, mockAiClient);
    });

    afterEach(() => {
        pke.dispose();
        vi.useRealTimers();
    });

    describe("[v4.0] Route Filtering (G-5)", () => {
        it("should skip extraction for system_command route", () => {
            pke.queueForExtraction("chạy lệnh ls -la", "OK, đang chạy...", "system_command");
            expect(pke["pendingBuffer"]).toHaveLength(0);
        });

        it("should skip extraction for deep_reasoning route", () => {
            pke.queueForExtraction("phân tích mã nguồn này giúp tôi", "OK phân tích...", "deep_reasoning");
            expect(pke["pendingBuffer"]).toHaveLength(0);
        });

        it("should skip extraction for tool_recall route", () => {
            pke.queueForExtraction("chạy lại lệnh đó", "OK...", "tool_recall");
            expect(pke["pendingBuffer"]).toHaveLength(0);
        });

        it("should allow extraction for chitchat route", () => {
            pke.queueForExtraction("Mẹ tôi tên là Lan, bà ấy rất giỏi nấu ăn", "Dạ vâng", "chitchat");
            expect(pke["pendingBuffer"]).toHaveLength(1);
        });

        it("should allow extraction for factual_recall route", () => {
            pke.queueForExtraction("Tôi đang làm việc tại công ty Viettel", "Dạ ghi nhớ", "factual_recall");
            expect(pke["pendingBuffer"]).toHaveLength(1);
        });

        it("should allow extraction when no route provided", () => {
            pke.queueForExtraction("Tôi thích uống cà phê đen buổi sáng", "Hay quá");
            expect(pke["pendingBuffer"]).toHaveLength(1);
        });
    });

    describe("[v4.0] Input Filtering", () => {
        it("should skip short messages (< 10 chars)", () => {
            pke.queueForExtraction("hi", "hello");
            expect(pke["pendingBuffer"]).toHaveLength(0);
        });

        it("should skip trivial greetings", () => {
            pke.queueForExtraction("xin chào", "Chào bạn!");
            expect(pke["pendingBuffer"]).toHaveLength(0);
        });

        it("should skip empty messages", () => {
            pke.queueForExtraction("", "response");
            expect(pke["pendingBuffer"]).toHaveLength(0);
        });
    });

    describe("[v4.0] Buffered Micro-Batching (MEM-104)", () => {
        it("should accumulate turns in buffer", () => {
            // Short messages that won't trigger 200-char flush
            pke.queueForExtraction("Tôi tên Hùng", "Xin chào Hùng");
            expect(pke["pendingBuffer"]).toHaveLength(1);
        });

        it("should flush buffer immediately when total length > 200 chars", () => {
            // First push a short message
            pke.queueForExtraction("Tôi tên là Hùng, tôi đang làm việc", "OK ghi nhận");
            // Second push to exceed 200 chars total (buffer accumulates formatted strings)
            const longMsg = "Tôi đang làm việc tại công ty Viettel, bộ phận phát triển phần mềm AI, sếp tên Hùng rất tốt bụng và hay giúp đỡ mọi người trong team nha";
            
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "[]" } }]
            });

            pke.queueForExtraction(longMsg, "Dạ, tôi ghi nhớ rồi ạ!");
            // Buffer should be cleared after immediate flush (flushBuffer empties synchronously)
            expect(pke["pendingBuffer"]).toHaveLength(0);
        });

        it("should flush buffer on idle timeout (60s)", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "[]" } }]
            });

            pke.queueForExtraction("Tôi tên Hùng nè", "Chào Hùng!");

            expect(pke["pendingBuffer"]).toHaveLength(1);

            // Advance timer by 60s to trigger idle flush
            vi.advanceTimersByTime(60_000);

            // Buffer should be cleared
            expect(pke["pendingBuffer"]).toHaveLength(0);
        });

        it("should catch and log batch extraction errors (Line 108)", async () => {
            vi.spyOn(pke, "extractAndStore").mockRejectedValueOnce(new Error("Simulated batch failure"));
            
            pke.queueForExtraction("Hello 1234567", "Hello!");
            expect(pke["pendingBuffer"]).toHaveLength(1);

            vi.advanceTimersByTime(60_000);
            await Promise.resolve();

            expect(pke["pendingBuffer"]).toHaveLength(0);
        });
    });

    describe("[v4.0] Fact Reconciliation (G-9)", () => {
        it("should soft-deprecate old fact when replaces_key is provided", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{
                    message: {
                        content: JSON.stringify([{
                            key: "cong_ty_hien_tai",
                            value: "Đang làm ở Viettel",
                            category: "Công việc",
                            replaces_key: "cong_ty_cu"
                        }])
                    }
                }]
            });

            await pke.extractAndStore("Tôi vừa chuyển sang Viettel\nLIVA: Hay quá");

            expect(mockSetFactImportance).toHaveBeenCalledWith("cong_ty_cu", 0.1);
            expect(mockSetFact).toHaveBeenCalledWith("cong_ty_hien_tai", "Đang làm ở Viettel", expect.any(Object));
        });

        it("should not deprecate when replaces_key matches current key", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{
                    message: {
                        content: JSON.stringify([{
                            key: "hobby",
                            value: "Thích cà phê",
                            category: "Sở thích",
                            replaces_key: "hobby"
                        }])
                    }
                }]
            });

            await pke.extractAndStore("Tôi thích cà phê\nLIVA: OK");

            expect(mockSetFactImportance).not.toHaveBeenCalled();
            expect(mockSetFact).toHaveBeenCalled();
        });
    });

    describe("[v4.0] Error Handling", () => {
        it("should handle LLM errors gracefully", async () => {
            mockCreate.mockRejectedValueOnce(new Error("API timeout"));

            await expect(pke.extractAndStore("test content")).resolves.not.toThrow();
        });

        it("should handle malformed JSON response", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "not valid json at all" } }]
            });

            await expect(pke.extractAndStore("test content")).resolves.not.toThrow();
        });

        it("should handle empty response", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "[]" } }]
            });

            await pke.extractAndStore("test content");
            expect(mockSetFact).not.toHaveBeenCalled();
        });
    });

    describe("[v4.0] Dispose", () => {
        it("should clear idle timer", () => {
            pke.queueForExtraction("Tên tôi là Hùng nhé", "OK Hùng");
            expect(pke["idleTimer"]).not.toBeNull();

            pke.dispose();
            expect(pke["idleTimer"]).toBeNull();
        });

        it("should be safe to call multiple times", () => {
            pke.dispose();
            expect(() => pke.dispose()).not.toThrow();
        });
    });

    describe("totalExtracted", () => {
        it("should track extraction count", async () => {
            expect(pke.totalExtracted).toBe(0);

            mockCreate.mockResolvedValueOnce({
                choices: [{
                    message: {
                        content: JSON.stringify([
                            { key: "k1", value: "v1", category: "Sở thích", replaces_key: null }
                        ])
                    }
                }]
            });

            await pke.extractAndStore("content");
            expect(pke.totalExtracted).toBe(1);
        });
    });
});
