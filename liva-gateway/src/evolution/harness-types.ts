import { z } from "zod";

export const ASTEvaluationPayloadSchema = z.object({
  jobId: z.string().uuid(),
  targetFile: z.string(),
  astDiff: z.string(),
  hypothesis: z.string(),
  expectedExecutionTimeMs: z.number().positive().max(60_000),
  testCommand: z.string().optional(),
});
export type ASTEvaluationPayload = z.infer<typeof ASTEvaluationPayloadSchema>;

export const EvaluationMetricsSchema = z.object({
  jobId: z.string().uuid(),
  verdict: z.enum([
    "APPROVED",
    "REJECTED_HALLUCINATION",
    "REJECTED_SANDBOX",
    "REJECTED_TIMEOUT",
    "REJECTED_SECURITY",
    "REJECTED_SANDBOX_OOM",
  ]),
  hallucinationScore: z.number().min(0).max(1),
  executionLatencyMs: z.number().nonnegative(),
  exitCode: z.number().nullable(),
  stdout: z.string(),
  stderr: z.string(),
  diagnosticCount: z.number().nonnegative(),
  timestamp: z.string(),
});
export type EvaluationMetrics = z.infer<typeof EvaluationMetricsSchema>;

export type HarnessVerdict = EvaluationMetrics["verdict"];

export interface HarnessConfig {
  readonly HALLUCINATION_THRESHOLD: number;
  readonly SANDBOX_TIMEOUT_MS: number;
  readonly MAX_OUTPUT_BYTES: number;
  readonly HITL_TIMEOUT_MS: number;
  readonly USE_DOCKER: boolean;
}

export const DEFAULT_HARNESS_CONFIG: HarnessConfig = {
  HALLUCINATION_THRESHOLD: 0.25,
  SANDBOX_TIMEOUT_MS: 60_000,
  MAX_OUTPUT_BYTES: 512 * 1024,
  HITL_TIMEOUT_MS: 900_000,
  USE_DOCKER: false,
};

export interface SandboxResult {
  pass: boolean;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  executionTimeMs: number;
  killedReason?: "timeout" | "oom_output" | "security";
}

export interface ISandboxExecutor {
  execute(
    sandboxRoot: string,
    testCommand: string,
    timeoutMs: number,
    maxOutputBytes: number,
  ): Promise<SandboxResult>;
  dispose(): void;
}

export const CSHS_WEIGHTS = {
  TS2304_UNDEFINED_NAME: 0.3,
  TS2307_UNRESOLVED_IMPORT: 0.3,
  TS7027_UNREACHABLE_CODE: 0.15,
  TS6133_UNUSED_VARIABLE: 0.05,
  PLACEHOLDER_TOKEN: 0.4,
} as const;
