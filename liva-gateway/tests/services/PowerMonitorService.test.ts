import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// The execAsync inside PowerMonitorService is `promisify(exec)` at module level.
// We need to mock child_process.exec with a callback-based mock that promisify can wrap.
const mockExecImpl = vi.fn();
vi.mock("child_process", () => ({
    exec: (...args: any[]) => mockExecImpl(...args),
}));

// Mock UIController
vi.mock("../../src/core/UIController", () => ({
    UIController: vi.fn(),
}));

import { PowerMonitorService } from "@services/PowerMonitorService";

describe("PowerMonitorService — Battery-Aware Eco Mode", () => {
    let service: PowerMonitorService;
    let mockUi: any;

    function setExecResult(stdout: string) {
        mockExecImpl.mockImplementation((_cmd: string, cb: Function) => {
            cb(null, { stdout, stderr: "" });
        });
    }

    function setExecError(error: Error) {
        mockExecImpl.mockImplementation((_cmd: string, cb: Function) => {
            cb(error);
        });
    }

    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();
        mockUi = { broadcastUIEvent: vi.fn() };
        service = new PowerMonitorService(mockUi);
    });

    afterEach(() => {
        service.stop();
        vi.useRealTimers();
    });

    // ============================================================
    // Constructor
    // ============================================================
    describe("Constructor", () => {
        it("should create without error", () => {
            expect(service).toBeTruthy();
        });
    });

    // ============================================================
    // start() / stop()
    // ============================================================
    describe("start() / stop()", () => {
        it("should start monitoring without error", () => {
            setExecResult("");
            expect(() => service.start(60000)).not.toThrow();
        });

        it("should be idempotent", () => {
            setExecResult("");
            service.start(60000);
            service.start(60000);
        });

        it("should stop without error even if never started", () => {
            expect(() => service.stop()).not.toThrow();
        });

        it("should stop after start", () => {
            setExecResult("");
            service.start(60000);
            expect(() => service.stop()).not.toThrow();
        });
    });

    // ============================================================
    // Eco Mode logic
    // ============================================================
    describe("Eco Mode", () => {
        it("should NOT broadcast when starting in non-eco state with no battery", async () => {
            // isEcoMode starts as false, no battery = updateEcoMode(false) = no-op
            setExecResult("");
            service.start(60000);
            await vi.advanceTimersByTimeAsync(100);

            // No state change → no broadcast
            expect(mockUi.broadcastUIEvent).not.toHaveBeenCalled();
        });

        it("should enable eco mode when discharging (BatteryStatus=1)", async () => {
            setExecResult(JSON.stringify({
                EstimatedChargeRemaining: 85,
                BatteryStatus: 1,
            }));
            service.start(60000);
            await vi.advanceTimersByTimeAsync(100);

            expect(mockUi.broadcastUIEvent).toHaveBeenCalledWith(
                "eco_mode_changed",
                expect.objectContaining({ enabled: true, fps: 5 })
            );
        });

        it("should transition eco→non-eco when plugged in after discharging", async () => {
            // First: discharging → eco enabled
            setExecResult(JSON.stringify({ EstimatedChargeRemaining: 85, BatteryStatus: 1 }));
            service.start(60000);
            await vi.advanceTimersByTimeAsync(100);
            expect(mockUi.broadcastUIEvent).toHaveBeenCalledWith(
                "eco_mode_changed",
                expect.objectContaining({ enabled: true, fps: 5 })
            );

            // Then: plugged in → eco disabled
            setExecResult(JSON.stringify({ EstimatedChargeRemaining: 85, BatteryStatus: 2 }));
            await vi.advanceTimersByTimeAsync(60000);
            expect(mockUi.broadcastUIEvent).toHaveBeenCalledWith(
                "eco_mode_changed",
                expect.objectContaining({ enabled: false, fps: 60 })
            );
        });

        it("should handle array of batteries (uses first)", async () => {
            setExecResult(JSON.stringify([
                { EstimatedChargeRemaining: 10, BatteryStatus: 1 },
                { EstimatedChargeRemaining: 50, BatteryStatus: 2 },
            ]));
            service.start(60000);
            await vi.advanceTimersByTimeAsync(100);

            expect(mockUi.broadcastUIEvent).toHaveBeenCalledWith(
                "eco_mode_changed",
                expect.objectContaining({ enabled: true })
            );
        });

        it("should not broadcast on error if already in non-eco state", async () => {
            setExecError(new Error("PowerShell not found"));
            service.start(60000);
            await vi.advanceTimersByTimeAsync(100);

            // isEcoMode was already false → updateEcoMode(false) is no-op
            expect(mockUi.broadcastUIEvent).not.toHaveBeenCalled();
        });

        it("should not broadcast again if state hasn't changed", async () => {
            setExecResult("");
            service.start(60000);
            await vi.advanceTimersByTimeAsync(100);

            const callCount = mockUi.broadcastUIEvent.mock.calls.length;
            await vi.advanceTimersByTimeAsync(60000);
            expect(mockUi.broadcastUIEvent.mock.calls.length).toBe(callCount);
        });
    });
});
