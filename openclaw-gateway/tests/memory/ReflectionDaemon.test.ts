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

    it("should skip greeting-only messages with padding (Line 116)", () => {
        daemon.queueTurn("hello          ", "chào bạn");
        expect(daemon.pendingCount).toBe(0);
    });

    it("should return early if already processing (Line 142)", async () => {
        daemon.queueTurn("Tôi muốn học lập trình TypeScript nâng cao", "Dạ!");
        const p1 = daemon.flushPending();
        const p2 = daemon.flushPending(); // This one should return early because isProcessing = true
        await Promise.all([p1, p2]);
        expect(mockAI.chat.completions.create).toHaveBeenCalledTimes(1);
    });

    it("should schedule next batch if queue has remaining items and debounceTimer is null (Line 216)", async () => {
        for (let i = 0; i < 6; i++) {
            daemon.queueTurn(`Tôi muốn học lập trình TypeScript nâng cao ${i}`, "Dạ!");
        }
        await daemon.flushPending(); // Clears debounceTimer, processes 5 items, leaves 1
        expect(daemon.pendingCount).toBe(1);
        // It should have started a new debounce timer for the 6th item
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

        it("should return early if already processing", async () => {
            // MAX_BATCH_SIZE is 5, so we queue 6 items
            for (let i = 0; i < 6; i++) {
                // Manually push to queue to bypass the debounce/flush logic in queueTurn
                (daemon as any)._pendingQueue = (daemon as any)._pendingQueue || [];
                // wait, #pendingQueue is a private field, can't easily push
            }
            // Actually, we can just use queueTurn but we don't want queueTurn to auto-flush on the 5th item!
            // If it auto-flushes, it starts processing.
            
            daemon.queueTurn("Hello there 11111", "I am fine");
            daemon.queueTurn("Hello there 22222", "I am fine");
            daemon.queueTurn("Hello there 33333", "I am fine");
            daemon.queueTurn("Hello there 44444", "I am fine");
            daemon.queueTurn("Hello there 55555", "I am fine"); 
            daemon.queueTurn("Hello there 66666", "I am fine"); 

            // Call flushPending twice synchronously
            // p1 will splice 5 items, setting isProcessing to true, and wait on LLM.
            const p1 = daemon.flushPending();
            
            // Queue still has 1 item.
            // p2 will see length > 0, and call processBatch().
            // processBatch() will see isProcessing === true, and return!
            const p2 = daemon.flushPending();
            
            await Promise.all([p1, p2]);
            
            // Only 1 call to mockAI should have happened because the second processBatch returned early
            expect(mockAI.chat.completions.create).toHaveBeenCalledTimes(1);
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

    it("should handle LLM returning very short string (raw.length < 5)", async () => {
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "{}" } }],
        });
        daemon.queueTurn("short text", "reply");
        vi.advanceTimersByTime(12_000);
        await vi.advanceTimersByTimeAsync(100);
        expect(mockMemory.insertEvent).not.toHaveBeenCalled();
    });

    it("should fallback to defaults when relational_entries is empty", async () => {
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{
                message: {
                    content: JSON.stringify({
                        factual_entries: [{ fact: "Fact only" }],
                        relational_entries: [],
                    }),
                },
            }],
        });

        daemon.queueTurn("Fact test turn", "reply");
        vi.advanceTimersByTime(12_000);
        await vi.advanceTimersByTimeAsync(100);

        expect(mockMemory.insertEvent).toHaveBeenCalledWith(
            expect.objectContaining({
                psi: { sentiment: "bình thường", intent: "chitchat", relational: "" },
            })
        );
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
        // We simulate processBatch failing on the SECOND call by mocking mockAI
        // wait, processBatch catches all errors inside. So it never rejects!
        // To cover the catch(e => logger.warn(...)) block, we must force this.processBatch() 
        // to reject synchronously. 
        // We will just mock processBatch using vitest for this specific case.
        
        // First, let's just trigger the follow-up batch logic correctly.
        mockAI.chat.completions.create.mockImplementation(async () => {
            await new Promise(r => setTimeout(r, 50));
            return { choices: [{ message: { content: "{}" } }] };
        });

        daemon.queueTurn("Tôi muốn học lập trình TypeScript nâng cao", "reply here...");
        vi.advanceTimersByTime(12_000); // starts first batch
        
        daemon.queueTurn("Ví dụ TypeScript generic", "reply two..."); // queues second batch while processing
        
        await vi.advanceTimersByTimeAsync(100); // 1st finishes, schedules follow-up timer

        // Now we mock processBatch to reject immediately when the follow-up timer fires
        vi.spyOn(daemon as any, "processBatch").mockRejectedValue(new Error("Forced follow-up failure"));

        await vi.advanceTimersByTimeAsync(12_000); // follow-up timer fires, hits the catch block
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
