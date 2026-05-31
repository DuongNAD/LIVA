import { describe, it, expect, vi, beforeEach, afterEach, type Mocked } from "vitest";
import { ConsolidationCron } from "../../src/memory/ConsolidationCron";
import { StructuredMemory } from "../../src/memory/StructuredMemory";
import { EmbeddingService } from "../../src/services/EmbeddingService";
import { BookIndex } from "../../src/memory/BookIndex";
import OpenAI from "openai";

vi.mock("../../src/memory/StructuredMemory");
vi.mock("../../src/services/EmbeddingService");
vi.mock("../../src/memory/BookIndex");
vi.mock("openai");

describe("ConsolidationCron", () => {
    let cron: ConsolidationCron;
    let mockStructuredMemory: Mocked<StructuredMemory>;
    let mockEmbeddingService: Mocked<EmbeddingService>;
    let mockBookIndex: Mocked<BookIndex>;
    let mockOpenAI: Mocked<OpenAI>;

    beforeEach(() => {
        vi.useFakeTimers();
        
        mockStructuredMemory = new StructuredMemory("test-agent.sqlite") as any;
        mockEmbeddingService = Object.create(EmbeddingService.prototype) as any;
        mockEmbeddingService.embed = vi.fn().mockResolvedValue(new Array(384).fill(0.1));
        
        mockBookIndex = new BookIndex() as any;
        mockOpenAI = new OpenAI({ apiKey: "test" }) as any;
        
        mockOpenAI.chat = {
            completions: {
                create: vi.fn()
            }
        } as any;

        cron = new ConsolidationCron(
            mockStructuredMemory,
            mockEmbeddingService,
            mockBookIndex,
            mockOpenAI
        );
    });

    afterEach(() => {
        cron.dispose();
        vi.useRealTimers();
        vi.clearAllMocks();
    });

    it("should start and stop idle timer", () => {
        const setIntervalSpy = vi.spyOn(global, 'setInterval');
        const clearIntervalSpy = vi.spyOn(global, 'clearInterval');
        
        cron.start();
        expect(setIntervalSpy).toHaveBeenCalled();
        
        cron.stop();
        expect(clearIntervalSpy).toHaveBeenCalled();
        
        setIntervalSpy.mockRestore();
        clearIntervalSpy.mockRestore();
    });

    it("should touch updates lastInteractionTime", () => {
        const initial = (cron as any).lastInteractionTime;
        vi.advanceTimersByTime(1000);
        cron.touch();
        expect((cron as any).lastInteractionTime).toBeGreaterThan(initial);
    });

    it("should preflightCheck and consolidate if above threshold", async () => {
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`,
                timestamp: Date.now() - i * 1000,
                rawUserMsg: "test",
                rawAiReply: "test",
                phi: { facts: [] },
                psi: { sentiment: "neutral" }
            }))
        );

        (mockOpenAI.chat.completions.create as any).mockResolvedValue({
            choices: [{
                message: {
                    content: '{"narrative_summary":"summary", "new_user_insights":[{"key":"k","value":"v"}]}'
                }
            }]
        });

        await cron.preflightCheck();

        expect(mockStructuredMemory.getUnconsolidatedCount).toHaveBeenCalled();
        expect(mockStructuredMemory.getUnconsolidatedEvents).toHaveBeenCalled();
        expect(mockOpenAI.chat.completions.create).toHaveBeenCalled();
    });

    it("should skip consolidateNow if already running", async () => {
        (cron as any).isRunning = true;
        const result = await cron.consolidateNow();
        expect(result).toBe(0);
    });

    it("should not consolidate if events below threshold", async () => {
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue([]);
        const result = await cron.consolidateNow();
        expect(result).toBe(0);
        expect(mockOpenAI.chat.completions.create).not.toHaveBeenCalled();
    });

    it("should postpone consolidation if running on battery and events < 5x threshold", async () => {
        const fs = require("node:fs");
        const readFileSyncSpy = vi.spyOn(fs, "readFileSync").mockReturnValueOnce(JSON.stringify({ is_battery: true }));
        
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`,
                timestamp: Date.now() - i * 1000,
                rawUserMsg: "test",
                rawAiReply: "test",
                phi: { facts: [] },
                psi: { sentiment: "neutral" }
            }))
        );

        const result = await cron.consolidateNow();
        expect(result).toBe(0); // Bypassed
        
        readFileSyncSpy.mockRestore();
    });

    it("should handle error in processSession gracefully without crashing the whole cron", async () => {
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`,
                timestamp: Date.now() - i * 1000,
                rawUserMsg: "test",
                rawAiReply: "test",
                phi: { facts: [] },
                psi: { sentiment: "neutral" }
            }))
        );

        (mockOpenAI.chat.completions.create as any).mockRejectedValueOnce(new Error("Network Error"));

        await expect(cron.consolidateNow()).resolves.toBe(0); // Should resolve, but consolidated 0 because it caught the error
    });

    it("should skip preflightCheck consolidation if pending is below threshold but > 0", async () => {
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(5); // 0 < 5 < 10
        await cron.preflightCheck();
        expect(mockStructuredMemory.getUnconsolidatedEvents).not.toHaveBeenCalled();
    });

    it("should catch errors thrown by consolidateNow during idle check", async () => {
        cron.start();
        
        // Mock consolidateNow to reject
        vi.spyOn(cron, "consolidateNow").mockRejectedValueOnce(new Error("Test Cron Error"));

        // Advance timers by 30 mins
        // IDLE_THRESHOLD_MS = 30 * 60 * 1000, IDLE_CHECK_INTERVAL_MS = 5 * 60 * 1000
        vi.advanceTimersByTime(30 * 60 * 1000);

        // Await next tick to allow catch block to execute
        await Promise.resolve();

        expect(cron.consolidateNow).toHaveBeenCalled();
        // The catch block should have handled it without crashing
    });

    it("should not trigger idle check if start is called twice", () => {
        const setIntervalSpy = vi.spyOn(global, 'setInterval');
        cron.start();
        cron.start();
        expect(setIntervalSpy).toHaveBeenCalledTimes(1);
        setIntervalSpy.mockRestore();
    });

    it("should catch and log error in consolidateNow (Line 222)", async () => {
        const { logger } = await import("../../src/utils/logger");
        const errorSpy = vi.spyOn(logger, "error").mockImplementation(() => {});
        mockStructuredMemory.getUnconsolidatedEvents.mockImplementationOnce(() => {
            throw new Error("L1 DB Error");
        });

        await cron.consolidateNow();
        expect(errorSpy).toHaveBeenCalledWith(expect.stringContaining("Failed at FetchAndGate"));
    });

    it("should group events into separate sessions when gap > 30 mins (Lines 247-248)", async () => {
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        
        // 15 events, first 10 at T0, next 5 at T0 + 40 mins
        const T0 = Date.now();
        const GAP = 40 * 60 * 1000;
        
        const events = Array(15).fill(0).map((_, i) => ({
            eventId: `evt_${i}`,
            timestamp: i < 10 ? T0 + i * 1000 : T0 + GAP + i * 1000,
            rawUserMsg: "test",
            rawAiReply: "test",
            phi: { facts: [] },
            psi: { sentiment: "neutral" }
        }));
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(events);

        (mockOpenAI.chat.completions.create as any).mockResolvedValue({
            choices: [{
                message: {
                    content: '{"narrative_summary":"summary", "new_user_insights":[]}'
                }
            }]
        });

        const result = await cron.consolidateNow();
        expect(result).toBe(15);
        // It should have called OpenAI 6 times (2 for macro synthesis, 4 for RAPTOR chunk summarizations)
        expect(mockOpenAI.chat.completions.create).toHaveBeenCalledTimes(6);
    });

    it("should handle Synthesis JSON parse failure (Lines 295-296)", async () => {
        const { logger } = await import("../../src/utils/logger");
        const warnSpy = vi.spyOn(logger, "warn").mockImplementation(() => {});
        
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`, timestamp: Date.now(), rawUserMsg: "test", rawAiReply: "test", phi: { facts: [] }, psi: { sentiment: "" }
            }))
        );

        (mockOpenAI.chat.completions.create as any).mockResolvedValue({
            choices: [{
                message: {
                    content: '{"invalid_json": true}' // missing narrative_summary
                }
            }]
        });

        const result = await cron.consolidateNow();
        expect(result).toBe(0); // 0 consolidated
        expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining("Synthesis JSON parse failed"));
    });

    it("should handle L2 write failure (Line 315)", async () => {
        const { logger } = await import("../../src/utils/logger");
        const warnSpy = vi.spyOn(logger, "warn").mockImplementation(() => {});
        
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`, timestamp: Date.now(), rawUserMsg: "test", rawAiReply: "test", phi: { facts: [] }, psi: { sentiment: "" }
            }))
        );

        (mockOpenAI.chat.completions.create as any).mockResolvedValue({
            choices: [{
                message: {
                    content: '{"narrative_summary":"summary", "new_user_insights":[]}'
                }
            }]
        });

        mockStructuredMemory.upsertVector = vi.fn().mockImplementationOnce(() => {
            throw new Error("L2 write error");
        });

        await cron.consolidateNow();
        expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining("L2 write failed"));
    });

    it("should handle RAPTOR Tree build failure (Line 341)", async () => {
        const { logger } = await import("../../src/utils/logger");
        const warnSpy = vi.spyOn(logger, "warn").mockImplementation(() => {});
        
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`, timestamp: Date.now(), rawUserMsg: "test", rawAiReply: "test", phi: { facts: [] }, psi: { sentiment: "" }
            }))
        );

        (mockOpenAI.chat.completions.create as any).mockResolvedValue({
            choices: [{
                message: {
                    content: '{"narrative_summary":"summary", "new_user_insights":[]}'
                }
            }]
        });

        mockBookIndex.addNode.mockImplementationOnce(() => {
            throw new Error("BookIndex Error");
        });

        await cron.consolidateNow();
        expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining("RAPTOR Tree build failed"));
    });

    it("should handle summarization chunk failure during recursiveSummarize (Lines 418, 420)", async () => {
        const { logger } = await import("../../src/utils/logger");
        const errorSpy = vi.spyOn(logger, "error").mockImplementation(() => {});
        
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`, timestamp: Date.now(), rawUserMsg: "test", rawAiReply: "test", phi: { facts: [] }, psi: { sentiment: "" }
            }))
        );

        // 1st call: processSession macro synthesis
        (mockOpenAI.chat.completions.create as any).mockResolvedValueOnce({
            choices: [{
                message: { content: '{"narrative_summary":"summary", "new_user_insights":[]}' }
            }]
        });

        // 2nd call: recursiveSummarize chunk 1 -> throw error
        (mockOpenAI.chat.completions.create as any).mockRejectedValueOnce(new Error("Summarize Error"));

        await cron.consolidateNow();
        expect(errorSpy).toHaveBeenCalledWith(expect.stringContaining("Summarization chunk failed at level"));
    });

    it("[v27] groupIntoSessions extracted to ConsolidationSteps", () => {
        // Method no longer exists on ConsolidationCron — tested in ConsolidationSteps
        expect(typeof (cron as any).groupIntoSessions).toBe('undefined');
    });

    it("should return 0 if LLM returns empty raw content (Line 290)", async () => {
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`, timestamp: Date.now(), rawUserMsg: "test", rawAiReply: "test", phi: { facts: [] }, psi: { sentiment: "" }
            }))
        );

        (mockOpenAI.chat.completions.create as any).mockResolvedValueOnce({
            choices: [{ message: { content: "" } }]
        });

        const result = await cron.consolidateNow();
        expect(result).toBe(0);
    });

    it("should handle missing new_user_insights entirely (Line 319)", async () => {
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`, timestamp: Date.now(), rawUserMsg: "test", rawAiReply: "test", phi: { facts: [] }, psi: { sentiment: "" }
            }))
        );

        (mockOpenAI.chat.completions.create as any).mockResolvedValue({
            choices: [{
                message: { content: '{"narrative_summary":"summary"}' } // No new_user_insights
            }]
        });

        const result = await cron.consolidateNow();
        expect(result).toBe(15);
        expect(mockStructuredMemory.setFact).not.toHaveBeenCalled();
    });

    it("should skip insights with missing key or value (Line 321)", async () => {
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`, timestamp: Date.now(), rawUserMsg: "test", rawAiReply: "test", phi: { facts: [] }, psi: { sentiment: "" }
            }))
        );

        (mockOpenAI.chat.completions.create as any).mockResolvedValue({
            choices: [{
                message: { content: '{"narrative_summary":"summary", "new_user_insights":[{"key":"only_key"}]}' } // Missing value
            }]
        });

        const result = await cron.consolidateNow();
        expect(result).toBe(15);
        expect(mockStructuredMemory.setFact).not.toHaveBeenCalled();
    });

    it("should handle empty summary during RAPTOR chunking (Lines 395, 425)", async () => {
        mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
        mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(
            Array(15).fill(0).map((_, i) => ({
                eventId: `evt_${i}`, timestamp: Date.now(), rawUserMsg: "test", rawAiReply: "test", phi: { facts: [] }, psi: { sentiment: "" }
            }))
        );

        // 1st call: processSession macro synthesis
        (mockOpenAI.chat.completions.create as any).mockResolvedValueOnce({
            choices: [{
                message: { content: '{"narrative_summary":"summary", "new_user_insights":[]}' }
            }]
        });

        // Next calls: recursiveSummarize chunk -> return empty summary
        (mockOpenAI.chat.completions.create as any).mockResolvedValue({
            choices: [{
                message: { content: "" }
            }]
        });

        const result = await cron.consolidateNow();
        expect(result).toBe(15);
        // Because it returned empty, nextLevelNodes will be empty, skipping recursion
        // This covers the false branch of `if (nextLevelNodes.length > 0 ...)`
    });

    // ===========================
    // [UHM] Passive Affective Trigger Tests
    // ===========================
    describe("[UHM] Passive Affective Triggers", () => {
        it("recordActivity('TOPIC_SHIFT') should increment topicShiftCount", async () => {
            cron.recordActivity('TOPIC_SHIFT');
            const state = await cron.getAffectiveState();
            expect(state.topicShiftCount).toBe(1);
        });

        it("recordActivity('NEW_TURN') should NOT increment topicShiftCount", async () => {
            cron.recordActivity('NEW_TURN');
            const state = await cron.getAffectiveState();
            expect(state.topicShiftCount).toBe(0);
        });

        it("recordActivity should always schedule debounced check", () => {
            cron.recordActivity('NEW_TURN');
            expect((cron as any).affectiveDebounceTimer).not.toBeNull();
        });

        it("shouldTriggerAffective returns true when topicShiftCount >= 3", async () => {
            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');
            expect(await cron.shouldTriggerAffective()).toBe(true);
        });

        it("shouldTriggerAffective returns false when topicShiftCount < 3 and events < 20", async () => {
            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');
            expect(await cron.shouldTriggerAffective()).toBe(false);
        });

        it("shouldTriggerAffective returns true when unconsolidatedCount >= 20", async () => {
            mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(20);
            expect(await cron.shouldTriggerAffective()).toBe(true);
        });

        it("shouldTriggerAffective returns false when unconsolidatedCount < 20 and no topic shifts", async () => {
            mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(5);
            expect(await cron.shouldTriggerAffective()).toBe(false);
        });

        it("debounced check should reset timer on subsequent activity", () => {
            cron.recordActivity('TOPIC_SHIFT');
            const timer1 = (cron as any).affectiveDebounceTimer;
            vi.advanceTimersByTime(5000);
            cron.recordActivity('TOPIC_SHIFT');
            const timer2 = (cron as any).affectiveDebounceTimer;
            expect(timer1).not.toBe(timer2);
        });

        it("VRAM guard: skipped if isRunning", async () => {
            const consoleSpy = vi.spyOn(cron, "consolidateNow").mockResolvedValue(0);
            (cron as any).isRunning = true;

            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');

            await vi.advanceTimersByTimeAsync(16_000);
            expect(consoleSpy).not.toHaveBeenCalled();
            consoleSpy.mockRestore();
        });

        it("VRAM guard: skipped if AgentLoop is NOT IDLE", async () => {
            const consoleSpy = vi.spyOn(cron, "consolidateNow").mockResolvedValue(0);
            cron.setAgentLoopStateGetter(() => 'THINKING');

            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');

            await vi.advanceTimersByTimeAsync(16_000);
            expect(consoleSpy).not.toHaveBeenCalled();
            consoleSpy.mockRestore();
        });

        it("fires consolidateNow when AgentLoop is IDLE and threshold met", async () => {
            const consoleSpy = vi.spyOn(cron, "consolidateNow").mockResolvedValue(0);
            cron.setAgentLoopStateGetter(() => 'IDLE');

            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');

            await vi.advanceTimersByTimeAsync(16_000);
            expect(consoleSpy).toHaveBeenCalled();
            consoleSpy.mockRestore();
        });

        it("fires even without agentLoopStateGetter (backward compat)", async () => {
            const consoleSpy = vi.spyOn(cron, "consolidateNow").mockResolvedValue(0);

            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');

            await vi.advanceTimersByTimeAsync(16_000);
            expect(consoleSpy).toHaveBeenCalled();
            consoleSpy.mockRestore();
        });

        it("topicShiftCount resets after trigger fires", async () => {
            const consoleSpy = vi.spyOn(cron, "consolidateNow").mockResolvedValue(0);

            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');

            await vi.advanceTimersByTimeAsync(16_000);
            const state = await cron.getAffectiveState();
            expect(state.topicShiftCount).toBe(0);
            consoleSpy.mockRestore();
        });

        it("dispose should clear affective timer and topicShiftCount", () => {
            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');
            cron.dispose();
            expect((cron as any).affectiveDebounceTimer).toBeNull();
            expect((cron as any).topicShiftCount).toBe(0);
        });

        it("getAffectiveState should return correct values", async () => {
            mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(12);
            cron.recordActivity('TOPIC_SHIFT');
            cron.recordActivity('TOPIC_SHIFT');
            const state = await cron.getAffectiveState();
            expect(state.topicShiftCount).toBe(2);
            expect(state.unconsolidatedCount).toBe(12);
        });
    });
});
