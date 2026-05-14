import { randomUUID } from "node:crypto";
import { beforeEach, describe, expect, it, vi } from "vitest";

const execFileSyncMock = vi.hoisted(() => vi.fn());
const microSandbox = vi.hoisted(() => ({
  execute: vi.fn(),
  dispose: vi.fn(),
}));
const dockerSandbox = vi.hoisted(() => ({
  execute: vi.fn(),
  dispose: vi.fn(),
}));
const recordEvaluationMock = vi.hoisted(() => vi.fn());
const loggerWarnMock = vi.hoisted(() => vi.fn());

vi.mock("node:child_process", async (importOriginal) => {
  const actual = await importOriginal<typeof import("node:child_process")>();
  return {
    ...actual,
    execFileSync: execFileSyncMock,
  };
});

vi.mock("../../src/sandbox/MicroVMDaemon", () => ({
  MicroVMDaemon: vi.fn(function MicroVMDaemonMock() {
    return microSandbox;
  }),
}));

vi.mock("../../src/sandbox/DockerEnvManager", () => ({
  DockerEnvManager: vi.fn(function DockerEnvManagerMock() {
    return dockerSandbox;
  }),
}));

vi.mock("../../src/memory/HeraCompass", () => ({
  HeraCompass: {
    getInstance: vi.fn(() => ({
      recordEvaluation: recordEvaluationMock,
    })),
  },
}));

vi.mock("../../src/utils/logger", () => ({
  logger: {
    info: vi.fn(),
    warn: loggerWarnMock,
    error: vi.fn(),
    debug: vi.fn(),
  },
}));

import { LivaHarnessOrchestrator } from "../../src/evolution/LivaHarnessOrchestrator";

function payload(overrides: Record<string, unknown> = {}) {
  return {
    jobId: randomUUID(),
    targetFile: "src/example.ts",
    astDiff: "export const value: number = 1;",
    hypothesis: "Safe change",
    expectedExecutionTimeMs: 1000,
    testCommand: "npx tsc --noEmit",
    ...overrides,
  };
}

describe("LivaHarnessOrchestrator", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    execFileSyncMock.mockImplementation(() => {
      throw new Error("Docker unavailable");
    });
    microSandbox.execute.mockResolvedValue({
      pass: true,
      stdout: "ok",
      stderr: "",
      exitCode: 0,
      executionTimeMs: 12,
    });
    dockerSandbox.execute.mockResolvedValue({
      pass: true,
      stdout: "ok",
      stderr: "",
      exitCode: 0,
      executionTimeMs: 12,
    });
  });

  it("approves a valid payload when sandbox passes", async () => {
    const orchestrator = new LivaHarnessOrchestrator();

    const metrics = await orchestrator.evaluateASTActuation(payload());

    expect(metrics.verdict).toBe("APPROVED");
    expect(metrics.exitCode).toBe(0);
    expect(microSandbox.execute).toHaveBeenCalledTimes(1);
  });

  it("rejects high CSHS before sandbox execution", async () => {
    const orchestrator = new LivaHarnessOrchestrator();

    const metrics = await orchestrator.evaluateASTActuation(payload({
      astDiff: "export const value = missingSymbol; // TODO: fill",
    }));

    expect(metrics.verdict).toBe("REJECTED_HALLUCINATION");
    expect(metrics.hallucinationScore).toBeGreaterThan(0.25);
    expect(microSandbox.execute).not.toHaveBeenCalled();
  });

  it("maps sandbox timeout verdict", async () => {
    microSandbox.execute.mockResolvedValueOnce({
      pass: false,
      stdout: "",
      stderr: "timeout",
      exitCode: -1,
      executionTimeMs: 1000,
      killedReason: "timeout",
    });
    const orchestrator = new LivaHarnessOrchestrator();

    const metrics = await orchestrator.evaluateASTActuation(payload());

    expect(metrics.verdict).toBe("REJECTED_TIMEOUT");
  });

  it("maps sandbox OOM verdict", async () => {
    microSandbox.execute.mockResolvedValueOnce({
      pass: false,
      stdout: "",
      stderr: "oom",
      exitCode: -2,
      executionTimeMs: 1000,
      killedReason: "oom_output",
    });
    const orchestrator = new LivaHarnessOrchestrator();

    const metrics = await orchestrator.evaluateASTActuation(payload());

    expect(metrics.verdict).toBe("REJECTED_SANDBOX_OOM");
  });

  it("throws on invalid Zod payload", async () => {
    const orchestrator = new LivaHarnessOrchestrator();

    await expect(orchestrator.evaluateASTActuation({ nope: true })).rejects.toThrow();
  });

  it("throws after dispose", async () => {
    const orchestrator = new LivaHarnessOrchestrator();
    orchestrator.dispose();

    await expect(orchestrator.evaluateASTActuation(payload())).rejects.toThrow("disposed");
    expect(microSandbox.dispose).toHaveBeenCalled();
  });

  it("keeps telemetry failure non-fatal", async () => {
    recordEvaluationMock.mockImplementationOnce(() => {
      throw new Error("hera down");
    });
    const orchestrator = new LivaHarnessOrchestrator();

    const metrics = await orchestrator.evaluateASTActuation(payload());
    await Promise.resolve();

    expect(metrics.verdict).toBe("APPROVED");
    expect(loggerWarnMock).toHaveBeenCalledWith(
      expect.objectContaining({ err: expect.any(Error) }),
      expect.stringContaining("Telemetry persistence failed"),
    );
  });

  it("always uses local MicroVMDaemon sandbox (Docker removed)", async () => {
    // Docker detection is bypassed — always uses MicroVMDaemon
    const orchestrator = new LivaHarnessOrchestrator();

    const metrics = await orchestrator.evaluateASTActuation(payload());

    expect(metrics.verdict).toBe("APPROVED");
    expect(microSandbox.execute).toHaveBeenCalledTimes(1);
  });
});
