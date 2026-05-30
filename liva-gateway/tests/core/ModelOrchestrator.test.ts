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
    kill: vi.fn(),
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
  withSafeTimeout: vi.fn().mockImplementation((promise) => promise),
}));

// Mock NativeIPCClient for anomaly detection native path
vi.mock("../../src/utils/NativeIPCClient", () => ({
  NativeIPCClient: class {
    healthCheck() {
      return Promise.resolve(true);
    }
    destroy() {}
  },
}));

// [v27 FIX] Mock ConfigManager singleton — tests control isNativeMode per test case
let mockIsNativeMode = false;
vi.mock("../../src/core/config/ConfigManager", () => ({
  ConfigManager: {
    getInstance: () => ({
      get isNativeMode() { return mockIsNativeMode; },
      get aiProvider() { return "local"; },
      get contextWindowTokens() { return 8192; },
      get env() { return { LIVA_USE_NATIVE: mockIsNativeMode }; },
      async getLivaConfig() { return {}; },
    }),
  },
}));

describe("ModelOrchestrator — Hardware Decoupled Facade", () => {
  let orchestrator: ModelOrchestrator;
  const originalEnv = { ...process.env };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    mockIsNativeMode = false;
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

      mockIsNativeMode = true;
      const nativeOrch = new ModelOrchestrator();
      expect(nativeOrch.routerPort).toBe(8100);
    });
  });

  describe("Native Mode Auto-Spawning and Self-Healing", () => {
    beforeEach(() => {
      vi.useRealTimers();
      mockIsNativeMode = true;
      orchestrator = new ModelOrchestrator();
    });

    afterEach(() => {
      vi.useFakeTimers();
    });

    it("should check if Native engine is running and skip spawn if it is active", async () => {
      const { NativeIPCClient } =
        await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      vi.spyOn(proto, "healthCheck").mockResolvedValue(true);

      const cp = await import("child_process");
      const spawnSpy = vi.spyOn(cp, "spawn");

      await orchestrator.startSingleExpert();
      expect(orchestrator.isReady()).toBe(true);
      expect(spawnSpy).not.toHaveBeenCalled();
    });

    it("should spawn Native engine if it is not already running and verify startup", async () => {
      const { NativeIPCClient } =
        await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;

      let healthCallCount = 0;
      vi.spyOn(proto, "healthCheck").mockImplementation(async () => {
        healthCallCount++;
        if (healthCallCount === 1) return false;
        return true;
      });

      const cp = await import("child_process");
      const spawnSpy = vi.spyOn(cp, "spawn");

      await orchestrator.startSingleExpert();

      expect(spawnSpy).toHaveBeenCalled();
      expect(orchestrator.isReady()).toBe(true);
    });

    it("should invoke self-healing (handleNativeRestart) directly and recover", async () => {
      const { NativeIPCClient } =
        await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;

      let healthCallCount = 0;
      vi.spyOn(proto, "healthCheck").mockImplementation(async () => {
        healthCallCount++;
        if (healthCallCount === 1) return false;
        return true;
      });

      const rewarmingListener = vi.fn();
      orchestrator.on("rewarming_ai", rewarmingListener);

      // Call the private method directly
      await (orchestrator as any).handleNativeRestart();

      expect(rewarmingListener).toHaveBeenCalled();
      expect(orchestrator.isReady()).toBe(true);
    });

    it("should trigger handleNativeRestart on anomaly detection", async () => {
      vi.useFakeTimers();

      const { NativeIPCClient } =
        await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      const healthSpy = vi
        .spyOn(proto, "healthCheck")
        .mockResolvedValueOnce(true);
      healthSpy.mockResolvedValue(false);

      // Mock spawnNativeEngine and handleNativeRestart to avoid hanging fake timers
      vi.spyOn(orchestrator as any, "spawnNativeEngine").mockResolvedValue(
        undefined,
      );
      const restartSpy = vi
        .spyOn(orchestrator as any, "handleNativeRestart")
        .mockResolvedValue(undefined);

      await orchestrator.startSingleExpert();
      expect(orchestrator.isReady()).toBe(true);

      orchestrator.startAnomalyDetection();

      // Skip grace period (3 pings)
      await vi.advanceTimersByTimeAsync(15000 * 3);

      // 3 consecutive failures to trigger anomaly detection
      await vi.advanceTimersByTimeAsync(15000);
      await vi.advanceTimersByTimeAsync(15000);
      await vi.advanceTimersByTimeAsync(15000);

      expect(restartSpy).toHaveBeenCalled();

      vi.useRealTimers();
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
        currentModelType: "router",
        isSwapping: false,
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
