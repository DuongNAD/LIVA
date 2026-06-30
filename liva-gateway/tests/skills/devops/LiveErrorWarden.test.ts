import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const realPlatform = process.platform;
function setPlatform(p: string) {
    Object.defineProperty(process, "platform", { value: p, configurable: true });
}

const mockExecAsync = vi.hoisted(() => vi.fn(() => Promise.resolve({ stdout: "", stderr: "" })));
const mockSpawn = vi.hoisted(() => vi.fn());

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));
vi.mock("node:util", () => ({ promisify: () => mockExecAsync }));
vi.mock("node:child_process", () => ({ exec: vi.fn(), spawn: mockSpawn }));

import { copyToClipboard, metadata } from "../../../src/skills/devops/LiveErrorWarden";

describe("Skill - LiveErrorWarden clipboard", () => {
    beforeEach(() => { vi.clearAllMocks(); });
    afterEach(() => { setPlatform(realPlatform); });

    it("exports metadata", () => {
        expect(metadata.name).toBe("live_error_warden");
    });

    it("uses `pbcopy` (text via stdin) on darwin", () => {
        setPlatform("darwin");
        const stdin = { end: vi.fn(), on: vi.fn() };
        mockSpawn.mockReturnValue({ stdin, on: vi.fn() });

        copyToClipboard("boom: Exception at line 5");

        expect(mockSpawn).toHaveBeenCalledWith("pbcopy");
        expect(stdin.end).toHaveBeenCalledWith("boom: Exception at line 5");
        expect(mockExecAsync).not.toHaveBeenCalled();
    });

    it("uses PowerShell Set-Clipboard on win32", () => {
        setPlatform("win32");
        copyToClipboard("oops");
        expect(mockExecAsync).toHaveBeenCalled();
        expect(mockExecAsync.mock.calls[0][0]).toContain("Set-Clipboard");
        expect(mockSpawn).not.toHaveBeenCalled();
    });
});
