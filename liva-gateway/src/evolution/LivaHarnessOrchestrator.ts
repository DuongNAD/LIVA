import { execFileSync } from "node:child_process";
import { performance } from "node:perf_hooks";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";
import { CSHSAnalyzer } from "./CSHSAnalyzer";
import { MicroVMDaemon } from "../sandbox/MicroVMDaemon";
import { HeraCompass } from "../memory/HeraCompass";
import {
  ASTEvaluationPayloadSchema,
  DEFAULT_HARNESS_CONFIG,
  EvaluationMetricsSchema,
  type ASTEvaluationPayload,
  type EvaluationMetrics,
  type HarnessConfig,
  type HarnessVerdict,
  type ISandboxExecutor,
} from "./harness-types";

export class LivaHarnessOrchestrator {
  #isDisposed = false;
  #config: HarnessConfig;
  #cshs: CSHSAnalyzer;
  #sandbox: ISandboxExecutor;
  #tempFileRegistry = new Set<string>();

  constructor(configOverrides?: Partial<HarnessConfig>) {
    this.#config = { ...DEFAULT_HARNESS_CONFIG, ...configOverrides };
    this.#cshs = new CSHSAnalyzer();

    // DockerEnvManager removed (DEPRECATED) — always use local sandbox
    this.#sandbox = new MicroVMDaemon();
    logger.info("[HarnessEngineer] Using local MicroVMDaemon sandbox");

    this.#config = { ...this.#config, USE_DOCKER: false };
  }

  public async evaluateASTActuation(rawPayload: unknown): Promise<EvaluationMetrics> {
    if (this.#isDisposed) {
      throw new Error("HarnessOrchestrator is disposed");
    }

    const payload = ASTEvaluationPayloadSchema.parse(rawPayload);
    const startTime = performance.now();
    let verdict: HarnessVerdict = "APPROVED";
    let stdout = "";
    let stderr = "";
    let exitCode: number | null = null;
    let hallucinationScore = 0;
    let diagnosticCount = 0;

    try {
      const cshsResult = this.#cshs.analyze(
        payload.astDiff,
        payload.jobId,
        this.#config.HALLUCINATION_THRESHOLD,
      );
      hallucinationScore = cshsResult.score;
      diagnosticCount = cshsResult.diagnosticCount;

      if (!cshsResult.pass) {
        verdict = "REJECTED_HALLUCINATION";
        stderr = `CSHS rejected: score=${cshsResult.score}, anchors=${cshsResult.anchors.join("; ")}`;
      } else {
        const sandboxResult = await this.#sandbox.execute(
          path.dirname(path.resolve(payload.targetFile)),
          payload.testCommand || "npx tsc --noEmit",
          Math.min(payload.expectedExecutionTimeMs, this.#config.SANDBOX_TIMEOUT_MS),
          this.#config.MAX_OUTPUT_BYTES,
        );

        stdout = sandboxResult.stdout;
        stderr = sandboxResult.stderr;
        exitCode = sandboxResult.exitCode;

        if (!sandboxResult.pass) {
          if (sandboxResult.killedReason === "oom_output") {
            verdict = "REJECTED_SANDBOX_OOM";
          } else if (sandboxResult.killedReason === "timeout") {
            verdict = "REJECTED_TIMEOUT";
          } else if (sandboxResult.killedReason === "security") {
            verdict = "REJECTED_SECURITY";
          } else {
            verdict = "REJECTED_SANDBOX";
          }
        }
      }
    } catch (error: unknown) {
      stderr = error instanceof Error ? error.message : "Unknown error";
      exitCode = 1;
      verdict = "REJECTED_SANDBOX";
    }

    const metrics = this.#compileMetrics(
      payload,
      verdict,
      hallucinationScore,
      diagnosticCount,
      performance.now() - startTime,
      exitCode,
      stdout,
      stderr,
    );

    this.#persistTelemetry(metrics).catch((error) => {
      logger.warn({ err: error }, "[HarnessEngineer] Telemetry persistence failed (non-fatal)");
    });

    return metrics;
  }

  #compileMetrics(
    payload: ASTEvaluationPayload,
    verdict: HarnessVerdict,
    hallucinationScore: number,
    diagnosticCount: number,
    latencyMs: number,
    exitCode: number | null,
    stdout: string,
    stderr: string,
  ): EvaluationMetrics {
    return EvaluationMetricsSchema.parse({
      jobId: payload.jobId,
      verdict,
      hallucinationScore,
      executionLatencyMs: Math.round(latencyMs),
      exitCode,
      stdout: stdout.substring(0, 4000),
      stderr: stderr.substring(0, 4000),
      diagnosticCount,
      timestamp: new Date().toISOString(),
    });
  }

  async #persistTelemetry(metrics: EvaluationMetrics): Promise<void> {
    const hera = HeraCompass.getInstance();
    hera.recordEvaluation(metrics);
  }

  public dispose(): void {
    if (this.#isDisposed) return;
    this.#isDisposed = true;
    this.#sandbox.dispose();

    for (const tempFile of this.#tempFileRegistry) {
      fsp.rm(tempFile, { force: true }).catch(() => {});
    }
    this.#tempFileRegistry.clear();
    logger.info("[HarnessEngineer] Disposed - all resources released");
  }
}
