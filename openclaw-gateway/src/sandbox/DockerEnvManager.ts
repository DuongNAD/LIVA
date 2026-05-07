import { spawn } from "child_process";
import { logger } from "../utils/logger";
import type { ISandboxExecutor } from "../evolution/harness-types";

export class DockerEnvManager implements ISandboxExecutor {
    public async execute(workingDir: string, command: string, timeoutMs: number, maxOutputBytes: number) {
        try {
            const startTime = Date.now();
            const out = await this.runSandboxTest(command.split(" "));
            return { pass: true, exitCode: 0, stdout: out, stderr: "", executionTimeMs: Date.now() - startTime };
        } catch (e: any) {
            return { pass: false, exitCode: 1, stdout: "", stderr: e.message, killedReason: "timeout" as any, executionTimeMs: 0 };
        }
    }

    public dispose(): void {
        this.cleanupZombieContainer();
    }
    public async runSandboxTest(commandArgs: string[]): Promise<string> {
        return new Promise((resolve, reject) => {
            const controller = new AbortController();
            const timeoutId = setTimeout(() => controller.abort(), 60000); // 60s timeout

            const dockerArgs = [
                "run", "--rm",
                "--network", "none",
                "--memory=512m",
                "--cpus=1.0",
                "--pids-limit=64",
                "--read-only",
                "--tmpfs", "/tmp:rw,noexec,nosuid",
                "--security-opt", "no-new-privileges:true",
                "--user", "1000:1000",
                "-v", `${process.cwd()}/src:/app/src:ro`,
                "node:20-alpine",
                ...commandArgs
            ];

            const child = spawn("docker", dockerArgs, { signal: controller.signal });

            let output = "";
            let errorOutput = "";

            child.stdout.on("data", (data) => output += data.toString());
            child.stderr.on("data", (data) => errorOutput += data.toString());

            child.on("close", (code) => {
                clearTimeout(timeoutId); // Bắt buộc Rule
                if (code === 0) {
                    resolve(output);
                } else {
                    reject(new Error(`Exit code ${code}. Error: ${errorOutput}`));
                }
            });

            child.on("error", (err) => {
                clearTimeout(timeoutId); // Bắt buộc
                if (err.name === 'AbortError') {
                    // Dọn dẹp zombie process bằng docker kill (boilerplate giả lập)
                    this.cleanupZombieContainer().catch(e => logger.error(`[Docker] Lỗi cleanup: ${e.message}`));
                    reject(new Error("Timeout 60s. Bị ngắt bởi AbortController."));
                } else {
                    reject(err);
                }
            });
        });
    }

    private async cleanupZombieContainer() {
        logger.warn("[DockerEnvManager] Đang dọn dẹp các sandbox container bị kẹt (Zombie process)...");
    }
}
