import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ConsolidationPipeline } from "../../src/memory/ConsolidationPipeline";
import { ConsolidationCron } from "../../src/memory/ConsolidationCron";
import { createConsolidationSteps, FetchAndGateStep, WALCheckpointStep } from "../../src/memory/ConsolidationSteps";
import { TaskQueue } from "../../src/core/TaskQueue";
import { promises as fsp } from "node:fs";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  },
}));

vi.mock("node:fs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("node:fs")>();
  return {
    ...actual,
    promises: {
      ...actual.promises,
      readFile: vi.fn().mockImplementation(() => Promise.reject(new Error("File not found"))),
    },
  };
});

describe("Memory Consolidation Tests", () => {
  let mockDb: any;
  let mockStructuredMemory: any;
  let mockEmbeddingService: any;
  let mockBookIndex: any;
  let mockAiClient: any;
  let mockReconsolidationEngine: any;
  let mockDreamingPipeline: any;

  beforeEach(() => {
    vi.clearAllMocks();
    TaskQueue.getInstance()['#isShutdown'] = false;
    TaskQueue.getInstance()['queue'] = [];
    TaskQueue.getInstance()['isProcessing'] = false;

    mockDb = {
      exec: vi.fn(),
      prepare: vi.fn().mockReturnValue({
        run: vi.fn(),
        all: vi.fn().mockReturnValue([]),
        get: vi.fn().mockReturnValue({ last_step: 2, state_data: "{}" }),
      }),
    };

    mockStructuredMemory = {
      getDb: () => mockDb,
      getUnconsolidatedEvents: vi.fn().mockResolvedValue([]),
      getUnconsolidatedCount: vi.fn().mockResolvedValue(5),
      markConsolidated: vi.fn().mockResolvedValue(undefined),
      setFact: vi.fn().mockResolvedValue(undefined),
      upsertVector: vi.fn().mockResolvedValue(undefined),
      gcOldEvents: vi.fn().mockResolvedValue(undefined),
      applyMemoryDecay: vi.fn().mockResolvedValue({ decayed: 1, archived: 0 }),
      createSnapshotBackup: vi.fn().mockResolvedValue(undefined),
      processDLQ: vi.fn().mockResolvedValue(undefined),
      graph: {
        upsertNode: vi.fn().mockResolvedValue(undefined),
        upsertEdge: vi.fn().mockResolvedValue(undefined),
        buildCommunitySummaries: vi.fn().mockResolvedValue(undefined),
      },
    };

    mockEmbeddingService = {
      ensureReady: vi.fn().mockResolvedValue(undefined),
      embed: vi.fn().mockResolvedValue(new Array(384).fill(0.01)),
      embedBatch: vi.fn().mockImplementation((texts: string[]) => 
        Promise.resolve(texts.map(() => new Array(384).fill(0.01)))
      ),
      embedWithTimeout: vi.fn().mockResolvedValue(new Array(384).fill(0.01)),
    };

    mockBookIndex = {
      addNode: vi.fn(),
      addEdge: vi.fn(),
    };

    mockAiClient = {
      chat: {
        completions: {
          create: vi.fn().mockResolvedValue({
            choices: [{ message: { content: '{"narrative_summary":"Summary","new_user_insights":[],"graph_nodes":[],"graph_edges":[]}' } }],
          }),
        },
      },
    };

    mockReconsolidationEngine = {
      sweepAndReconcile: vi.fn().mockResolvedValue(undefined),
    };

    mockDreamingPipeline = {
      executeDreamingSequence: vi.fn().mockResolvedValue(null),
    };
  });

  it("should initialize ConsolidationPipeline with the standard 9 steps", () => {
    const deps = {
      structuredMemory: mockStructuredMemory,
      embeddingService: mockEmbeddingService,
      aiClient: mockAiClient,
      bookIndex: mockBookIndex,
      contradictionResolver: {} as any,
      reconsolidationEngine: mockReconsolidationEngine,
      synthesisPrompt: "Prompt",
    };

    const steps = createConsolidationSteps(deps, false);
    expect(steps).toHaveLength(9);

    const stepNames = steps.map((s) => s.stepName);
    expect(stepNames).toContain("FetchAndGate");
    expect(stepNames).toContain("ProcessSessions");
    expect(stepNames).toContain("GCOldEvents");
    expect(stepNames).toContain("DynamicTaxonomy");
    expect(stepNames).toContain("WALCheckpoint");
    expect(stepNames).toContain("ProcessDLQ");
    expect(stepNames).toContain("EbbinghausDecay");
    expect(stepNames).toContain("GraphRAGCommunity");
    expect(stepNames).toContain("SnapshotBackup");
  });

  it("should resume ConsolidationPipeline from last checkpoint using the DB schema", async () => {
    const dbExec = vi.fn();
    const dbPrepareGet = vi.fn().mockReturnValue({
      get: vi.fn().mockReturnValue({ last_step: 3, state_data: '{"data":"test"}' }),
    });
    const dbPrepareRun = vi.fn().mockReturnValue({
      run: vi.fn(),
    });

    const pipeline = new ConsolidationPipeline(dbExec, dbPrepareGet, dbPrepareRun);
    const resumeCtx = pipeline.resumeFromCheckpoint("test_session");

    expect(dbPrepareGet).toHaveBeenCalledWith(
      expect.stringContaining("SELECT last_step, state_data FROM consolidation_checkpoints")
    );
    expect(resumeCtx).toBeDefined();
    expect(resumeCtx?.currentStepIndex).toBe(3);
    expect(resumeCtx?.sharedState).toEqual({ data: "test" });
  });

  it("should block ConsolidationCron when AgentLoop is not IDLE (LLM busy guard) using fake timers", async () => {
    vi.useFakeTimers();

    const cron = new ConsolidationCron(
      mockStructuredMemory,
      mockEmbeddingService,
      mockBookIndex,
      mockAiClient,
      mockReconsolidationEngine,
      mockDreamingPipeline
    );

    const spyConsolidate = vi.spyOn(cron, "consolidateNow").mockResolvedValue(0);

    // Mock states: busy AgentLoop
    cron.setAgentLoopStateGetter(() => "RUNNING");
    cron.recordActivity("TOPIC_SHIFT");
    cron.recordActivity("TOPIC_SHIFT");
    cron.recordActivity("TOPIC_SHIFT"); // Topic shift count = 3

    // Advance timer by 15s to trigger
    await vi.advanceTimersByTimeAsync(15000);

    expect(spyConsolidate).not.toHaveBeenCalled();

    // Now change AgentLoop to IDLE and trigger again
    cron.setAgentLoopStateGetter(() => "IDLE");
    cron.recordActivity("TOPIC_SHIFT");
    cron.recordActivity("TOPIC_SHIFT");
    cron.recordActivity("TOPIC_SHIFT");

    await vi.advanceTimersByTimeAsync(15000);

    expect(spyConsolidate).toHaveBeenCalled();

    vi.useRealTimers();
  });

  it("should return 0 from consolidateNow(false) when expert model is active on VRAM (concurrency VRAM guard)", async () => {
    const cron = new ConsolidationCron(
      mockStructuredMemory,
      mockEmbeddingService,
      mockBookIndex,
      mockAiClient,
      mockReconsolidationEngine,
      mockDreamingPipeline
    );

    // Mock states: expert model on VRAM
    cron.setModelTypeGetter(() => "expert");

    const result = await cron.consolidateNow(false);
    expect(result).toBe(0);
  });

  it("should handle WAL checkpoint SQLite busy contention gracefully without throwing", async () => {
    const deps = {
      structuredMemory: mockStructuredMemory,
      embeddingService: mockEmbeddingService,
      aiClient: mockAiClient,
      bookIndex: mockBookIndex,
      contradictionResolver: {} as any,
      reconsolidationEngine: mockReconsolidationEngine,
      synthesisPrompt: "Prompt",
    };

    const ctx = {
      sessionId: "test",
      currentStepIndex: 0,
      totalConsolidated: 0,
      sharedState: {
        events: [{ eventId: "1" }],
      } as any,
    };

    mockDb.exec.mockImplementation((sql: string) => {
      if (sql.includes("PRAGMA wal_checkpoint")) {
        throw new Error("SQLITE_BUSY: database is locked");
      }
    });

    const step = new WALCheckpointStep(deps);
    await expect(step.execute(ctx)).resolves.not.toThrow();
    expect(mockDb.exec).toHaveBeenCalledWith("PRAGMA wal_checkpoint(PASSIVE)");
  });

  it("should enforce battery gating and scale threshold when running on battery", async () => {
    vi.mocked(fsp.readFile).mockResolvedValue(JSON.stringify({ is_battery: true }));

    const deps = {
      structuredMemory: mockStructuredMemory,
      embeddingService: mockEmbeddingService,
      aiClient: mockAiClient,
      bookIndex: mockBookIndex,
      contradictionResolver: {} as any,
      reconsolidationEngine: mockReconsolidationEngine,
      synthesisPrompt: "Prompt",
    };

    mockStructuredMemory.getUnconsolidatedEvents.mockResolvedValue(new Array(25).fill({}));

    const step = new FetchAndGateStep(deps, false);
    const ctx = {
      sessionId: "test",
      currentStepIndex: 0,
      totalConsolidated: 0,
      sharedState: {} as any,
    };

    await step.execute(ctx);

    // scaled threshold is 50, events (25) < threshold (50) -> skips consolidation
    expect(ctx.sharedState.events).toEqual([]);
    expect(ctx.sharedState.sessions).toEqual([]);

    // Now test with is_battery: false
    vi.mocked(fsp.readFile).mockResolvedValue(JSON.stringify({ is_battery: false }));
    const ctx2 = {
      sessionId: "test",
      currentStepIndex: 0,
      totalConsolidated: 0,
      sharedState: {} as any,
    };
    await step.execute(ctx2);

    // standard threshold is 10, events (25) >= threshold (10) -> proceeds with consolidation
    expect(ctx2.sharedState.events).toHaveLength(25);
  });
});
