import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as path from "node:path";
import * as fs from "node:fs/promises";
import { MicroVMDaemon } from "../../src/sandbox/MicroVMDaemon";

describe("MicroVMDaemon — Sandbox Escape & Process Hardening (Unmocked)", () => {
    let daemon: MicroVMDaemon;
    const tempTestDir = path.join(process.cwd(), "data", "temp_sandbox_test");

    beforeEach(async () => {
        daemon = new MicroVMDaemon();
        await fs.mkdir(tempTestDir, { recursive: true });
        
        // Write a simple tsconfig.json to make tsc compile pass inside the test dir
        const tsconfig = {
            compilerOptions: {
                target: "es2022",
                module: "commonjs",
                strict: true,
                esModuleInterop: true,
                skipLibCheck: true,
                forceConsistentCasingInFileNames: true
            }
        };
        await fs.writeFile(path.join(tempTestDir, "tsconfig.json"), JSON.stringify(tsconfig, null, 2), "utf-8");
    });

    afterEach(async () => {
        await fs.rm(tempTestDir, { recursive: true, force: true }).catch(() => {});
    });

    it("should scrub sensitive environment variables at the OS process boundary", async () => {
        // 1. Inject a mock sensitive key into process.env
        process.env.LIVA_SECRET_API_KEY = "TOP_SECRET_JWT_KEY_99999";
        
        // 2. Prepare a test script that prints environment variables
        const scriptContent = `
            console.log("ENV_KEYS:", JSON.stringify(Object.keys(process.env)));
            if (process.env.LIVA_SECRET_API_KEY) {
                console.log("LEAK:", process.env.LIVA_SECRET_API_KEY);
            } else {
                console.log("SAFE: Key not found");
            }
        `;
        await fs.writeFile(path.join(tempTestDir, "test_env.js"), scriptContent, "utf-8");

        // 3. Verify in sandbox using real node execution
        const result = await daemon.verifyShadowCandidate(tempTestDir, "node test_env.js");

        // 4. Assert key is completely missing from child process environment
        expect(result.pass).toBe(true);
        expect(result.vmLogs).toContain("SAFE: Key not found");
        expect(result.vmLogs).not.toContain("TOP_SECRET_JWT_KEY_99999");
        expect(result.vmLogs).not.toContain("LIVA_SECRET_API_KEY");

        // Clean up mock env
        delete process.env.LIVA_SECRET_API_KEY;
    });

    it("should terminate infinite loop execution at the OS level on timeout", async () => {
        // 1. Create a script with an infinite CPU loop
        const loopScript = `
            console.log("Loop starting...");
            while(true) {}
        `;
        await fs.writeFile(path.join(tempTestDir, "loop.js"), loopScript, "utf-8");

        // 2. Run in sandbox with a short command (timeout is verified synchronously in runCommandSync)
        const startTime = Date.now();
        
        // We call the inner runCommandSync via public method by passing command to verifyShadowCandidate
        // Wait, default testCommand timeout in verifyShadowCandidate is SANDBOX_TIMEOUT_MS (60s).
        // Since we want this unit test to run quickly, we can execute via public .execute() method
        // which takes workingDir, command, timeoutMs (custom timeout).
        // Let's verify daemon.execute(tempTestDir, "node loop.js", 800, 1024)
        const result = await daemon.execute(tempTestDir, "node loop.js", 800, 1024);
        
        const duration = Date.now() - startTime;

        // 3. Assert the process was terminated and did not freeze the test suite
        expect(result.pass).toBe(false);
        expect(result.exitCode).toBe(1);
        expect(result.stdout).toContain("TIMEOUT");
        expect(duration).toBeLessThan(10000); // Should exit well under 10 seconds
    });

    it("should deny path traversal containing forbidden folder segments", async () => {
        const sensitivePath = path.join(tempTestDir, ".ssh", "keys");
        const result = await daemon.verifyShadowCandidate(sensitivePath, "node -v");

        expect(result.pass).toBe(false);
        expect(result.vmLogs).toContain("SECURITY BLOCK");
        expect(result.vmLogs).toContain("Path access denied");
    });
});
