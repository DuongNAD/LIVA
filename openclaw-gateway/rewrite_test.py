import re

with open("tests/skills/ExecuteCommand.test.ts", "r", encoding="utf-8") as f:
    content = f.read()

# Replace the mocks at the top
top_mocks = """import { describe, it, expect, vi, beforeEach } from "vitest";

// Control HITL auto-response (y/n) per test
let hitlResponse = "y";

vi.mock("@security/HITLGuard", () => ({
    HITLGuard: {
        requestApproval: vi.fn().mockImplementation(async () => {
            if (hitlResponse === "y" || hitlResponse === "yes") return true;
            if (hitlResponse === "n" || hitlResponse === "") return false;
            return false;
        })
    }
}));

vi.mock("node:child_process", () => ({
    spawn: vi.fn(),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { spawn } from "node:child_process";
const mockSpawn = vi.mocked(spawn);

function createMockSpawn(stdoutStr: string, stderrStr: string, exitCode: number = 0) {
    return {
        stdout: { on: (event: string, cb: any) => { if (event === 'data') cb(Buffer.from(stdoutStr)); } },
        stderr: { on: (event: string, cb: any) => { if (event === 'data') cb(Buffer.from(stderrStr)); } },
        on: (event: string, cb: any) => {
            if (event === 'error' && exitCode !== 0) cb(new Error("Command failed"));
            else if (event === 'close') cb(exitCode);
        }
    } as any;
}

// ============================================================
// Tests
// ============================================================
"""

content = re.sub(r'import \{ describe[\s\S]*?// Tests\n// ============================================================\n', top_mocks, content)

# Replace mockExec with mockSpawn
content = content.replace("mockExec", "mockSpawn")

# Replace mockSpawn.mockImplementation((cmd: string, cb...
content = re.sub(
    r'mockSpawn\.mockImplementation\(\(\(cmd: string, cb: \(err: any, result: any\) => void\) => \{\s*cb\(null, \{ stdout: "(.*?)", stderr: "" \}\);\s*\}\) as any\);',
    r'mockSpawn.mockImplementation(() => createMockSpawn("\1", ""));',
    content
)

# Handle the error case replacement
error_impl = """mockSpawn.mockImplementation(((cmd: string, cb: (err: any, result: any) => void) => {
                const err = new Error("Command not found") as any;
                err.stdout = "";
                cb(err, null);
            }) as any);"""

new_error_impl = """mockSpawn.mockImplementation(() => createMockSpawn("", "", 1));"""
content = content.replace(error_impl, new_error_impl)

with open("tests/skills/ExecuteCommand.test.ts", "w", encoding="utf-8") as f:
    f.write(content)
