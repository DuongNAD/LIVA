import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";

// Must mock logger before importing ErrorBoundary
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

// Reset the module-level `initialized` flag between tests
// by re-importing a fresh module each time
let installErrorBoundary: () => void;

import { logger } from "../../src/utils/logger";

describe("ErrorBoundary", () => {
    beforeEach(async () => {
        vi.clearAllMocks();
        // Reset module to clear the `initialized = true` guard
        vi.resetModules();
        const mod = await import("../../src/utils/ErrorBoundary");
        installErrorBoundary = mod.installErrorBoundary;
    });

    afterEach(() => {
        vi.restoreAllMocks();
        process.removeAllListeners("unhandledRejection");
        process.removeAllListeners("uncaughtException");
    });

    it("should install without errors", () => {
        expect(() => installErrorBoundary()).not.toThrow();
    });

    it("should log on initialization", () => {
        installErrorBoundary();
        expect(logger.info).toHaveBeenCalledWith(
            expect.stringContaining("ErrorBoundary")
        );
    });

    it("should be idempotent — second call should not re-register listeners", () => {
        installErrorBoundary();
        const infoCallCount = vi.mocked(logger.info).mock.calls.length;

        installErrorBoundary(); // second call
        // Should NOT log again (guarded by `initialized`)
        expect(vi.mocked(logger.info).mock.calls.length).toBe(infoCallCount);
    });

    it("should catch unhandledRejection and log via logger.error", () => {
        installErrorBoundary();

        const testError = new Error("Test rejection");
        process.emit("unhandledRejection", testError, Promise.resolve());

        expect(logger.error).toHaveBeenCalledWith(
            expect.objectContaining({ context: "ErrorBoundary" }),
            expect.stringContaining("Unhandled Promise Rejection")
        );
    });

    it("should handle non-Error rejection reasons gracefully", () => {
        installErrorBoundary();

        // String rejection (not an Error instance)
        process.emit("unhandledRejection", "string reason" as any, Promise.resolve());

        expect(logger.error).toHaveBeenCalledWith(
            expect.objectContaining({ err: "string reason" }),
            expect.stringContaining("Unhandled Promise Rejection")
        );
    });

    it("should catch uncaughtException and log via logger.error", () => {
        installErrorBoundary();

        const testError = new Error("Test uncaught");
        process.emit("uncaughtException", testError);

        expect(logger.error).toHaveBeenCalledWith(
            expect.objectContaining({ context: "ErrorBoundary" }),
            expect.stringContaining("Uncaught Exception")
        );
    });

    it("should call process.exit(1) for fatal OOM errors", () => {
        installErrorBoundary();

        const oomError = new Error("JavaScript heap out of memory");
        // setup.ts mocks process.exit to throw — we verify exit WAS triggered
        expect(() => process.emit("uncaughtException", oomError)).toThrow();
    });

    it("should NOT call process.exit for regular uncaught exceptions", () => {
        installErrorBoundary();

        const regularError = new Error("Some random bug");
        // Should NOT trigger process.exit (which would throw in test env)
        expect(() => process.emit("uncaughtException", regularError)).not.toThrow();
    });
});
