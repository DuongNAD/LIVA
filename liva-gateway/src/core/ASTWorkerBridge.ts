/**
 * ASTWorkerBridge — Main-thread bridge to ASTWorker (one-shot Worker Thread)
 * ==========================================================================
 * [AI_CONTEXT §4 CRITICAL_DIRECTIVE — Event Loop Protection]
 *
 * Offloads heavy ts-morph operations (full multi-file type-check, import
 * healing) off the Gateway event loop. Each call spawns a fresh worker,
 * runs ONE operation, and terminates it — no persistent state or timers,
 * so nothing needs to be wired into CoreKernel.shutdown().
 *
 * A hard timeout guards against silent ts-morph/WASM deadlocks (AI_CONTEXT §6
 * "Silent Worker Deadlocks").
 */
import { Worker } from "node:worker_threads";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { logger } from "../utils/logger";
import type { AstWorkerOp, AstWorkerResponse } from "../workers/ASTWorker";

const _dirname = import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));

// Full multi-file type-check can be slow on large sandboxes; allow generous headroom.
const AST_OP_TIMEOUT_MS = 120_000;

function spawnWorker(): Worker {
    const workerPath = path.join(_dirname, "..", "workers", "ASTWorker.ts");

    if (process.env.NODE_ENV === "production") {
        const prodWorkerPath = workerPath.replace(/\.ts$/, ".js");
        return new Worker(prodWorkerPath);
    }

    const workerUrl = pathToFileURL(workerPath).href;
    return new Worker(
        `
        import { register } from 'node:module';
        import { pathToFileURL } from 'node:url';
        register('tsx', pathToFileURL('./'), { data: {} });
        import('${workerUrl.replace(/\\/g, "\\\\")}');
        `,
        { eval: true, execArgv: [] }
    );
}

/**
 * Run a single AST operation in a throwaway worker thread.
 * Resolves with the worker's structured response; rejects on spawn failure,
 * worker crash, or timeout.
 */
function runAstOp(op: AstWorkerOp, reqObj: Partial<import("../workers/ASTWorker").AstWorkerRequest>): Promise<AstWorkerResponse> {
    return new Promise<AstWorkerResponse>((resolve, reject) => {
        let worker: Worker;
        try {
            worker = spawnWorker();
        } catch (e: unknown) {
            reject(e instanceof Error ? e : new Error(String(e)));
            return;
        }

        let settled = false;
        const finish = (fn: () => void) => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            worker.terminate().catch(() => { /* already gone */ });
            fn();
        };

        const timer = setTimeout(() => {
            logger.error(`[ASTWorkerBridge] 🏥 AST worker timed out (${AST_OP_TIMEOUT_MS}ms) on op '${op}'. Terminating.`);
            finish(() => reject(new Error(`AST worker timed out after ${AST_OP_TIMEOUT_MS}ms (op: ${op})`)));
        }, AST_OP_TIMEOUT_MS);
        timer.unref();

        worker.on("message", (res: AstWorkerResponse) => {
            finish(() => resolve(res));
        });
        worker.on("error", (err: Error) => {
            logger.error(`[ASTWorkerBridge] ❌ AST worker crashed (op '${op}'): ${err.message}`);
            finish(() => reject(err));
        });
        worker.on("exit", (code) => {
            if (code !== 0) {
                finish(() => reject(new Error(`AST worker exited with code ${code} (op: ${op})`)));
            }
        });

        worker.postMessage({ op, ...reqObj });
    });
}

export class ASTWorkerBridge {
    /** Auto-route imports + clean code across the whole sandbox (offloaded). */
    static async healImports(sandboxRoot: string): Promise<{ success: boolean; logs: string }> {
        const res = await runAstOp("heal", { sandboxRoot });
        if (!res.ok) throw new Error(res.error || "AST worker heal failed");
        return { success: res.success ?? false, logs: res.logs ?? "" };
    }

    /** Collect pre-emit diagnostics → ASI report (offloaded). */
    static async getDiagnostics(sandboxRoot: string): Promise<string> {
        const res = await runAstOp("diagnostics", { sandboxRoot });
        if (!res.ok) throw new Error(res.error || "AST worker diagnostics failed");
        return res.asi ?? "";
    }

    /** CSHS Analysis (offloaded). */
    static async analyzeCSHS(astDiff: string, jobId: string, threshold: number): Promise<NonNullable<AstWorkerResponse["cshsResult"]>> {
        const res = await runAstOp("cshsAnalyze", { astDiff, jobId, threshold });
        if (!res.ok || !res.cshsResult) throw new Error(res.error || "AST worker CSHS analyze failed");
        return res.cshsResult;
    }

    /** Apply AST surgery (offloaded). */
    static async applySurgery(targetFile: string, instructions: any): Promise<string> {
        const res = await runAstOp("surgery", { targetFile, instructions });
        if (!res.ok) throw new Error(res.error || "AST worker surgery failed");
        return res.newCode ?? "";
    }
}
