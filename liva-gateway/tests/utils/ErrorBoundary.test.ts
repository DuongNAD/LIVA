import { describe, it, expect, vi, afterEach } from "vitest";

// Must mock logger before importing ErrorBoundary
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

import { installErrorBoundary } from "../../src/utils/ErrorBoundary";
import { logger } from "../../src/utils/logger";

describe("ErrorBoundary", () => {
    afterEach(() => {
        vi.restoreAllMocks();
        process.removeAllListeners("unhandledRejection");
        process.removeAllListeners("uncaughtException");
    });

    it("should install without errors", () => {
        // installErrorBoundary is idempotent — calling it should not throw
        expect(() => installErrorBoundary()).not.toThrow();
    });

    it("should log on initialization", () => {
        installErrorBoundary();
        expect(logger.info).toHaveBeenCalledWith(
            expect.stringContaining("ErrorBoundary")
        );
    });
});
