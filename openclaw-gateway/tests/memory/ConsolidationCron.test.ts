/**
 * ConsolidationCron.test.ts — Sleep-time memory consolidation tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { ConsolidationCron } from "../../src/memory/ConsolidationCron";

describe("ConsolidationCron", () => {
    let cron: ConsolidationCron;
    let mockStructuredMemory: any;
    let mockLanceMemory: any;
    let mockBookIndex: any;
    let mockAI: any;

    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();

        mockStructuredMemory = {
            getUnconsolidatedCount: vi.fn().mockReturnValue(0),
            getUnconsolidatedEvents: vi.fn().mockReturnValue([]),
            setFact: vi.fn(),
            markConsolidated: vi.fn(),
            gcOldEvents: vi.fn(),
        };

        mockLanceMemory = {
            addMemory: vi.fn().mockResolvedValue(undefined),
            addSemanticAnchor: vi.fn().mockResolvedValue(undefined),
        };

        mockAI = {
            chat: {
                completions: {
                    create: vi.fn().mockResolvedValue({
                        choices: [{
                            message: {
                                content: JSON.stringify({
                                    narrative_summary: "Test session summary",
                                    new_user_insights: [
                                        { key: "test_key", value: "test_value", category: "Test" }
                                    ]
                                }),
                            },
                        }],
                    }),
                },
            },
        };

        mockBookIndex = {
            addSummaryNode: vi.fn().mockReturnValue("mock-node-id"),
            addChild: vi.fn(),
            getRoots: vi.fn().mockReturnValue([])
        };

        cron = new ConsolidationCron(mockStructuredMemory, mockLanceMemory, mockBookIndex, mockAI);
    });

    afterEach(() => {
        cron.dispose();
        vi.useRealTimers();
    });

    it("should start and stop idle timer", () => {
        cron.start();
        expect(cron["idleCheckTimer"]).not.toBeNull();
        cron.stop();
        expect(cron["idleCheckTimer"]).toBeNull();
    });

    it("should update lastInteractionTime on touch()", () => {
        const initialTime = cron["lastInteractionTime"];
        vi.advanceTimersByTime(1000);
        cron.touch();
        expect(cron["lastInteractionTime"]).toBeGreaterThan(initialTime);
    });

    describe("preflightCheck()", () => {
        it("should trigger consolidation if pending events >= 10", async () => {
            mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
            // Mock getUnconsolidatedEvents to return an array of 15 items
            const mockEvents = Array.from({ length: 15 }, (_, i) => ({
                eventId: `evt-${i}`,
                timestamp: Date.now(),
                phi: { facts: [] },
                psi: { sentiment: "ok" },
                rawUserMsg: "test",
                rawAiReply: "test"
            }));
            mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(mockEvents);

            await cron.preflightCheck();

            expect(mockStructuredMemory.getUnconsolidatedEvents).toHaveBeenCalled();
            expect(mockAI.chat.completions.create).toHaveBeenCalledTimes(1);
        });

        it("should skip if pending events < 10", async () => {
            mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(5);
            await cron.preflightCheck();
            expect(mockStructuredMemory.getUnconsolidatedEvents).not.toHaveBeenCalled();
        });
    });

    describe("consolidateNow()", () => {
        it("should skip if already running", async () => {
            cron["isRunning"] = true;
            const count = await cron.consolidateNow();
            expect(count).toBe(0);
            expect(mockStructuredMemory.getUnconsolidatedEvents).not.toHaveBeenCalled();
        });

        it("should skip if events < MIN_EVENTS_THRESHOLD", async () => {
            mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue([
                { eventId: "1", timestamp: Date.now() }
            ]);
            const count = await cron.consolidateNow();
            expect(count).toBe(0);
            expect(mockAI.chat.completions.create).not.toHaveBeenCalled();
        });

        it("should group events into sessions correctly (gap > 30min)", async () => {
            const now = Date.now();
            const events = [
                // Session 1
                { eventId: "1", timestamp: now, phi: { facts: [] }, psi: { sentiment: "" }, rawUserMsg: "", rawAiReply: "" },
                { eventId: "2", timestamp: now + 60000, phi: { facts: [] }, psi: { sentiment: "" }, rawUserMsg: "", rawAiReply: "" }, // 1 min later
                // Session 2
                { eventId: "3", timestamp: now + 40 * 60 * 1000, phi: { facts: [] }, psi: { sentiment: "" }, rawUserMsg: "", rawAiReply: "" }, // 40 min later
            ];
            
            // Add extra events to pass MIN_EVENTS_THRESHOLD (10)
            for(let i=4; i<=15; i++) {
                 events.push({ eventId: `${i}`, timestamp: now + 41 * 60 * 1000, phi: { facts: [] }, psi: { sentiment: "" }, rawUserMsg: "", rawAiReply: "" });
            }

            mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(events);

            await cron.consolidateNow();

            // Should process 2 sessions
            expect(mockAI.chat.completions.create).toHaveBeenCalledTimes(2);
            expect(mockLanceMemory.addMemory).toHaveBeenCalledTimes(2);
        });

        it("should process session and update L2 and L3", async () => {
            const events = Array.from({ length: 10 }, (_, i) => ({
                eventId: `evt-${i}`,
                timestamp: Date.now(),
                phi: { facts: ["fact1"] },
                psi: { sentiment: "happy" },
                rawUserMsg: "user test",
                rawAiReply: "ai test"
            }));
            mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(events);

            const count = await cron.consolidateNow();

            expect(count).toBe(10);
            expect(mockLanceMemory.addMemory).toHaveBeenCalledWith("AXIOM", "Test session summary", expect.any(String));
            expect(mockStructuredMemory.setFact).toHaveBeenCalledWith("test_key", "test_value", expect.objectContaining({ category: "Test" }));
            expect(mockStructuredMemory.markConsolidated).toHaveBeenCalledWith(events.map(e => e.eventId));
            expect(mockStructuredMemory.gcOldEvents).toHaveBeenCalled();
        });

        it("should handle LLM invalid JSON gracefully", async () => {
            const events = Array.from({ length: 10 }, (_, i) => ({
                eventId: `evt-${i}`,
                timestamp: Date.now(),
                phi: { facts: [] },
                psi: { sentiment: "" },
                rawUserMsg: "",
                rawAiReply: ""
            }));
            mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(events);
            
            mockAI.chat.completions.create.mockResolvedValue({
                choices: [{ message: { content: "invalid json" } }],
            });

            const count = await cron.consolidateNow();
            // Should still return 0 because processing failed, but not crash
            expect(count).toBe(0);
            expect(mockLanceMemory.addMemory).not.toHaveBeenCalled();
            expect(mockStructuredMemory.markConsolidated).not.toHaveBeenCalled();
        });
        
        it("should auto-trigger when idle threshold is reached", async () => {
            mockStructuredMemory.getUnconsolidatedCount.mockReturnValue(15);
            const events = Array.from({ length: 15 }, (_, i) => ({
                eventId: `evt-${i}`,
                timestamp: Date.now(),
                phi: { facts: [] },
                psi: { sentiment: "" },
                rawUserMsg: "",
                rawAiReply: ""
            }));
            mockStructuredMemory.getUnconsolidatedEvents.mockReturnValue(events);
            
            cron.start();
            
            // Advance by 30 minutes + 5 minutes
            vi.advanceTimersByTime(35 * 60 * 1000);
            await vi.advanceTimersByTimeAsync(100);
            
            expect(mockAI.chat.completions.create).toHaveBeenCalled();
        });
    });
});
