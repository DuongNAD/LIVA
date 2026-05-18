import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const mocks = vi.hoisted(() => ({
    mockExecAsync: vi.fn(),
    mockSpawn: vi.fn(),
    mockSafeFetch: vi.fn()
}));

vi.mock("node:child_process", () => ({
    exec: (cmd: string, cb: any) => cb(null, "stdout", ""),
    spawn: (...args: any[]) => mocks.mockSpawn(...args)
}));

vi.mock("node:util", () => ({
    promisify: () => mocks.mockExecAsync
}));

vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: (...args: any[]) => mocks.mockSafeFetch(...args)
}));

vi.mock("../../src/evolution/EvolutionLogger", () => ({
    evoLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() }
}));

vi.mock("node:fs", () => ({
    existsSync: vi.fn().mockReturnValue(true),
    mkdirSync: vi.fn(),
    openSync: vi.fn().mockReturnValue(1),
    closeSync: vi.fn(),
    createWriteStream: vi.fn()
}));

import { EngineManager, sleep } from "../../src/evolution/EngineManager";

describe("EngineManager", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it("should sleep correctly", async () => {
        const p = sleep(1000);
        vi.advanceTimersByTime(1000);
        await expect(p).resolves.toBeUndefined();
    });

    it("pingUvicorn should return true if fetch succeeds", async () => {
        mocks.mockSafeFetch.mockResolvedValueOnce({ status: 200 });
        const p = EngineManager.pingUvicorn(8000, 1);
        vi.runAllTimers();
        expect(await p).toBe(true);
    });

    it("pingUvicorn should return false if fetch fails", async () => {
        mocks.mockSafeFetch.mockRejectedValue(new Error("Fail"));
        const p = EngineManager.pingUvicorn(8000, 2);
        await vi.runAllTimersAsync();
        expect(await p).toBe(false);
    });

    it("checkPortAvailable should return true if no stdout", async () => {
        mocks.mockExecAsync.mockResolvedValueOnce({ stdout: "" });
        const p = EngineManager.checkPortAvailable(8000);
        vi.runAllTimers();
        expect(await p).toBe(true);
    });

    it("waitForVRAMClear should return if VRAM is below threshold", async () => {
        mocks.mockExecAsync.mockResolvedValueOnce({ stdout: "1024" });
        const p = EngineManager.waitForVRAMClear(2048, 1);
        vi.runAllTimers();
        await expect(p).resolves.toBeUndefined();
    });

    it("startEngineWindows should spawn process", async () => {
        mocks.mockSpawn.mockReturnValue({
            on: vi.fn(),
            unref: vi.fn()
        });
        await EngineManager.startEngineWindows("test.py", ["--arg"]);
        expect(mocks.mockSpawn).toHaveBeenCalled();
    });
});
