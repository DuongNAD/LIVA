import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { ModelOrchestrator } from "../../src/core/ModelOrchestrator";
import { safeFetch, withSafeTimeout } from "../../src/utils/HttpClient";

vi.mock("fs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("fs")>();
  const mockExists = vi.fn().mockImplementation((p: string) => {
    if (typeof p === "string" && p.includes("hardware_state.json")) {
      return actual.existsSync(p);
    }
    return true;
  });
  return {
    ...actual,
    default: {
      ...actual,
      existsSync: mockExists,
    },
    existsSync: mockExists,
  };
});

vi.mock("node:fs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("fs")>();
  const mockExists = vi.fn().mockImplementation((p: string) => {
    if (typeof p === "string" && p.includes("hardware_state.json")) {
      return actual.existsSync(p);
    }
    return true;
  });
  return {
    ...actual,
    default: {
      ...actual,
      existsSync: mockExists,
    },
    existsSync: mockExists,
  };
});

vi.mock("child_process", () => ({
  spawn: vi.fn().mockReturnValue({
    pid: 9999,
    stdout: { on: vi.fn() },
    stderr: { on: vi.fn() },
    on: vi.fn(),
    kill: vi.fn(),
  }),
  execSync: vi.fn().mockReturnValue(Buffer.from("")),
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
  withSafeTimeout: vi.fn().mockImplementation((promise, timeoutMs, errMsg) => {
    return Promise.race([
      promise,
      new Promise((_, reject) => setTimeout(() => reject(new Error(errMsg || "Timeout")), timeoutMs))
    ]);
  }),
}));

// Mock NativeIPCClient for anomaly detection native path
vi.mock("../../src/utils/NativeIPCClient", () => ({
  NativeIPCClient: class {
    healthCheck() {
      return Promise.resolve(true);
    }
    swapModel(modelPath: string, nCtx: number = 0, nGpuLayers: number = -1, backend: string = "") {
      return Promise.resolve({ success: true, loadedModel: "model", swapDurationMs: 100 });
    }
    destroy() {}
  },
}));

// [v27 FIX] Mock ConfigManager singleton — tests control isNativeMode per test case
let mockIsNativeMode = false;
let mockEnableSpeculative = false;
let mockDraftModelName = "";
vi.mock("../../src/core/config/ConfigManager", () => ({
  ConfigManager: {
    getInstance: () => ({
      get isNativeMode() { return mockIsNativeMode; },
      get aiProvider() { return "local"; },
      get contextWindowTokens() { return 8192; },
      get env() { 
        return { 
          LIVA_USE_NATIVE: mockIsNativeMode,
          AI_MODELS_DIR: "/tmp/models",
          EXPERT_MODEL_NAME: "gemma-expert.gguf",
          LIVA_ENABLE_SPECULATIVE: mockEnableSpeculative,
          LIVA_DRAFT_MODEL_NAME: mockDraftModelName,
        }; 
      },
      get() {
        return this.env;
      },
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
    mockEnableSpeculative = false;
    mockDraftModelName = "";
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

    it("should start with speculative decoding when enabled", async () => {
      mockIsNativeMode = false;
      mockEnableSpeculative = true;
      mockDraftModelName = "draft-model.gguf";
      process.env.AI_MODELS_DIR = "/tmp/models";

      const cp = await import("child_process");
      const path = await import("path");
      const spawnSpy = vi.spyOn(cp, "spawn");

      await orchestrator.startSingleExpert();

      expect(spawnSpy).toHaveBeenCalled();
      const spawnArgs = spawnSpy.mock.calls[0][1];
      expect(spawnArgs).toContain("-md");
      expect(spawnArgs).toContain(path.join("/tmp/models", "draft-model.gguf"));
      expect(spawnArgs).toContain("--draft");
      expect(spawnArgs).toContain("5");
    });
    it("should respect LIVA_THREADS and LIVA_THREADS_BATCH overrides when spawning llama-server", async () => {
      mockIsNativeMode = false;
      process.env.LIVA_THREADS = "6";
      process.env.LIVA_THREADS_BATCH = "12";
      process.env.AI_MODELS_DIR = "/tmp/models";

      const cp = await import("child_process");
      const spawnSpy = vi.spyOn(cp, "spawn");

      await orchestrator.startSingleExpert();

      expect(spawnSpy).toHaveBeenCalled();
      const spawnArgs = spawnSpy.mock.calls[0][1];
      
      const threadIdx = spawnArgs.indexOf("-t");
      expect(threadIdx).not.toBe(-1);
      expect(spawnArgs[threadIdx + 1]).toBe("6");

      const threadBatchIdx = spawnArgs.indexOf("-tb");
      expect(threadBatchIdx).not.toBe(-1);
      expect(spawnArgs[threadBatchIdx + 1]).toBe("12");

      delete process.env.LIVA_THREADS;
      delete process.env.LIVA_THREADS_BATCH;
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

    it("should set isWarmingUp during auto-spawning and reset it when complete", async () => {
      const { NativeIPCClient } =
        await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;

      let healthCallCount = 0;
      let wasWarmingUpChecked = false;
      vi.spyOn(proto, "healthCheck").mockImplementation(async () => {
        healthCallCount++;
        if (healthCallCount === 1) {
          return false;
        }
        if (healthCallCount === 2) {
          if (orchestrator.isWarmingUp) {
            wasWarmingUpChecked = true;
          }
          return true;
        }
        return true;
      });

      await orchestrator.startSingleExpert();

      expect(wasWarmingUpChecked).toBe(true);
      expect(orchestrator.isWarmingUp).toBe(false);
      expect(orchestrator.isReady()).toBe(true);
    });

    it("should set isWarmingUp during handleNativeRestart and reset it when complete", async () => {
      const { NativeIPCClient } =
        await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;

      let healthCallCount = 0;
      let wasWarmingUpChecked = false;
      vi.spyOn(proto, "healthCheck").mockImplementation(async () => {
        healthCallCount++;
        if (healthCallCount === 1) {
          if (orchestrator.isWarmingUp) {
            wasWarmingUpChecked = true;
          }
          return false;
        }
        return true;
      });

      await (orchestrator as any).handleNativeRestart();

      expect(wasWarmingUpChecked).toBe(true);
      expect(orchestrator.isWarmingUp).toBe(false);
      expect(orchestrator.isReady()).toBe(true);
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

  describe("Hot-Swap & Robustness Edge Cases (TC-01 to TC-09, TC-12, TC-14 to TC-17)", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("should swap to Expert successfully and start Cooldown TTL (TC-01, TC-03)", async () => {
      const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      const swapSpy = vi.spyOn(proto, "swapModel").mockResolvedValue({
        success: true,
        loadedModel: "expert-model",
        swapDurationMs: 500,
      });

      const swapPromise = orchestrator.swapToExpert();
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(1000); // resolve VRAM delay
      const res = await swapPromise;

      expect(res).toBe(true);
      expect(orchestrator.currentModelType).toBe("expert");
      expect(swapSpy).toHaveBeenCalled();

      // Verify expert cooldown auto-swaps back to router after 90s (TC-03)
      const swapBackSpy = vi.spyOn(orchestrator, "swapToRouter").mockResolvedValue(true);
      await vi.advanceTimersByTimeAsync(90_000);
      expect(swapBackSpy).toHaveBeenCalled();
    });

    it("should swap to Router successfully and clear cooldown (TC-02)", async () => {
      const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      
      // Spy on swapModel to return success
      vi.spyOn(proto, "swapModel").mockResolvedValue({
        success: true,
        loadedModel: "any-model",
        swapDurationMs: 500,
      });

      // Force current state to expert by swapping to expert first
      const swapExpertPromise = orchestrator.swapToExpert();
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(1000);
      await swapExpertPromise;

      expect(orchestrator.currentModelType).toBe("expert");

      // Swap back to Router
      const swapPromise = orchestrator.swapToRouter();
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(1000); // resolve VRAM delay
      const res = await swapPromise;

      expect(res).toBe(true);
      expect(orchestrator.currentModelType).toBe("router");
    });

    it("should touch and extend Expert cooldown TTL (TC-04)", async () => {
      const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      vi.spyOn(proto, "swapModel").mockResolvedValue({
        success: true,
        loadedModel: "expert-model",
        swapDurationMs: 500,
      });

      const swapPromise = orchestrator.swapToExpert();
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(1000);
      await swapPromise;

      const swapBackSpy = vi.spyOn(orchestrator, "swapToRouter").mockResolvedValue(true);

      // Advance 60 seconds (within 90s cooldown)
      await vi.advanceTimersByTimeAsync(60_000);
      expect(swapBackSpy).not.toHaveBeenCalled();

      // Touch cooldown — resets the 90s timer
      orchestrator.touchExpertCooldown();

      // Advance another 60 seconds (total 120s, but only 60s since touch)
      await vi.advanceTimersByTimeAsync(60_000);
      expect(swapBackSpy).not.toHaveBeenCalled();

      // Advance 30 more seconds (90s since touch — should fire)
      await vi.advanceTimersByTimeAsync(30_000);
      expect(swapBackSpy).toHaveBeenCalled();
    });

    it("should block concurrent swap requests (TC-05)", async () => {
      const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      vi.spyOn(proto, "swapModel").mockResolvedValue({
        success: true,
        loadedModel: "expert-model",
        swapDurationMs: 500,
      });

      const swapPromise1 = orchestrator.swapToExpert();
      const swapPromise2 = orchestrator.swapToExpert();

      const res2 = await swapPromise2;
      expect(res2).toBe(false); // Second request blocked

      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(1000);
      const res1 = await swapPromise1;
      expect(res1).toBe(true);
    });

    it("should rollback to Router on Expert swap failure (TC-06)", async () => {
      const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      vi.spyOn(proto, "swapModel").mockResolvedValue({
        success: false,
        errorMessage: "VRAM Allocation Error",
        swapDurationMs: 0,
      });

      const swapRouterSpy = vi.spyOn(orchestrator, "swapToRouter").mockResolvedValue(true);

      const swapPromise = orchestrator.swapToExpert();
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(1000);
      const res = await swapPromise;

      expect(res).toBe(false);
      expect(swapRouterSpy).toHaveBeenCalled();
    });

    it("should respect VRAM clearance delay (TC-14)", async () => {
      process.env.VRAM_CLEARANCE_DELAY_MS = "2000";
      const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      vi.spyOn(proto, "swapModel").mockResolvedValue({
        success: true,
        loadedModel: "expert-model",
        swapDurationMs: 500,
      });

      const swapPromise = orchestrator.swapToExpert();
      await vi.advanceTimersByTimeAsync(0);

      // Check that it's still in progress and hasn't finished at 1000ms
      await vi.advanceTimersByTimeAsync(1000);
      expect(orchestrator.isSwapping).toBe(true);

      // Now resolve at 2000ms
      await vi.advanceTimersByTimeAsync(1000);
      const res = await swapPromise;
      expect(res).toBe(true);

      delete process.env.VRAM_CLEARANCE_DELAY_MS;
    });

    it("should invoke taskkill with tree kill (/F /T) on Windows (TC-12 / TC-15)", async () => {
      const cp = await import("child_process");
      const execSyncSpy = vi.spyOn(cp, "execSync").mockReturnValue(Buffer.from(""));
      
      const originalPlatform = process.platform;
      Object.defineProperty(process, "platform", { value: "win32" });

      mockIsNativeMode = true;
      const nativeOrch = new ModelOrchestrator();

      // Mock NativeIPCClient health check to fail first (offline) and succeed second (online)
      const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
      let healthCount = 0;
      const healthSpy = vi.spyOn(NativeIPCClient.prototype, "healthCheck").mockImplementation(async () => {
        healthCount++;
        return healthCount > 1;
      });

      await nativeOrch.startSingleExpert();

      await nativeOrch.killLlamaServer();

      expect(execSyncSpy).toHaveBeenCalledWith("taskkill /F /T /PID 9999");

      Object.defineProperty(process, "platform", { value: originalPlatform });
      healthSpy.mockRestore();
    });

    it("should handle gRPC model swap I/O timeout and rollback (TC-16)", async () => {
      process.env.MODEL_SWAP_TIMEOUT_MS = "500";
      
      const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      // Mock swapModel to hang indefinitely
      vi.spyOn(proto, "swapModel").mockImplementation(() => new Promise(() => {}));

      const rollbackSpy = vi.spyOn(orchestrator, "swapToRouter").mockResolvedValue(true);

      const swapPromise = orchestrator.swapToExpert();
      await vi.advanceTimersByTimeAsync(0);
      
      // Advance VRAM clearance delay
      await vi.advanceTimersByTimeAsync(1000);
      // Advance timeout limit
      await vi.advanceTimersByTimeAsync(500);

      const res = await swapPromise;

      expect(res).toBe(false);
      expect(orchestrator.isSwapping).toBe(false);
      expect(rollbackSpy).toHaveBeenCalled();

      delete process.env.MODEL_SWAP_TIMEOUT_MS;
    });

    it("should queue interaction requests while swapping and process them when complete (TC-17)", async () => {
      const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      vi.spyOn(proto, "swapModel").mockResolvedValue({
        success: true,
        loadedModel: "expert-model",
        swapDurationMs: 500,
      });

      const swapPromise = orchestrator.swapToExpert();
      await vi.advanceTimersByTimeAsync(0);

      // Simulate 3 incoming messages trying to wait for swap completion
      const executionLogs: string[] = [];
      const msg1 = orchestrator.waitForSwap().then(() => executionLogs.push("msg1"));
      const msg2 = orchestrator.waitForSwap().then(() => executionLogs.push("msg2"));
      const msg3 = orchestrator.waitForSwap().then(() => executionLogs.push("msg3"));

      expect(executionLogs).toEqual([]); // No messages processed yet

      // Settle VRAM delay
      await vi.advanceTimersByTimeAsync(1000);
      await swapPromise;

      await Promise.all([msg1, msg2, msg3]);

      // All messages processed after swap complete
      expect(executionLogs).toEqual(["msg1", "msg2", "msg3"]);
    });
  });

  describe("VRAM and nGpuLayers Dynamic Allocation & Mutex Integration", () => {
    const dataPath = path.join(process.cwd(), "data/hardware_state.json");

    afterEach(() => {
      if (fs.existsSync(dataPath)) {
        try {
          fs.unlinkSync(dataPath);
        } catch (e) {}
      }
    });

    it("should fallback to 8000 MB and Tier 2 dynamic layers if hardware_state.json is missing or invalid", () => {
      if (fs.existsSync(dataPath)) {
        fs.unlinkSync(dataPath);
      }
      const orch = new ModelOrchestrator();
      expect(orch.vramMb).toBe(8000);
      expect(orch.expertGpuLayers).toBe(13); // Math.floor(40 * (8000 - 6000) / 6000) = 13
      expect(orch.routerGpuLayers).toBe(-1); // Tier 2 / 8000 >= 6000 -> -1
    });

    it("should compute correct dynamic layers for Tier 1 (VRAM >= 12GB)", () => {
      fs.writeFileSync(dataPath, JSON.stringify({ vram_mb: 16000 }), "utf8");
      const orch = new ModelOrchestrator();
      expect(orch.vramMb).toBe(16000);
      expect(orch.expertGpuLayers).toBe(-1);
      expect(orch.routerGpuLayers).toBe(-1);
    });

    it("should compute correct dynamic layers for Tier 2 (6GB <= VRAM < 12GB)", () => {
      fs.writeFileSync(dataPath, JSON.stringify({ vram_mb: 9000 }), "utf8");
      const orch = new ModelOrchestrator();
      expect(orch.vramMb).toBe(9000);
      expect(orch.expertGpuLayers).toBe(20); // Math.floor(40 * (9000 - 6000) / 6000) = 20
      expect(orch.routerGpuLayers).toBe(-1);
    });

    it("should compute correct dynamic layers for Tier 3 (VRAM < 6GB / no GPU)", () => {
      fs.writeFileSync(dataPath, JSON.stringify({ vram_mb: 4000 }), "utf8");
      const orch = new ModelOrchestrator();
      expect(orch.vramMb).toBe(4000);
      expect(orch.expertGpuLayers).toBe(0);
      expect(orch.routerGpuLayers).toBe(0);
    });

    it("should acquire and release VRAM locks correctly on swapToExpert and swapToRouter", async () => {
      // Mock NativeIPCClient
      const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
      const proto = NativeIPCClient.prototype;
      vi.spyOn(proto, "swapModel").mockResolvedValue({
        success: true,
        loadedModel: "some-model",
        swapDurationMs: 100,
      });

      // Spy on PreemptiveVramMutex methods via proto
      const { PreemptiveVramMutex } = await import("../../src/core/PreemptiveVramMutex");
      const acquireSpy = vi.spyOn(PreemptiveVramMutex.prototype, "acquire");
      const acquireGradSpy = vi.spyOn(PreemptiveVramMutex.prototype, "acquireWithGraduation");

      fs.writeFileSync(dataPath, JSON.stringify({ vram_mb: 8000 }), "utf8");
      const orch = new ModelOrchestrator();

      // Initially currentModelType is router, let's swap to Expert
      const expertPromise = orch.swapToExpert();
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(1000); // Wait for VRAM settle delay
      const expertRes = await expertPromise;
      expect(expertRes).toBe(true);

      // Expert should use acquireWithGraduation with priority 12 and timeout 60000ms
      expect(acquireGradSpy).toHaveBeenCalledWith("expert", 6700, 12, 60000);

      // Check current model is expert
      expect(orch.currentModelType).toBe("expert");

      // Swap back to Router
      const routerPromise = orch.swapToRouter();
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(1000); // Wait for VRAM settle delay
      const routerRes = await routerPromise;
      expect(routerRes).toBe(true);

      // Router should use normal acquire with priority 10 and timeout 30000ms
      expect(acquireSpy).toHaveBeenCalledWith("router", 5300, 10, 30000);
      expect(orch.currentModelType).toBe("router");
    });
  });
});
