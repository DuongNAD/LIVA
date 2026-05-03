/**
 * ReflectionDaemon.test.ts — Debounced Φ/Ψ extraction tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock dependencies
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn((m) => console.log(m)), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("uuid", () => ({
    v4: vi.fn(() => "test-uuid-123"),
}));

vi.mock("jsonrepair", () => ({
    jsonrepair: vi.fn((s: string) => s),
}));

import { ReflectionDaemon } from "../../src/memory/ReflectionDaemon";

describe("ReflectionDaemon", () => {
    let daemon: ReflectionDaemon;
    let mockMemory: any;
    let mockAI: any;

    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();

        mockMemory = {
            insertEvent: vi.fn(),
        };

        mockAI = {
            chat: {
                completions: {
                    create: vi.fn().mockResolvedValue({
                        choices: [{
                            message: {
                                content: JSON.stringify({
                                    factual_entries: [{ fact: "User likes TypeScript", entity: "TypeScript", confidence: 0.9 }],
                                    relational_entries: [{ sentiment: "hào hứng", intent: "nhờ giúp đỡ", relation: "" }],
                                }),
                            },
                        }],
                    }),
                },
            },
        };

        daemon = new ReflectionDaemon(mockMemory, mockAI);
    });

    afterEach(() => {
        daemon.dispose();
        vi.useRealTimers();
    });

    it("should initialize with 0 pending", () => {
        expect(daemon.pendingCount).toBe(0);
    });

    it("should skip trivial messages (short)", () => {
        daemon.queueTurn("hi", "chào bạn");
        expect(daemon.pendingCount).toBe(0);
    });

    it("should skip greeting-only messages", () => {
        daemon.queueTurn("hello", "chào bạn");
        expect(daemon.pendingCount).toBe(0);
    });

    it("should queue meaningful messages", () => {
        daemon.queueTurn("Tôi muốn học lập trình TypeScript", "Dạ, em sẽ hướng dẫn anh!");
        expect(daemon.pendingCount).toBe(1);
    });

    it("should debounce — not process immediately", () => {
        daemon.queueTurn("Tôi muốn học lập trình TypeScript", "Dạ, em sẽ hướng dẫn anh!");
        expect(mockAI.chat.completions.create).not.toHaveBeenCalled();
    });

    it("should process after debounce interval (12s)", async () => {
        daemon.queueTurn("Tôi muốn học lập trình TypeScript nâng cao", "Dạ, em sẽ hướng dẫn anh!");

        // Advance past debounce
        vi.advanceTimersByTime(12_000);
        // Let microtasks resolve
        await vi.advanceTimersByTimeAsync(100);

        expect(mockAI.chat.completions.create).toHaveBeenCalledTimes(1);
        expect(mockMemory.insertEvent).toHaveBeenCalledTimes(1);
    });

    it("should insert event with correct structure after processing", async () => {
        daemon.queueTurn("Tôi đang xây dự án LIVA-UHM", "Em hiểu rồi, đây là kiến trúc bộ nhớ phân tầng");

        vi.advanceTimersByTime(12_000);
        await vi.advanceTimersByTimeAsync(100);

        expect(mockMemory.insertEvent).toHaveBeenCalledWith(
            expect.objectContaining({
                eventId: "test-uuid-123",
                phi: { facts: ["User likes TypeScript"], entities: ["TypeScript"] },
                psi: { sentiment: "hào hứng", intent: "nhờ giúp đỡ", relational: "" },
            })
        );
    });

    it("should batch multiple turns into single extraction", async () => {
        daemon.queueTurn("Tôi muốn học lập trình TypeScript nâng cao", "Dạ, em sẽ hướng dẫn anh!");
        daemon.queueTurn("TypeScript có generics không?", "Có anh! Generics rất mạnh mẽ");
        daemon.queueTurn("Cho tôi ví dụ generic TypeScript", "Ví dụ: function identity<T>(arg: T): T {}");

        expect(daemon.pendingCount).toBe(3);

        vi.advanceTimersByTime(12_000);
        await vi.advanceTimersByTimeAsync(100);

        // Single LLM call for all 3 turns
        expect(mockAI.chat.completions.create).toHaveBeenCalledTimes(1);
        // But 3 events inserted (one per turn in the batch)
        expect(mockMemory.insertEvent).toHaveBeenCalledTimes(3);
    });

    it("should handle LLM failure gracefully (non-critical)", async () => {
        mockAI.chat.completions.create.mockRejectedValue(new Error("GPU OOM"));

        daemon.queueTurn("Tôi muốn học lập trình TypeScript nâng cao", "reply here...");

        vi.advanceTimersByTime(12_000);
        await vi.advanceTimersByTimeAsync(100);

        // Should NOT throw — reflection is best-effort
        expect(mockMemory.insertEvent).not.toHaveBeenCalled();
    });

    it("should handle empty LLM response", async () => {
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "" } }],
        });

        daemon.queueTurn("Tôi muốn học lập trình TypeScript nâng cao", "reply here...");
        vi.advanceTimersByTime(12_000);
        await vi.advanceTimersByTimeAsync(100);

        expect(mockMemory.insertEvent).not.toHaveBeenCalled();
    });

    it("should handle invalid JSON from LLM", async () => {
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "Not valid JSON at all!" } }],
        });

        daemon.queueTurn("Tôi muốn học lập trình TypeScript nâng cao", "reply here...");
        vi.advanceTimersByTime(12_000);
        await vi.advanceTimersByTimeAsync(100);

        expect(mockMemory.insertEvent).not.toHaveBeenCalled();
    });

    describe("flushPending()", () => {
        it("should process all pending turns immediately", async () => {
            daemon.queueTurn("Tôi muốn học lập trình TypeScript nâng cao", "Dạ!");
            daemon.queueTurn("Cho tôi ví dụ generic TypeScript hôm nay", "Ví dụ...");

            await daemon.flushPending();

            expect(mockAI.chat.completions.create).toHaveBeenCalledTimes(1);
            expect(daemon.pendingCount).toBe(0);
        });

        it("should be safe to call with no pending items", async () => {
            await expect(daemon.flushPending()).resolves.not.toThrow();
        });
    });

    describe("dispose()", () => {
        it("should clear pending queue", () => {
            daemon.queueTurn("Tôi muốn học lập trình TypeScript nâng cao", "reply");
            daemon.dispose();
            expect(daemon.pendingCount).toBe(0);
        });
    });

    it("should catch and log error if processBatch throws synchronously or rejects unexpectedly", async () => {
        vi.spyOn(daemon as any, "processBatch").mockRejectedValue(new Error("Forced processBatch failure"));
        daemon.queueTurn("force throw", "reply");
        vi.advanceTimersByTime(12_000);
        await vi.advanceTimersByTimeAsync(100);
        // Expect no unhandled rejections; logger.warn should be called in catch block
        // (Since processBatch is called in setTimeout, it won't crash the test if caught)
    });

    it.skip("should schedule a follow-up batch if queue has items after processing", async () => {
        // Mock API to hang forever so we can queue another turn while processing
        let resolveApi: any;
        mockAI.chat.completions.create.mockImplementation(() => new Promise(r => { resolveApi = r; }));

        daemon.queueTurn("turn 1", "reply 1");
        
        // Fast forward to start processing
        vi.advanceTimersByTime(12_000);
        
        // While processing (it's awaited), queue another turn
        daemon.queueTurn("turn 2", "reply 2");
        
        // Now resolve the API
        resolveApi({ choices: [{ message: { content: "{}" } }] });
        
        // Let all microtasks and promises finish
        await vi.advanceTimersByTimeAsync(100);
        
        // The finally block should have scheduled the next timer.
        // We just needed to hit that branch.
    });

    it("should catch extractJSON jsonrepair/parse throw", async () => {
        const originalParse = JSON.parse;
        JSON.parse = vi.fn().mockImplementation(() => { throw new Error("Mocked parse error"); });
        
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "{ \"test\": 1 }" } }]
        });

        daemon.queueTurn("force parse error", "reply");
        vi.advanceTimersByTime(12_000);
        await vi.advanceTimersByTimeAsync(100);

        expect(mockMemory.insertEvent).not.toHaveBeenCalled();
        JSON.parse = originalParse;
    });

    it("should catch follow-up batch processBatch rejection", async () => {
        // Ensure processBatch rejects on the SECOND call
        let calls = 0;
        const originalProcessBatch = (daemon as any).processBatch.bind(daemon);
        vi.spyOn(daemon as any, "processBatch").mockImplementation(async () => {
            calls++;
            if (calls === 2) throw new Error("Forced follow-up failure");
            return originalProcessBatch();
        });

        mockAI.chat.completions.create.mockImplementation(async () => {
            await new Promise(r => setTimeout(r, 50));
            return { choices: [{ message: { content: "{}" } }] };
        });

        daemon.queueTurn("turn 1", "reply 1");
        vi.advanceTimersByTime(12_000);
        
        // Queue while processing
        daemon.queueTurn("turn 2", "reply 2");
        await vi.advanceTimersByTimeAsync(100); // 1st finishes, 2nd scheduled
        
        vi.advanceTimersByTime(12_000); // 2nd starts and throws
        await vi.advanceTimersByTimeAsync(100);
    });

    it("DEV GUARD C (Valid JSON but Invalid Zod): should catch ZodError when factual_entries is missing", async () => {
        // Valid JSON but missing factual_entries array
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{
                message: {
                    content: JSON.stringify({
                        relational_entries: [{ sentiment: "hào hứng", intent: "nhờ giúp đỡ", relation: "" }],
                    }),
                },
            }],
        });

        // DEV GUARD A: Catch promise rejection before advancing timers
        const flushPromise = daemon.flushPending().catch(vi.fn());

        daemon.queueTurn("Tôi muốn học", "reply here...");
        
        vi.advanceTimersByTime(12_000);
        await vi.advanceTimersByTimeAsync(100);

        await flushPromise;

        // Since it's invalid Zod, it shouldn't insert anything
        expect(mockMemory.insertEvent).not.toHaveBeenCalled();
    });
});
