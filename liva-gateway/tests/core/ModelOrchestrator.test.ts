import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ModelOrchestrator } from "../../src/core/ModelOrchestrator";
import { safeFetch, withSafeTimeout } from "../../src/utils/HttpClient";

vi.mock("fs", () => ({
    existsSync: vi.fn().mockReturnValue(true),
}));

vi.mock("child_process", () => ({
    spawn: vi.fn().mockReturnValue({
        stdout: { on: vi.fn() },
        stderr: { on: vi.fn() },
        on: vi.fn(),
        kill: vi.fn()
    }),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
    withSafeTimeout: vi.fn(),
}));

// Mock NativeIPCClient for anomaly detection native path
vi.mock("../../src/utils/NativeIPCClient", () => ({
    NativeIPCClient: class {
        healthCheck = vi.fn().mockResolvedValue(true);
        destroy = vi.fn();
    }
}));

describe("ModelOrchestrator — Hardware Decoupled Facade", () => {
    let orchestrator: ModelOrchestrator;
    const originalEnv = { ...process.env };

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        process.env.LIVA_USE_NATIVE = "false";
        process.env.AI_PROVIDER = "local";
        orchestrator = new ModelOrchestrator();
    });

    afterEach(async () => {
        await orchestrator.dispose();
        process.env = { ...originalEnv };
        vi.useRealTimers();
    });

    describe("startSingleExpert", () => {
        it("should activate immediately (decoupled mode)", async () => {
            await orchestrator.startSingleExpert();
            expect(orchestrator.getStatus().routerActive).toBe(true);
        });

        it("should set port based on LIVA_USE_NATIVE env", () => {
            expect(orchestrator.routerPort).toBe(8000);

            process.env.LIVA_USE_NATIVE = "true";
            const nativeOrch = new ModelOrchestrator();
            expect(nativeOrch.routerPort).toBe(8100);
        });
    });

    describe("killLlamaServer", () => {
        it("should set active to false", async () => {
            await orchestrator.startSingleExpert();
            expect(orchestrator.getStatus().routerActive).toBe(true);

            await orchestrator.killLlamaServer();
            expect(orchestrator.getStatus().routerActive).toBe(false);
        });
    });

    describe("restartRouter", () => {
        it("should emit rewarming_ai and set active", async () => {
            const listener = vi.fn();
            orchestrator.on("rewarming_ai", listener);

            await orchestrator.restartRouter();

            expect(listener).toHaveBeenCalled();
            expect(orchestrator.getStatus().routerActive).toBe(true);
        });
    });

    describe("isReady", () => {
        it("should return false before start", () => {
            expect(orchestrator.isReady()).toBe(false);
        });

        it("should return true after start", async () => {
            await orchestrator.startSingleExpert();
            expect(orchestrator.isReady()).toBe(true);
        });
    });

    describe("getStatus", () => {
        it("should return full status object", async () => {
            await orchestrator.startSingleExpert();
            const status = orchestrator.getStatus();
            expect(status).toEqual({
                routerActive: true,
                routerPort: 8000,
                expertActive: true,
                expertPort: 8000,
            });
        });
    });

    describe("Anomaly Detection", () => {
        it("should not start duplicate monitors", async () => {
            await orchestrator.startSingleExpert();
            orchestrator.startAnomalyDetection();
            orchestrator.startAnomalyDetection(); // second call should be no-op
            // No error thrown = pass
        });

        it("should skip first 3 pings as grace period", async () => {
            await orchestrator.startSingleExpert();
            orchestrator.startAnomalyDetection();

            // First 3 pings should not call safeFetch
            await vi.advanceTimersByTimeAsync(15000);
            await vi.advanceTimersByTimeAsync(15000);
            await vi.advanceTimersByTimeAsync(15000);

            expect(safeFetch).not.toHaveBeenCalled();
        });

        it("should emit anomaly_detected after 3 consecutive failures", async () => {
            await orchestrator.startSingleExpert();
            orchestrator.startAnomalyDetection();

            const anomalyListener = vi.fn();
            orchestrator.on("anomaly_detected", anomalyListener);

            // Skip grace period (3 pings)
            await vi.advanceTimersByTimeAsync(15000);
            await vi.advanceTimersByTimeAsync(15000);
            await vi.advanceTimersByTimeAsync(15000);

            // Now pings will actually run safeFetch
            vi.mocked(safeFetch).mockRejectedValue(new Error("Fail"));

            // Fail 1
            await vi.advanceTimersByTimeAsync(15000);
            // Fail 2
            await vi.advanceTimersByTimeAsync(15000);
            // Fail 3 -> should emit anomaly_detected
            await vi.advanceTimersByTimeAsync(15000);

            expect(anomalyListener).toHaveBeenCalled();
            expect(orchestrator.getStatus().routerActive).toBe(false);
        });

        it("should reset fail count on successful ping", async () => {
            await orchestrator.startSingleExpert();
            orchestrator.startAnomalyDetection();

            const anomalyListener = vi.fn();
            orchestrator.on("anomaly_detected", anomalyListener);

            // Skip grace period
            await vi.advanceTimersByTimeAsync(15000 * 3);

            // Fail 1
            vi.mocked(safeFetch).mockRejectedValueOnce(new Error("Fail 1"));
            await vi.advanceTimersByTimeAsync(15000);

            // Fail 2
            vi.mocked(safeFetch).mockRejectedValueOnce(new Error("Fail 2"));
            await vi.advanceTimersByTimeAsync(15000);

            // Success -> resets counter
            vi.mocked(safeFetch).mockResolvedValueOnce({} as any);
            await vi.advanceTimersByTimeAsync(15000);

            // Fail again (should be counted as 1, not 3)
            vi.mocked(safeFetch).mockRejectedValueOnce(new Error("Fail 3"));
            await vi.advanceTimersByTimeAsync(15000);

            expect(anomalyListener).not.toHaveBeenCalled();
        });
    });

    describe("dispose", () => {
        it("should clean up anomaly timer and set inactive", async () => {
            await orchestrator.startSingleExpert();
            orchestrator.startAnomalyDetection();

            await orchestrator.dispose();

            expect(orchestrator.getStatus().routerActive).toBe(false);
        });
    });
});
