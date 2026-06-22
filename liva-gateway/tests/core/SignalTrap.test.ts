import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { SignalTrap } from "../../src/core/kernel/SignalTrap";
import { CoreKernel } from "../../src/core/CoreKernel";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
        child: vi.fn().mockReturnThis()
    },
}));

// Mock CoreKernel
vi.mock("../../src/core/CoreKernel", () => {
    return {
        CoreKernel: class {
            shutdown = vi.fn().mockResolvedValue(undefined);
        }
    };
});

describe("SignalTrap", () => {
    let processOnSpy: any;
    let processExitSpy: any;
    let stdinOnSpy: any;
    let stdinResumeSpy: any;

    beforeEach(() => {
        vi.clearAllMocks();
        processOnSpy = vi.spyOn(process, "on").mockImplementation(() => process as any);
        processExitSpy = vi.spyOn(process, "exit").mockImplementation((code?: number) => {
            throw new Error(`process.exit: ${code}`);
        });
        stdinOnSpy = vi.spyOn(process.stdin, "on").mockImplementation(() => process.stdin as any);
        stdinResumeSpy = vi.spyOn(process.stdin, "resume").mockImplementation(() => process.stdin as any);
    });

    afterEach(() => {
        processOnSpy.mockRestore();
        processExitSpy.mockRestore();
        stdinOnSpy.mockRestore();
        stdinResumeSpy.mockRestore();
    });

    it("should register event listeners on process and stdin", () => {
        const kernel = new CoreKernel();
        SignalTrap.listen(kernel);

        expect(stdinResumeSpy).toHaveBeenCalled();
        expect(stdinOnSpy).toHaveBeenCalledWith("end", expect.any(Function));
        expect(processOnSpy).toHaveBeenCalledWith("SIGINT", expect.any(Function));
        expect(processOnSpy).toHaveBeenCalledWith("SIGTERM", expect.any(Function));
    });

    it("should execute graceful shutdown on SIGINT", async () => {
        const kernel = new CoreKernel();
        SignalTrap.listen(kernel);

        const sigintCall = processOnSpy.mock.calls.find((call: any[]) => call[0] === "SIGINT");
        expect(sigintCall).toBeDefined();
        const handler = sigintCall[1];

        // Create a promise that resolves when process.exit is called
        const exitPromise = new Promise<void>((resolve, reject) => {
            processExitSpy.mockImplementation((code: number) => {
                if (code === 0) resolve();
                else reject(new Error(`Exit code ${code}`));
                // We don't throw to prevent unhandled rejection crashes, or we throw but catch it
            });
        });

        handler();

        await exitPromise;
        expect(kernel.shutdown).toHaveBeenCalled();
        expect(processExitSpy).toHaveBeenCalledWith(0);
    });

    it("should execute graceful shutdown on SIGTERM", async () => {
        const kernel = new CoreKernel();
        SignalTrap.listen(kernel);

        const sigtermCall = processOnSpy.mock.calls.find((call: any[]) => call[0] === "SIGTERM");
        expect(sigtermCall).toBeDefined();
        const handler = sigtermCall[1];

        const exitPromise = new Promise<void>((resolve, reject) => {
            processExitSpy.mockImplementation((code: number) => {
                if (code === 0) resolve();
                else reject(new Error(`Exit code ${code}`));
            });
        });

        handler();

        await exitPromise;
        expect(kernel.shutdown).toHaveBeenCalled();
        expect(processExitSpy).toHaveBeenCalledWith(0);
    });

    it("should execute graceful shutdown on stdin end", async () => {
        const kernel = new CoreKernel();
        SignalTrap.listen(kernel);

        const endCall = stdinOnSpy.mock.calls.find((call: any[]) => call[0] === "end");
        expect(endCall).toBeDefined();
        const handler = endCall[1];

        const exitPromise = new Promise<void>((resolve, reject) => {
            processExitSpy.mockImplementation((code: number) => {
                if (code === 0) resolve();
                else reject(new Error(`Exit code ${code}`));
            });
        });

        handler();

        await exitPromise;
        expect(kernel.shutdown).toHaveBeenCalled();
        expect(processExitSpy).toHaveBeenCalledWith(0);
    });
});
