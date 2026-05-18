import { describe, expect, it } from "vitest";
import { randomUUID } from "node:crypto";
import {
  ASTEvaluationPayloadSchema,
  DEFAULT_HARNESS_CONFIG,
  EvaluationMetricsSchema,
} from "../../src/evolution/harness-types";

describe("harness-types", () => {
  it("parses a valid AST evaluation payload", () => {
    const payload = ASTEvaluationPayloadSchema.parse({
      jobId: randomUUID(),
      targetFile: "src/core/Foo.ts",
      astDiff: "export const value = 1;",
      hypothesis: "Improve Foo",
      expectedExecutionTimeMs: 1000,
      testCommand: "npx tsc --noEmit",
    });

    expect(payload.expectedExecutionTimeMs).toBe(1000);
  });

  it("rejects invalid payload boundaries", () => {
    expect(() =>
      ASTEvaluationPayloadSchema.parse({
        jobId: "not-a-uuid",
        targetFile: "src/core/Foo.ts",
        astDiff: "export const value = 1;",
        hypothesis: "Improve Foo",
        expectedExecutionTimeMs: 90_000,
      }),
    ).toThrow();
  });

  it("parses valid metrics with nullable exitCode", () => {
    const metrics = EvaluationMetricsSchema.parse({
      jobId: randomUUID(),
      verdict: "REJECTED_TIMEOUT",
      hallucinationScore: 0.2,
      executionLatencyMs: 3000,
      exitCode: null,
      stdout: "",
      stderr: "timeout",
      diagnosticCount: 0,
      timestamp: new Date().toISOString(),
    });

    expect(metrics.exitCode).toBeNull();
  });

  it("rejects metrics with out-of-range hallucination score", () => {
    expect(() =>
      EvaluationMetricsSchema.parse({
        jobId: randomUUID(),
        verdict: "APPROVED",
        hallucinationScore: 2,
        executionLatencyMs: 0,
        exitCode: 0,
        stdout: "",
        stderr: "",
        diagnosticCount: 0,
        timestamp: new Date().toISOString(),
      }),
    ).toThrow();
  });

  it("exposes conservative defaults", () => {
    expect(DEFAULT_HARNESS_CONFIG.HALLUCINATION_THRESHOLD).toBe(0.25);
    expect(DEFAULT_HARNESS_CONFIG.SANDBOX_TIMEOUT_MS).toBe(60_000);
    expect(DEFAULT_HARNESS_CONFIG.MAX_OUTPUT_BYTES).toBe(512 * 1024);
  });
});
