/**
 * E2B Firecracker Cloud Sandbox (Phase 3)
 * ========================================
 * Executes candidate code in a fully isolated, ephemeral Firecracker microVM
 * powered by E2B (e2b.dev).
 * 
 * Provides true OS-level isolation (unlike local Regex/ExecSync wrappers).
 * Best used for untrusted third-party code or AI-generated complex scripts.
 *
 * [v5.0] LIVA Remote Control Hub
 */

import { Sandbox } from "@e2b/code-interpreter";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import { logger } from "../utils/logger";

export class E2BFirecrackerSandbox {
    readonly #apiKey: string;
    readonly #timeoutMs: number;

    constructor(timeoutMs = 60_000) {
        this.#apiKey = process.env.E2B_API_KEY || "";
        this.#timeoutMs = timeoutMs;
    }

    public isEnabled(): boolean {
        return this.#apiKey.length > 0;
    }

    /**
     * Executes untrusted code in a secure Cloud Firecracker MicroVM.
     * 1. Spawns an ephemeral VM.
     * 2. Uploads the target files.
     * 3. Executes the command.
     * 4. Destroys the VM.
     */
    public async verifyShadowCandidate(
        sandboxRoot: string,
        testCommand: string = "npm test"
    ): Promise<{ pass: boolean; vmLogs: string; executionTimeMs: number }> {
        const startTime = Date.now();

        if (!this.isEnabled()) {
            return {
                pass: false,
                vmLogs: "[E2B_Sandbox] ERROR: E2B_API_KEY is not configured.",
                executionTimeMs: 0
            };
        }

        let sandbox: Sandbox | null = null;
        try {
            logger.info("[E2B_Sandbox] 🚀 Spawning Firecracker MicroVM...");
            sandbox = await Sandbox.create({ apiKey: this.#apiKey });

            logger.info(`[E2B_Sandbox] 📦 Uploading workspace: ${path.basename(sandboxRoot)}`);
            // In a real scenario, we'd zip and upload. For MVP, we upload just the primary files or use an E2B template.
            // Here we do a basic file transfer of essential files.
            await this.#uploadWorkspace(sandbox, sandboxRoot);

            logger.info(`[E2B_Sandbox] ⚙️ Executing command: ${testCommand}`);
            const execResult = await sandbox.commands.run(testCommand, {
                timeout: this.#timeoutMs,
                cwd: "/home/user/workspace"
            });

            const pass = execResult.exitCode === 0;
            const vmLogs = (execResult.stdout || "") + "\n" + (execResult.stderr || "");

            logger.info(`[E2B_Sandbox] ✅ Execution finished. Exit code: ${execResult.exitCode}`);

            return {
                pass,
                vmLogs: `[E2B Firecracker Runtime]\nExit Code: ${execResult.exitCode}\n---\n${vmLogs}`,
                executionTimeMs: Date.now() - startTime
            };

        } catch (error: any) {
            logger.error(`[E2B_Sandbox] ❌ Critical failure: ${error.message}`);
            return {
                pass: false,
                vmLogs: `[E2B_Sandbox] FATAL CRASH: ${error.message}`,
                executionTimeMs: Date.now() - startTime
            };
        } finally {
            /* istanbul ignore next */
            if (sandbox) {
                logger.info("[E2B_Sandbox] 🧹 Destroying MicroVM...");
                await sandbox.kill().catch(e => logger.warn(`[E2B_Sandbox] Failed to kill VM: ${e.message}`));
            }
        }
    }

    async #uploadWorkspace(sandbox: Sandbox, localPath: string): Promise<void> {
        // Create workspace dir in VM
        await sandbox.commands.run("mkdir -p /home/user/workspace");

        // Simple sync of top-level files (to avoid massive uploads during tests)
        const entries = await fs.readdir(localPath, { withFileTypes: true });
        
        for (const entry of entries) {
            // Skip node_modules and .git for speed
            if (entry.name === "node_modules" || entry.name === ".git") continue;
            
            const fullPath = path.join(localPath, entry.name);
            /* istanbul ignore next */
            if (entry.isFile()) {
                const content = await fs.readFile(fullPath, "utf-8");
                // Upload to sandbox
                await sandbox.files.write(`/home/user/workspace/${entry.name}`, content);
            }
        }
    }
}
