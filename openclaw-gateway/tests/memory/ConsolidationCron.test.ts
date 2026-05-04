import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ConsolidationCron } from "../../src/memory/ConsolidationCron";
import { StructuredMemory } from "../../src/memory/StructuredMemory";
import { LanceMemoryManager } from "../../src/memory/LanceMemory";
import { BookIndex } from "../../src/memory/BookIndex";
import OpenAI from "openai";

vi.mock("../../src/memory/StructuredMemory");
vi.mock("../../src/memory/LanceMemory");
vi.mock("../../src/memory/BookIndex");
vi.mock("openai");

describe("ConsolidationCron", () => {
    let cron: ConsolidationCron;
    let mockStructuredMemory: vi.Mocked<StructuredMemory>;
    let mockLanceMemory: vi.Mocked<LanceMemoryManager>;
    let mockBookIndex: vi.Mocked<BookIndex>;
    let mockOpenAI: vi.Mocked<OpenAI>;

    beforeEach(() => {
        vi.useFakeTimers();
        
        mockStructuredMemory = new StructuredMemory("test-agent") as any;
        mockLanceMemory = new LanceMemoryManager("test-collection") as any;
        mockBookIndex = new BookIndex() as any;
        mockOpenAI = new OpenAI({ apiKey: "test" }) as any;
        
        mockOpenAI.chat = {
            completions: {
                create: vi.fn()
            }
        } as any;

        cron = new ConsolidationCron(
            mockStructuredMemory,
            mockLanceMemory,
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
        cron.start();
        expect((cron as any).idleCheckTimer).not.toBeNull();
        
        cron.stop();
        expect((cron as any).idleCheckTimer).toBeNull();
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
        cron.start();
        const firstTimer = (cron as any).idleCheckTimer;
        cron.start();
        const secondTimer = (cron as any).idleCheckTimer;
        expect(firstTimer).toBe(secondTimer);
    });

    it("should catch and log error in consolidateNow (Line 222)", async () => {
        const { logger } = await import("../../src/utils/logger");
        const errorSpy = vi.spyOn(logger, "error").mockImplementation(() => {});
        mockStructuredMemory.getUnconsolidatedEvents.mockImplementationOnce(() => {
            throw new Error("L1 DB Error");
        });

        await cron.consolidateNow();
        expect(errorSpy).toHaveBeenCalledWith(expect.stringContaining("Consolidation failed"));
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

        mockLanceMemory.addSemanticAnchor.mockRejectedValueOnce(new Error("L2 write error"));

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

    it("should cover unreachable branch in groupIntoSessions (Line 234)", () => {
        const result = (cron as any).groupIntoSessions([]);
        expect(result).toEqual([]);
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
});
