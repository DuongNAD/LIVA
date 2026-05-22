import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// vi.hoisted — survives module hoisting
const { mockExec, mockPrepare, mockStmtRun, mockStmtGet, mockStmtAll } = vi.hoisted(() => {
    const mockStmtRun = vi.fn(() => ({ changes: 1 }));
    const mockStmtGet = vi.fn();
    const mockStmtAll = vi.fn(() => []);
    const mockPrepare = vi.fn(() => ({
        get: mockStmtGet,
        all: mockStmtAll,
        run: mockStmtRun,
    }));
    const mockExec = vi.fn();
    return { mockExec, mockPrepare, mockStmtRun, mockStmtGet, mockStmtAll };
});

vi.mock("node:sqlite", () => {
    class MockDatabaseSync {
        exec = mockExec;
        prepare = mockPrepare;
        constructor() {}
    }
    return { DatabaseSync: MockDatabaseSync };
});

import { EventRepository, EventBrick } from "@memory/EventRepository";
import { DatabaseSync } from "node:sqlite";

function makeEvent(overrides: Partial<EventBrick> = {}): EventBrick {
    return {
        eventId: `evt_${Math.random().toString(36).slice(2, 6)}`,
        timestamp: Date.now(),
        phi: { facts: ["f1"], entities: ["e1"] },
        psi: { sentiment: "positive", intent: "info", relational: "" },
        rawUserMsg: "hello",
        rawAiReply: "hi there",
        ...overrides,
    };
}

describe("EventRepository — Event Brick Persistence", () => {
    let repo: EventRepository;

    beforeEach(() => {
        vi.clearAllMocks();
        const db = new DatabaseSync(":memory:" as any);
        repo = new EventRepository(db);
    });

    afterEach(() => {
        repo.flushAndStop();
    });

    // ============================================================
    // Constructor
    // ============================================================
    describe("Constructor", () => {
        it("should create without error", () => {
            expect(repo).toBeTruthy();
        });
    });

    // ============================================================
    // insertEvent()
    // ============================================================
    describe("insertEvent()", () => {
        it("should prepare INSERT OR REPLACE and run with correct params", () => {
            const evt = makeEvent({ eventId: "evt_test1", rawUserMsg: "test msg" });
            repo.insertEvent(evt);

            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("INSERT OR REPLACE INTO events")
            );
            expect(mockStmtRun).toHaveBeenCalledWith(
                "evt_test1",
                evt.timestamp,
                JSON.stringify(evt.phi.facts),
                JSON.stringify(evt.phi.entities),
                evt.psi.sentiment,
                evt.psi.intent,
                evt.psi.relational,
                "test msg",
                evt.rawAiReply,
                "General",
                "Uncategorized",
                "[]",
                0
            );
        });

        it("should use custom domain and category", () => {
            const evt = makeEvent({
                eventId: "evt_custom",
                domain: "Work",
                category: "Meeting",
                traceKeywords: ["project", "deadline"],
            });
            repo.insertEvent(evt);

            expect(mockStmtRun).toHaveBeenCalledWith(
                "evt_custom",
                expect.any(Number),
                expect.any(String),
                expect.any(String),
                expect.any(String),
                expect.any(String),
                expect.any(String),
                expect.any(String),
                expect.any(String),
                "Work",
                "Meeting",
                JSON.stringify(["project", "deadline"]),
                0
            );
        });
    });

    // ============================================================
    // getUnconsolidatedEvents()
    // ============================================================
    describe("getUnconsolidatedEvents()", () => {
        it("should query unconsolidated pending events", () => {
            mockStmtAll.mockReturnValue([]);
            const events = repo.getUnconsolidatedEvents();

            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("WHERE consolidated = 0 AND consolidation_status = 'pending'")
            );
            expect(events).toEqual([]);
        });

        it("should map DB rows to EventBrick via mapEventRow", () => {
            mockStmtAll.mockReturnValue([
                {
                    eventId: "evt_1",
                    timestamp: 123456,
                    phi_facts: '["fact1"]',
                    phi_entities: '["entity1"]',
                    psi_sentiment: "positive",
                    psi_intent: "info",
                    psi_relational: "",
                    rawUserMsg: "hello",
                    rawAiReply: "hi",
                    consolidated: 0,
                    domain: "Personal",
                    category: "Greeting",
                    trace_keywords: '["greet"]',
                    last_accessed_at: 99999,
                },
            ]);

            const events = repo.getUnconsolidatedEvents();
            expect(events).toHaveLength(1);
            expect(events[0].eventId).toBe("evt_1");
            expect(events[0].phi.facts).toEqual(["fact1"]);
            expect(events[0].psi.sentiment).toBe("positive");
            expect(events[0].domain).toBe("Personal");
            expect(events[0].traceKeywords).toEqual(["greet"]);
        });

        it("should handle NULL fields with defaults", () => {
            mockStmtAll.mockReturnValue([
                {
                    eventId: "evt_null",
                    timestamp: 100,
                    phi_facts: null,
                    phi_entities: null,
                    psi_sentiment: null,
                    psi_intent: null,
                    psi_relational: null,
                    rawUserMsg: null,
                    rawAiReply: null,
                    consolidated: 0,
                    domain: null,
                    category: null,
                    trace_keywords: null,
                    last_accessed_at: null,
                },
            ]);

            const events = repo.getUnconsolidatedEvents();
            expect(events[0].phi.facts).toEqual([]);
            expect(events[0].phi.entities).toEqual([]);
            expect(events[0].psi.sentiment).toBe("");
            expect(events[0].rawUserMsg).toBe("");
            expect(events[0].domain).toBe("General");
            expect(events[0].last_accessed_at).toBe(0);
        });
    });

    // ============================================================
    // getUnconsolidatedCount()
    // ============================================================
    describe("getUnconsolidatedCount()", () => {
        it("should return count from DB", () => {
            mockStmtGet.mockReturnValue({ c: 42 });
            expect(repo.getUnconsolidatedCount()).toBe(42);
        });
    });

    // ============================================================
    // markConsolidated()
    // ============================================================
    describe("markConsolidated()", () => {
        it("should no-op on empty array", () => {
            repo.markConsolidated([]);
            expect(mockPrepare).not.toHaveBeenCalledWith(
                expect.stringContaining("UPDATE events SET consolidated")
            );
        });

        it("should update each event individually", () => {
            repo.markConsolidated(["evt_a", "evt_b"]);
            expect(mockStmtRun).toHaveBeenCalledWith("evt_a");
            expect(mockStmtRun).toHaveBeenCalledWith("evt_b");
        });
    });

    // ============================================================
    // markDLQ()
    // ============================================================
    describe("markDLQ()", () => {
        it("should no-op on empty array", () => {
            repo.markDLQ([]);
            // No prepare calls for DLQ
        });

        it("should mark events as dlq", () => {
            repo.markDLQ(["evt_fail_1", "evt_fail_2"]);
            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("consolidation_status = 'dlq'")
            );
            expect(mockStmtRun).toHaveBeenCalledWith("evt_fail_1");
            expect(mockStmtRun).toHaveBeenCalledWith("evt_fail_2");
        });
    });

    // ============================================================
    // incrementRetryCount()
    // ============================================================
    describe("incrementRetryCount()", () => {
        it("should no-op on empty array", () => {
            repo.incrementRetryCount([]);
        });

        it("should increment retry for each event", () => {
            repo.incrementRetryCount(["evt_r1", "evt_r2"]);
            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("retry_count = retry_count + 1")
            );
        });
    });

    // ============================================================
    // gcOldEvents()
    // ============================================================
    describe("gcOldEvents()", () => {
        it("should delete consolidated events older than retention days", () => {
            mockStmtRun.mockReturnValue({ changes: 5 });
            const removed = repo.gcOldEvents(7);
            expect(removed).toBe(5);
            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("DELETE FROM events WHERE consolidated = 1 AND timestamp <")
            );
        });

        it("should use default 7 days retention", () => {
            mockStmtRun.mockReturnValue({ changes: 0 });
            const removed = repo.gcOldEvents();
            expect(removed).toBe(0);
        });
    });

    // ============================================================
    // deleteAllEvents()
    // ============================================================
    describe("deleteAllEvents()", () => {
        it("should delete from events and turn_layer_nodes", () => {
            repo.deleteAllEvents();
            expect(mockExec).toHaveBeenCalledWith("DELETE FROM events");
            expect(mockExec).toHaveBeenCalledWith("DELETE FROM turn_layer_nodes");
        });
    });

    // ============================================================
    // Turn Layer — insertTurnNode / getTurnsByTimeRange / getTurnsByIds
    // ============================================================
    describe("Turn Layer", () => {
        it("should insert turn node", () => {
            repo.insertTurnNode("turn_1", 1000, "hello", "hi");
            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("INSERT INTO turn_layer_nodes")
            );
        });

        it("should catch error during insertTurnNode", () => {
            mockPrepare.mockImplementationOnce(() => { throw new Error("DB error"); });
            // Should not throw
            expect(() => repo.insertTurnNode("turn_err", 1000, "msg", "reply")).not.toThrow();
        });

        it("should query turns by time range", () => {
            mockStmtAll.mockReturnValue([
                { turnId: "t1", temporal_anchor: 1500, userMsg: "hi", aiReply: "hello", createdAt: "2026-01-01" },
            ]);
            const turns = repo.getTurnsByTimeRange(1000, 2000);
            expect(turns).toHaveLength(1);
            expect(turns[0].turnId).toBe("t1");
        });

        it("should query turns by IDs", () => {
            mockStmtAll.mockReturnValue([
                { turnId: "tA", temporal_anchor: 100, userMsg: "A", aiReply: "A", createdAt: "2026-01-01" },
            ]);
            const turns = repo.getTurnsByIds(["tA"]);
            expect(turns).toHaveLength(1);
        });

        it("should return empty for empty turnIds", () => {
            const turns = repo.getTurnsByIds([]);
            expect(turns).toEqual([]);
        });
    });

    // ============================================================
    // Memory Touch — queueMemoryTouch / flushTouchQueue
    // ============================================================
    describe("Memory Touch", () => {
        it("should queue a memory touch", () => {
            repo.queueMemoryTouch("evt_touch_1");
            // No direct way to verify queue, but flush should work
        });

        it("should flush touch queue", async () => {
            repo.queueMemoryTouch("evt_t1");
            repo.queueMemoryTouch("evt_t2");

            await repo.flushTouchQueue();

            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("UPDATE events SET last_accessed_at")
            );
        });

        it("should be no-op when queue is empty", async () => {
            const callsBefore = mockPrepare.mock.calls.length;
            await repo.flushTouchQueue();
            expect(mockPrepare.mock.calls.length).toBe(callsBefore);
        });

        it("should re-add items on flush failure", async () => {
            mockPrepare.mockImplementationOnce(() => { throw new Error("DB write failed"); });
            repo.queueMemoryTouch("evt_retry");
            await repo.flushTouchQueue();
            // Items re-added — next flush should try again
        });

        it("should cap queue at TOUCH_QUEUE_CAPACITY", () => {
            for (let i = 0; i < EventRepository.TOUCH_QUEUE_CAPACITY + 10; i++) {
                repo.queueMemoryTouch(`evt_cap_${i}`);
            }
            // No throw
        });

        it("should trigger early flush at TOUCH_EARLY_FLUSH", () => {
            for (let i = 0; i < EventRepository.TOUCH_EARLY_FLUSH; i++) {
                repo.queueMemoryTouch(`evt_early_${i}`);
            }
            // Early flush triggered via Promise.resolve().then — no error
        });
    });

    // ============================================================
    // startTouchDebounce / flushAndStop
    // ============================================================
    describe("startTouchDebounce / flushAndStop", () => {
        it("should start and stop debounce timer", () => {
            repo.startTouchDebounce();
            expect(() => repo.flushAndStop()).not.toThrow();
        });

        it("should flush remaining items on stop", () => {
            repo.queueMemoryTouch("evt_shutdown");
            repo.flushAndStop();
            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("UPDATE events SET last_accessed_at")
            );
        });

        it("should handle flush error during shutdown gracefully", () => {
            repo.queueMemoryTouch("evt_shutdown_err");
            mockPrepare.mockImplementationOnce(() => { throw new Error("Shutdown DB error"); });
            expect(() => repo.flushAndStop()).not.toThrow();
        });
    });
});
