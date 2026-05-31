import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../src/utils/JsonExtractor", () => ({
    safeExtractJSON: vi.fn(),
}));

vi.mock("node:fs", () => ({
    promises: { readFile: vi.fn() },
}));

import {
    FetchAndGateStep,
    GCOldEventsStep,
    WALCheckpointStep,
    ProcessDLQStep,
    EbbinghausDecayStep,
    SnapshotBackupStep,
    createConsolidationSteps,
    type StepDependencies,
} from "../../src/memory/ConsolidationSteps";
import type { ConsolidationContext } from "../../src/memory/ConsolidationPipeline";
import { promises as fsp } from "node:fs";

// ─── Mock Dependencies Factory ───
function createMockDeps(): StepDependencies {
    return {
        structuredMemory: {
            getUnconsolidatedEvents: vi.fn().mockResolvedValue([]),
            markConsolidated: vi.fn().mockResolvedValue(undefined),
            upsertVector: vi.fn().mockResolvedValue(undefined),
            setFact: vi.fn().mockResolvedValue(undefined),
            getDb: vi.fn().mockReturnValue({
                prepare: vi.fn().mockReturnValue({ all: vi.fn().mockReturnValue([]), run: vi.fn() }),
                exec: vi.fn(),
            }),
            gcOldEvents: vi.fn().mockResolvedValue(undefined),
            processDLQ: vi.fn().mockResolvedValue(undefined),
            applyMemoryDecay: vi.fn().mockResolvedValue({ decayed: 0, archived: 0 }),
            createSnapshotBackup: vi.fn().mockResolvedValue(undefined),
            graph: {
                upsertNode: vi.fn().mockResolvedValue(undefined),
                upsertEdge: vi.fn().mockResolvedValue(undefined),
                buildCommunitySummaries: vi.fn().mockResolvedValue(undefined),
            },
        } as any,
        embeddingService: {
            embed: vi.fn().mockResolvedValue(new Float32Array(128)),
        } as any,
        aiClient: {
            chat: {
                completions: {
                    create: vi.fn().mockResolvedValue({
                        choices: [{ message: { content: "{}" } }],
                    }),
                },
            },
        } as any,
        bookIndex: {
            addNode: vi.fn(),
            addEdge: vi.fn(),
        } as any,
        contradictionResolver: {
            resolve: vi.fn().mockResolvedValue(undefined),
        } as any,
        reconsolidationEngine: null,
        synthesisPrompt: "Test prompt",
    };
}

function createCtx(): ConsolidationContext {
    return {
        startedAt: Date.now(),
        totalConsolidated: 0,
        sharedState: {},
    };
}

// ────────────────────────────────────────────
// Step 1: FetchAndGateStep
// ────────────────────────────────────────────
describe("FetchAndGateStep", () => {
    let deps: StepDependencies;
    let ctx: ConsolidationContext;

    beforeEach(() => {
        vi.clearAllMocks();
        deps = createMockDeps();
        ctx = createCtx();
    });

    it("should skip when events count is below threshold", async () => {
        (deps.structuredMemory.getUnconsolidatedEvents as any).mockResolvedValue(
            Array.from({ length: 5 }, (_, i) => ({ eventId: `e${i}`, timestamp: Date.now() + i * 1000 }))
        );

        const step = new FetchAndGateStep(deps);
        await step.execute(ctx);

        expect(ctx.sharedState.events).toEqual([]);
        expect(ctx.sharedState.sessions).toEqual([]);
    });

    it("should proceed when events count meets threshold", async () => {
        const events = Array.from({ length: 12 }, (_, i) => ({
            eventId: `e${i}`, timestamp: Date.now() + i * 1000,
        }));
        (deps.structuredMemory.getUnconsolidatedEvents as any).mockResolvedValue(events);

        const step = new FetchAndGateStep(deps);
        await step.execute(ctx);

        expect(ctx.sharedState.events).toHaveLength(12);
        expect(ctx.sharedState.sessions.length).toBeGreaterThan(0);
    });

    it("should force execute even with 1 event when force=true", async () => {
        const events = [{ eventId: "e1", timestamp: Date.now() }];
        (deps.structuredMemory.getUnconsolidatedEvents as any).mockResolvedValue(events);

        const step = new FetchAndGateStep(deps, true);
        await step.execute(ctx);

        expect(ctx.sharedState.events).toHaveLength(1);
    });

    it("should increase threshold 5x when running on battery", async () => {
        // Mock hardware_state.json to indicate battery mode
        vi.mocked(fsp.readFile).mockResolvedValue(JSON.stringify({ is_battery: true }));

        // 10 events = passes normal threshold (10) but fails battery threshold (50)
        const events = Array.from({ length: 10 }, (_, i) => ({
            eventId: `e${i}`, timestamp: Date.now() + i * 1000,
        }));
        (deps.structuredMemory.getUnconsolidatedEvents as any).mockResolvedValue(events);

        const step = new FetchAndGateStep(deps);
        await step.execute(ctx);

        // Should skip because 10 < 50 (battery threshold)
        expect(ctx.sharedState.events).toEqual([]);
    });

    it("should group events into sessions separated by 30min gap", async () => {
        const baseTime = Date.now();
        const SESSION_GAP = 31 * 60 * 1000; // 31 minutes
        const events = [
            { eventId: "s1e1", timestamp: baseTime },
            { eventId: "s1e2", timestamp: baseTime + 5000 },
            { eventId: "s2e1", timestamp: baseTime + SESSION_GAP },
            { eventId: "s2e2", timestamp: baseTime + SESSION_GAP + 5000 },
        ];
        (deps.structuredMemory.getUnconsolidatedEvents as any).mockResolvedValue(events);

        const step = new FetchAndGateStep(deps, true);
        await step.execute(ctx);

        expect(ctx.sharedState.sessions).toHaveLength(2);
        expect(ctx.sharedState.sessions[0].events).toHaveLength(2);
        expect(ctx.sharedState.sessions[1].events).toHaveLength(2);
    });
});

// ────────────────────────────────────────────
// Step 3: GCOldEventsStep
// ────────────────────────────────────────────
describe("GCOldEventsStep", () => {
    it("should call gcOldEvents with 7 days retention", async () => {
        const deps = createMockDeps();
        const ctx = createCtx();
        ctx.sharedState.events = [{ eventId: "e1" }];

        const step = new GCOldEventsStep(deps);
        await step.execute(ctx);

        expect(deps.structuredMemory.gcOldEvents).toHaveBeenCalledWith(7);
    });

    it("should skip when no events in shared state", async () => {
        const deps = createMockDeps();
        const ctx = createCtx();
        ctx.sharedState.events = [];

        const step = new GCOldEventsStep(deps);
        await step.execute(ctx);

        expect(deps.structuredMemory.gcOldEvents).not.toHaveBeenCalled();
    });
});

// ────────────────────────────────────────────
// Step 5: WALCheckpointStep
// ────────────────────────────────────────────
describe("WALCheckpointStep", () => {
    it("should execute PRAGMA wal_checkpoint(PASSIVE)", async () => {
        const deps = createMockDeps();
        const ctx = createCtx();
        ctx.sharedState.events = [{ eventId: "e1" }];

        const step = new WALCheckpointStep(deps);
        await step.execute(ctx);

        expect(deps.structuredMemory.getDb().exec).toHaveBeenCalledWith("PRAGMA wal_checkpoint(PASSIVE)");
    });

    it("should not throw if exec fails", async () => {
        const deps = createMockDeps();
        const ctx = createCtx();
        ctx.sharedState.events = [{ eventId: "e1" }];
        (deps.structuredMemory.getDb().exec as any).mockImplementation(() => { throw new Error("disk full"); });

        const step = new WALCheckpointStep(deps);
        await expect(step.execute(ctx)).resolves.not.toThrow();
    });
});

// ────────────────────────────────────────────
// Step 6: ProcessDLQStep
// ────────────────────────────────────────────
describe("ProcessDLQStep", () => {
    it("should call processDLQ", async () => {
        const deps = createMockDeps();
        const ctx = createCtx();
        ctx.sharedState.events = [{ eventId: "e1" }];

        const step = new ProcessDLQStep(deps);
        await step.execute(ctx);

        expect(deps.structuredMemory.processDLQ).toHaveBeenCalled();
    });

    it("should skip when no events", async () => {
        const deps = createMockDeps();
        const ctx = createCtx();
        ctx.sharedState.events = [];

        const step = new ProcessDLQStep(deps);
        await step.execute(ctx);

        expect(deps.structuredMemory.processDLQ).not.toHaveBeenCalled();
    });
});

// ────────────────────────────────────────────
// Step 7: EbbinghausDecayStep
// ────────────────────────────────────────────
describe("EbbinghausDecayStep", () => {
    it("should call applyMemoryDecay", async () => {
        const deps = createMockDeps();
        const ctx = createCtx();
        ctx.sharedState.events = [{ eventId: "e1" }];

        const step = new EbbinghausDecayStep(deps);
        await step.execute(ctx);

        expect(deps.structuredMemory.applyMemoryDecay).toHaveBeenCalled();
    });

    it("should not throw if decay fails (non-critical)", async () => {
        const deps = createMockDeps();
        const ctx = createCtx();
        ctx.sharedState.events = [{ eventId: "e1" }];
        (deps.structuredMemory.applyMemoryDecay as any).mockRejectedValue(new Error("DB locked"));

        const step = new EbbinghausDecayStep(deps);
        await expect(step.execute(ctx)).resolves.not.toThrow();
    });
});

// ────────────────────────────────────────────
// Step 9: SnapshotBackupStep
// ────────────────────────────────────────────
describe("SnapshotBackupStep", () => {
    it("should call createSnapshotBackup", async () => {
        const deps = createMockDeps();
        const ctx = createCtx();
        ctx.sharedState.events = [{ eventId: "e1" }];

        const step = new SnapshotBackupStep(deps);
        await step.execute(ctx);

        expect(deps.structuredMemory.createSnapshotBackup).toHaveBeenCalled();
    });

    it("should not throw if backup fails", async () => {
        const deps = createMockDeps();
        const ctx = createCtx();
        ctx.sharedState.events = [{ eventId: "e1" }];
        (deps.structuredMemory.createSnapshotBackup as any).mockRejectedValue(new Error("disk full"));

        const step = new SnapshotBackupStep(deps);
        await expect(step.execute(ctx)).resolves.not.toThrow();
    });
});

// ────────────────────────────────────────────
// Factory: createConsolidationSteps
// ────────────────────────────────────────────
describe("createConsolidationSteps", () => {
    it("should create exactly 9 steps in correct order", () => {
        const deps = createMockDeps();
        const steps = createConsolidationSteps(deps);

        expect(steps).toHaveLength(9);
        expect(steps[0].stepName).toBe("FetchAndGate");
        expect(steps[1].stepName).toBe("ProcessSessions");
        expect(steps[2].stepName).toBe("GCOldEvents");
        expect(steps[3].stepName).toBe("DynamicTaxonomy");
        expect(steps[4].stepName).toBe("WALCheckpoint");
        expect(steps[5].stepName).toBe("ProcessDLQ");
        expect(steps[6].stepName).toBe("EbbinghausDecay");
        expect(steps[7].stepName).toBe("GraphRAGCommunity");
        expect(steps[8].stepName).toBe("SnapshotBackup");
    });
});
