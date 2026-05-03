import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { E2BFirecrackerSandbox } from "../../src/sandbox/E2BFirecrackerSandbox";

// Mock @e2b/code-interpreter
const mockRun = vi.fn();
const mockWrite = vi.fn();
const mockKill = vi.fn().mockResolvedValue(undefined);

vi.mock("@e2b/code-interpreter", () => {
    return {
        Sandbox: {
            create: vi.fn().mockImplementation(() => {
                return {
                    commands: { run: mockRun },
                    files: { write: mockWrite },
                    kill: mockKill
                };
            })
        }
    };
});

// Mock fs
vi.mock("node:fs/promises", () => ({
    readdir: vi.fn().mockResolvedValue([
        { name: "test.ts", isFile: () => true },
        { name: "node_modules", isFile: () => false }
    ]),
    readFile: vi.fn().mockResolvedValue("console.log('hello')")
}));

describe("E2BFirecrackerSandbox", () => {
    let sandbox: E2BFirecrackerSandbox;
    const originalEnv = process.env;

    beforeEach(() => {
        vi.clearAllMocks();
        process.env = { ...originalEnv, E2B_API_KEY: "test_key" };
        sandbox = new E2BFirecrackerSandbox(5000);
    });

    afterEach(() => {
        process.env = originalEnv;
    });

    it("isEnabled returns true if API key exists", () => {
        expect(sandbox.isEnabled()).toBe(true);
    });

    it("isEnabled returns false if API key is missing", () => {
        delete process.env.E2B_API_KEY;
        const disabledSandbox = new E2BFirecrackerSandbox();
        expect(disabledSandbox.isEnabled()).toBe(false);
    });

    it("verifyShadowCandidate should abort if disabled", async () => {
        delete process.env.E2B_API_KEY;
        const disabledSandbox = new E2BFirecrackerSandbox();
        
        const result = await disabledSandbox.verifyShadowCandidate("/tmp/test");
        expect(result.pass).toBe(false);
        expect(result.vmLogs).toContain("E2B_API_KEY is not configured");
    });

    it("verifyShadowCandidate should upload files and run command", async () => {
        mockRun.mockResolvedValueOnce({ exitCode: 0 }); // for mkdir
        mockRun.mockResolvedValueOnce({ exitCode: 0, stdout: "Success", stderr: "" }); // for testCommand

        const result = await sandbox.verifyShadowCandidate("/tmp/test", "npm test");

        expect(result.pass).toBe(true);
        expect(result.vmLogs).toContain("Success");
        
        // Assert mkdir was called
        expect(mockRun.mock.calls[0][0]).toBe("mkdir -p /home/user/workspace");
        
        // Assert file upload
        expect(mockWrite).toHaveBeenCalledWith("/home/user/workspace/test.ts", "console.log('hello')");
        
        // Assert actual command
        expect(mockRun.mock.calls[1][0]).toBe("npm test");
        
        // Assert cleanup
        expect(mockKill).toHaveBeenCalledTimes(1);
    });

    it("verifyShadowCandidate should return pass=false on non-zero exit", async () => {
        mockRun.mockResolvedValueOnce({ exitCode: 0 }); // mkdir
        mockRun.mockResolvedValueOnce({ exitCode: 1, stdout: "", stderr: "Syntax Error" }); // testCommand

        const result = await sandbox.verifyShadowCandidate("/tmp/test", "npm test");

        expect(result.pass).toBe(false);
        expect(result.vmLogs).toContain("Syntax Error");
    });

    it("verifyShadowCandidate should handle crashes gracefully and ensure kill is called", async () => {
        mockRun.mockRejectedValueOnce(new Error("Network Error"));

        const result = await sandbox.verifyShadowCandidate("/tmp/test", "npm test");

        expect(result.pass).toBe(false);
        expect(result.vmLogs).toContain("FATAL CRASH: Network Error");
        
        // Cleanup must still run
        expect(mockKill).toHaveBeenCalledTimes(1);
    });
});
