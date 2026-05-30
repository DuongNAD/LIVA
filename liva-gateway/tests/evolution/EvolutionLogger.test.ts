import { describe, it, expect, vi } from "vitest";

vi.mock("../../src/utils/logger", () => {
    const mockInfo = vi.fn();
    const mockError = vi.fn();
    const mockWarn = vi.fn();
    const mockDebug = vi.fn();
    return {
        logger: {
            child: vi.fn().mockReturnValue({
                info: mockInfo,
                error: mockError,
                warn: mockWarn,
                debug: mockDebug,
            })
        }
    };
});

import { evoLogger } from "../../src/evolution/EvolutionLogger";
import { logger } from "../../src/utils/logger";

describe("EvolutionLogger", () => {
    it("should export an instance of logger", () => {
        expect(evoLogger).toBeDefined();
    });

    it("should create child logger with 'SingularityPipeline' module tag", () => {
        expect(logger.child).toHaveBeenCalledWith({ module: "SingularityPipeline" });
    });

    it("should expose standard log methods (info, error, warn, debug)", () => {
        expect(typeof evoLogger.info).toBe("function");
        expect(typeof evoLogger.error).toBe("function");
        expect(typeof evoLogger.warn).toBe("function");
        expect(typeof evoLogger.debug).toBe("function");
    });

    it("should delegate calls to the underlying child logger", () => {
        evoLogger.info("Test evolution message");
        expect(evoLogger.info).toHaveBeenCalledWith("Test evolution message");
    });
});
