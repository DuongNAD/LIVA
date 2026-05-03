/**
 * FeatureFlags.test.ts — v4.0 Feature Flag System Tests
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { FF } from "../../src/utils/FeatureFlags";

describe("FeatureFlags", () => {
    beforeEach(() => {
        FF.clearAllOverrides();
    });

    afterEach(() => {
        FF.clearAllOverrides();
        // Clean up env vars
        delete process.env.FF_ENABLE_L2_INJECTION;
        delete process.env.FF_ENABLE_ENCRYPTION;
    });

    describe("isEnabled", () => {
        it("should return default value when no env var or override", () => {
            expect(FF.isEnabled("L2_INJECTION")).toBe(false); // default: false
            expect(FF.isEnabled("ENCRYPTION")).toBe(true);    // default: true
        });

        it("should respect environment variable override", () => {
            process.env.FF_ENABLE_L2_INJECTION = "true";
            expect(FF.isEnabled("L2_INJECTION")).toBe(true);

            process.env.FF_ENABLE_L2_INJECTION = "false";
            expect(FF.isEnabled("L2_INJECTION")).toBe(false);
        });

        it("should accept '1' as true for env vars", () => {
            process.env.FF_ENABLE_L2_INJECTION = "1";
            expect(FF.isEnabled("L2_INJECTION")).toBe(true);
        });

        it("should return false for unknown flags", () => {
            expect(FF.isEnabled("NONEXISTENT_FLAG")).toBe(false);
        });
    });

    describe("overrides", () => {
        it("should prioritize runtime overrides over env vars", () => {
            process.env.FF_ENABLE_L2_INJECTION = "false";
            FF.setOverride("L2_INJECTION", true);
            expect(FF.isEnabled("L2_INJECTION")).toBe(true);
        });

        it("should clear a specific override", () => {
            FF.setOverride("L2_INJECTION", true);
            expect(FF.isEnabled("L2_INJECTION")).toBe(true);

            FF.clearOverride("L2_INJECTION");
            expect(FF.isEnabled("L2_INJECTION")).toBe(false); // back to default
        });

        it("should clear all overrides", () => {
            FF.setOverride("L2_INJECTION", true);
            FF.setOverride("ENCRYPTION", false);

            FF.clearAllOverrides();

            expect(FF.isEnabled("L2_INJECTION")).toBe(false);
            expect(FF.isEnabled("ENCRYPTION")).toBe(true);
        });
    });

    describe("getAllFlags", () => {
        it("should return all flags with current state", () => {
            const flags = FF.getAllFlags();
            expect(flags).toHaveProperty("L2_INJECTION");
            expect(flags).toHaveProperty("ENCRYPTION");
            expect(flags).toHaveProperty("TELEMETRY");
            expect(flags.L2_INJECTION.enabled).toBe(false);
            expect(flags.L2_INJECTION.source).toBe("default");
        });

        it("should show correct source for env var", () => {
            process.env.FF_ENABLE_L2_INJECTION = "true";
            const flags = FF.getAllFlags();
            expect(flags.L2_INJECTION.source).toBe("env");
            expect(flags.L2_INJECTION.enabled).toBe(true);
        });

        it("should show correct source for override", () => {
            FF.setOverride("L2_INJECTION", true);
            const flags = FF.getAllFlags();
            expect(flags.L2_INJECTION.source).toBe("override");
        });
    });
});
