import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Must reset singleton between tests — use dynamic import + resetModules
let LlmCircuitBreaker: any;
let cb: any;

describe("LlmCircuitBreaker", () => {
    beforeEach(async () => {
        vi.clearAllMocks();
        vi.resetModules();
        const mod = await import("../../src/core/LlmCircuitBreaker");
        LlmCircuitBreaker = mod.LlmCircuitBreaker;
        cb = LlmCircuitBreaker.getInstance();
    });

    // ─── Singleton ───
    it("should be a singleton", () => {
        const a = LlmCircuitBreaker.getInstance();
        const b = LlmCircuitBreaker.getInstance();
        expect(a).toBe(b);
    });

    // ─── Initial State ───
    it("should allow execution for unknown targets (CLOSED by default)", () => {
        expect(cb.canExecute("gpt-4")).toBe(true);
    });

    // ─── Success Recording ───
    it("should ignore recordSuccess for unknown targets without error", () => {
        expect(() => cb.recordSuccess("nonexistent")).not.toThrow();
    });

    // ─── Failure Accumulation ───
    it("should stay CLOSED after 1-2 failures (below threshold)", () => {
        cb.recordFailure("model-a", "timeout");
        expect(cb.canExecute("model-a")).toBe(true);

        cb.recordFailure("model-a", "connection refused");
        expect(cb.canExecute("model-a")).toBe(true);
    });

    it("should transition to OPEN after 3 consecutive failures (threshold)", () => {
        cb.recordFailure("model-b", "error 1");
        cb.recordFailure("model-b", "error 2");
        cb.recordFailure("model-b", "error 3");

        expect(cb.canExecute("model-b")).toBe(false);
    });

    it("should block execution while OPEN and cooldown has not elapsed", () => {
        for (let i = 0; i < 5; i++) {
            cb.recordFailure("model-c", `error ${i}`);
        }
        expect(cb.canExecute("model-c")).toBe(false);
    });

    // ─── HALF_OPEN Transition ───
    it("should transition to HALF_OPEN after cooldown period (30s)", () => {
        // Record 3 failures to OPEN the circuit
        cb.recordFailure("model-d", "err 1");
        cb.recordFailure("model-d", "err 2");
        cb.recordFailure("model-d", "err 3");
        expect(cb.canExecute("model-d")).toBe(false);

        // Fast-forward past cooldown (30s)
        vi.spyOn(Date, "now").mockReturnValue(Date.now() + 31_000);
        expect(cb.canExecute("model-d")).toBe(true); // Now HALF_OPEN
    });

    // ─── Recovery Path ───
    it("should recover to CLOSED on success after HALF_OPEN", () => {
        cb.recordFailure("model-e", "err 1");
        cb.recordFailure("model-e", "err 2");
        cb.recordFailure("model-e", "err 3");

        // Fast-forward past cooldown
        vi.spyOn(Date, "now").mockReturnValue(Date.now() + 31_000);
        cb.canExecute("model-e"); // Transitions to HALF_OPEN

        // Record success → should recover (delete entry)
        cb.recordSuccess("model-e");

        // Should be fully open again (entry deleted = CLOSED)
        expect(cb.canExecute("model-e")).toBe(true);
    });

    // ─── Independent Targets ───
    it("should track circuits independently per target model", () => {
        // Open circuit for model-f
        cb.recordFailure("model-f", "err 1");
        cb.recordFailure("model-f", "err 2");
        cb.recordFailure("model-f", "err 3");

        // model-f is blocked, model-g is fine
        expect(cb.canExecute("model-f")).toBe(false);
        expect(cb.canExecute("model-g")).toBe(true);
    });

    // ─── Negative: Failure after recovery ───
    it("should re-open circuit if failures continue after HALF_OPEN probe", () => {
        // Open circuit
        cb.recordFailure("model-h", "err 1");
        cb.recordFailure("model-h", "err 2");
        cb.recordFailure("model-h", "err 3");

        // Cooldown → HALF_OPEN
        vi.spyOn(Date, "now").mockReturnValue(Date.now() + 31_000);
        cb.canExecute("model-h"); // HALF_OPEN

        // Probe fails → should re-open
        cb.recordFailure("model-h", "probe failed");
        // Now at 4 failures (>= 3 threshold), should be OPEN again
        vi.spyOn(Date, "now").mockReturnValue(Date.now()); // Reset time
        expect(cb.canExecute("model-h")).toBe(false);
    });
});
