import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock logger before importing the module
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

import { SkillCircuitBreaker, type CircuitState } from "@core/SkillCircuitBreaker";

describe("SkillCircuitBreaker — Passive Circuit Breaker", () => {
    let cb: SkillCircuitBreaker;

    beforeEach(() => {
        cb = new SkillCircuitBreaker();
    });

    // ============================================================
    // CLOSED state (default)
    // ============================================================
    describe("CLOSED state", () => {
        it("should allow execution by default (never failed)", () => {
            expect(cb.canExecute("some_skill")).toBe(true);
        });

        it("should remain CLOSED after 1 failure", () => {
            cb.recordFailure("some_skill", "Network error");
            expect(cb.canExecute("some_skill")).toBe(true);
        });

        it("should remain CLOSED after 2 failures", () => {
            cb.recordFailure("some_skill", "Error 1");
            cb.recordFailure("some_skill", "Error 2");
            expect(cb.canExecute("some_skill")).toBe(true);
        });
    });

    // ============================================================
    // OPEN state (after 3 consecutive failures)
    // ============================================================
    describe("OPEN state", () => {
        it("should OPEN after 3 consecutive failures", () => {
            cb.recordFailure("dead_skill", "err1");
            cb.recordFailure("dead_skill", "err2");
            cb.recordFailure("dead_skill", "err3");
            expect(cb.canExecute("dead_skill")).toBe(false);
        });

        it("should block execution in OPEN state", () => {
            for (let i = 0; i < 5; i++) cb.recordFailure("broken", `err${i}`);
            expect(cb.canExecute("broken")).toBe(false);
        });

        it("should include OPEN circuit in getOpenCircuits()", () => {
            for (let i = 0; i < 3; i++) cb.recordFailure("broken_api", `err${i}`);
            const open = cb.getOpenCircuits();
            expect(open.has("broken_api")).toBe(true);
        });

        it("should not include CLOSED circuits in getOpenCircuits()", () => {
            cb.recordFailure("flaky", "one error");
            const open = cb.getOpenCircuits();
            expect(open.has("flaky")).toBe(false);
        });
    });

    // ============================================================
    // recordSuccess() — Reset to CLOSED
    // ============================================================
    describe("recordSuccess()", () => {
        it("should reset circuit after success (failures cleared)", () => {
            cb.recordFailure("flaky_skill", "err1");
            cb.recordFailure("flaky_skill", "err2");
            cb.recordSuccess("flaky_skill");
            // Failures reset — need 3 NEW consecutive failures to open again
            cb.recordFailure("flaky_skill", "err3");
            expect(cb.canExecute("flaky_skill")).toBe(true);
        });

        it("should be no-op for skill that never failed", () => {
            // Should not throw
            cb.recordSuccess("never_failed");
            expect(cb.canExecute("never_failed")).toBe(true);
        });
    });

    // ============================================================
    // HALF_OPEN state (after cooldown)
    // ============================================================
    describe("HALF_OPEN state", () => {
        it("should transition to HALF_OPEN after cooldown elapsed", () => {
            for (let i = 0; i < 3; i++) cb.recordFailure("skill_a", `err${i}`);
            expect(cb.canExecute("skill_a")).toBe(false);

            // Fast-forward past 5-minute cooldown
            vi.spyOn(Date, "now").mockReturnValue(Date.now() + 6 * 60 * 1000);

            // Should now be HALF_OPEN → allowed
            expect(cb.canExecute("skill_a")).toBe(true);
        });

        it("should CLOSE on success after HALF_OPEN probe", () => {
            for (let i = 0; i < 3; i++) cb.recordFailure("skill_b", `err${i}`);

            // Fast-forward past cooldown
            vi.spyOn(Date, "now").mockReturnValue(Date.now() + 6 * 60 * 1000);
            expect(cb.canExecute("skill_b")).toBe(true); // Now HALF_OPEN

            cb.recordSuccess("skill_b"); // Probe succeeded
            
            vi.restoreAllMocks();
            // Should be fully CLOSED now
            expect(cb.canExecute("skill_b")).toBe(true);
            const open = cb.getOpenCircuits();
            expect(open.has("skill_b")).toBe(false);
        });

        it("should return to OPEN on failure during HALF_OPEN probe", () => {
            for (let i = 0; i < 3; i++) cb.recordFailure("skill_c", `err${i}`);
            
            vi.spyOn(Date, "now").mockReturnValue(Date.now() + 6 * 60 * 1000);
            expect(cb.canExecute("skill_c")).toBe(true); // HALF_OPEN

            cb.recordFailure("skill_c", "still broken");
            
            vi.restoreAllMocks();
            // Back to OPEN
            expect(cb.canExecute("skill_c")).toBe(false);
        });
    });

    // ============================================================
    // getStatus() — Telemetry
    // ============================================================
    describe("getStatus()", () => {
        it("should return empty array when no circuits tracked", () => {
            expect(cb.getStatus()).toEqual([]);
        });

        it("should return tracked circuits with correct state", () => {
            cb.recordFailure("a", "err_a");
            cb.recordFailure("b", "err_b1");
            cb.recordFailure("b", "err_b2");
            cb.recordFailure("b", "err_b3");

            const status = cb.getStatus();
            expect(status).toHaveLength(2);

            const circuitA = status.find(s => s.name === "a");
            expect(circuitA?.state).toBe("CLOSED");
            expect(circuitA?.failures).toBe(1);
            expect(circuitA?.lastError).toBe("err_a");

            const circuitB = status.find(s => s.name === "b");
            expect(circuitB?.state).toBe("OPEN");
            expect(circuitB?.failures).toBe(3);
        });
    });

    // ============================================================
    // getCircuitError()
    // ============================================================
    describe("getCircuitError()", () => {
        it("should return null for unknown skill", () => {
            expect(cb.getCircuitError("unknown")).toBeNull();
        });

        it("should return null for CLOSED circuit", () => {
            cb.recordFailure("skill", "err");
            cb.recordSuccess("skill");
            expect(cb.getCircuitError("skill")).toBeNull();
        });

        it("should return error message for OPEN circuit", () => {
            for (let i = 0; i < 3; i++) cb.recordFailure("broken", `err${i}`);
            expect(cb.getCircuitError("broken")).toBe("err2");
        });
    });

    // ============================================================
    // Cross-skill isolation
    // ============================================================
    describe("Cross-skill isolation", () => {
        it("should track skills independently", () => {
            for (let i = 0; i < 3; i++) cb.recordFailure("skill_x", `err${i}`);
            cb.recordFailure("skill_y", "one error");

            expect(cb.canExecute("skill_x")).toBe(false); // OPEN
            expect(cb.canExecute("skill_y")).toBe(true);  // CLOSED
        });
    });
});
