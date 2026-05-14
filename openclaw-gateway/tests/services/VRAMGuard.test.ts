import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const { mockExec } = vi.hoisted(() => ({
    mockExec: vi.fn(),
}));

vi.mock("node:child_process", () => ({
    exec: mockExec,
    spawn: vi.fn(),
    ChildProcess: vi.fn(),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { VRAMGuard } from "../../src/services/VRAMGuard";

describe("VRAMGuard", () => {
    let guard: VRAMGuard;

    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();
        Object.defineProperty(process, "platform", { value: "win32", configurable: true });
        guard = new VRAMGuard(5000);
    });

    afterEach(() => {
        guard.dispose();
        vi.useRealTimers();
    });

    it("should start and stop without errors", () => {
        guard.start();
        guard.dispose();
    });

    it("should be idempotent on start", () => {
        guard.start();
        guard.start();
        guard.dispose();
    });

    it("should emit yield_vram when heavy app detected", async () => {
        const yieldSpy = vi.fn();
        guard.on("yield_vram", yieldSpy);

        mockExec.mockImplementation((cmd: string, opts: any, cb: Function) => {
            if (cmd.includes("tasklist")) {
                cb(null, '"cyberpunk2077.exe","1234","Console","1","500,000 K"\n');
            } else {
                cb(null, "50\n");
            }
        });

        guard.start();
        vi.advanceTimersByTime(5000);
        await new Promise(r => process.nextTick(r));
        await new Promise(r => process.nextTick(r));

        expect(yieldSpy).toHaveBeenCalledWith(
            expect.objectContaining({ appName: "cyberpunk2077.exe" })
        );
        expect(guard.isYielded).toBe(true);
    });

    it("should emit reclaim_vram when heavy app exits", async () => {
        const reclaimSpy = vi.fn();
        guard.on("reclaim_vram", reclaimSpy);

        let hasGame = true;
        mockExec.mockImplementation((cmd: string, opts: any, cb: Function) => {
            if (cmd.includes("tasklist")) {
                cb(null, hasGame
                    ? '"blender.exe","5678","Console","1","500,000 K"\n'
                    : '"explorer.exe","1","Console","1","50,000 K"\n');
            } else {
                cb(null, "30\n");
            }
        });

        guard.start();

        vi.advanceTimersByTime(5000);
        await new Promise(r => process.nextTick(r));
        await new Promise(r => process.nextTick(r));
        expect(guard.isYielded).toBe(true);

        hasGame = false;
        vi.advanceTimersByTime(5000);
        await new Promise(r => process.nextTick(r));
        await new Promise(r => process.nextTick(r));

        expect(reclaimSpy).toHaveBeenCalled();
        expect(guard.isYielded).toBe(false);
    });

    it("should not emit when disabled", async () => {
        const yieldSpy = vi.fn();
        guard.on("yield_vram", yieldSpy);

        mockExec.mockImplementation((cmd: string, opts: any, cb: Function) => {
            cb(null, '"cyberpunk2077.exe","1234","Console","1","500,000 K"\n');
        });

        guard.disable();
        guard.start();
        vi.advanceTimersByTime(5000);
        await new Promise(r => process.nextTick(r));

        expect(yieldSpy).not.toHaveBeenCalled();
    });

    it("should handle exec errors gracefully", async () => {
        mockExec.mockImplementation((cmd: string, opts: any, cb: Function) => {
            cb(new Error("Command failed"), "");
        });

        guard.start();
        vi.advanceTimersByTime(5000);
        await new Promise(r => process.nextTick(r));

        expect(guard.isYielded).toBe(false);
    });

    it("should skip on non-Windows platforms", () => {
        Object.defineProperty(process, "platform", { value: "linux", configurable: true });
        const newGuard = new VRAMGuard();
        newGuard.start();
        expect(guard.isYielded).toBe(false);
        newGuard.dispose();
    });

    it("should report isYielded correctly", () => {
        expect(guard.isYielded).toBe(false);
    });
});
