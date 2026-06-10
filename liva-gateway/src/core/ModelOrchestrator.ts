/* eslint-disable */
import { EventEmitter } from "node:events";
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { logger } from "../utils/logger";
import { safeFetch, withSafeTimeout } from "../utils/HttpClient";
import { ConfigManager } from "./config/ConfigManager";
import { PreemptiveVramMutex, VramLockHandle } from "./PreemptiveVramMutex";
import { VramCostEstimator, TaskType } from "./VramCostEstimator";

/**
 * ModelOrchestrator — Phase 3 Hardware Decoupled Facade
 * =========================================================
 * C++ process spawning, VRAMGuard, and AutoGPUSetup have been
 * moved out to the Python Hardware Resource Daemon.
 * Gateway Node.js is now completely blind to the hardware
 * and only monitors HTTP/gRPC health.
 *
 * [v29] Hot-Swap Architecture: Sequential Single Model on VRAM.
 * - Default: Router model (Gemma 4 E4B) loaded on VRAM.
 * - On handoff_to_expert: Unload Router → Load Expert (26B A4B).
 * - Expert Cooldown TTL: Keep Expert for 3 min, then auto-swap back to Router.
 * - Only 1 model on VRAM at any time.
 */
export class ModelOrchestrator extends EventEmitter {
  #isActive: boolean = false;
  #serverPort: number = 8100;
  #failedPings = 0;
  #pingsExecuted = 0;
  #anomalyMonitorTimer: NodeJS.Timeout | null = null;
  #llamaProcess: any = null;
  #nativeProcess: any = null;
  #isNativeRestarting: boolean = false;
  #isWarmingUp: boolean = false;

  // ── [v29] Hot-Swap State ──
  #currentModelType: "router" | "expert" = "router";
  #isSwapping: boolean = false;
  #expertCooldownTimer: NodeJS.Timeout | null = null;
  readonly #EXPERT_COOLDOWN_MS = Number(process.env.EXPERT_COOLDOWN_MS) || 90_000; // 90s default (env-configurable)

  // VRAM Mutex & Lock Handle
  #vramMutex: PreemptiveVramMutex;
  #activeVramLock: VramLockHandle | null = null;
  #vramMb: number = 8000;

  public get routerPort() {
    return this.#serverPort;
  }
  public get expertPort() {
    return this.#serverPort;
  }
  /** [v29] Which model is currently loaded on VRAM */
  public get currentModelType() {
    return this.#currentModelType;
  }
  /** [v29] Whether a model swap is in progress */
  public get isSwapping() {
    return this.#isSwapping;
  }
  /** [v29] Whether the engine is currently starting up / warming up */
  public get isWarmingUp() {
    return this.#isWarmingUp;
  }

  public get vramMb() {
    return this.#vramMb;
  }

  public get expertGpuLayers() {
    const vram = this.#vramMb;
    if (vram >= 12000) {
      return -1;
    } else if (vram >= 6000) {
      const ratio = (vram - 6000) / 6000;
      return Math.floor(40 * ratio);
    } else {
      return 0;
    }
  }

  public get routerGpuLayers() {
    const vram = this.#vramMb;
    if (vram >= 6000) {
      return -1;
    } else {
      return 0;
    }
  }

  constructor() {
    super();
    // [v27 FIX] Unified env parsing via ConfigManager — Single Source of Truth
    const isNative = ConfigManager.getInstance().isNativeMode;
    this.#serverPort = isNative ? 8100 : 8000;

    let detectedVram = 8000;
    try {
      const _dirname = import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));
      const projectRoot = path.join(_dirname, "../../..");
      const possiblePaths = [
        path.join(projectRoot, "data/hardware_state.json"),
        path.join(process.cwd(), "data/hardware_state.json"),
        path.join(process.cwd(), "liva-gateway/data/hardware_state.json"),
      ];
      let found = false;
      for (const p of possiblePaths) {
        if (fs.existsSync(p)) {
          const content = fs.readFileSync(p, "utf8");
          const parsed = JSON.parse(content);
          if (parsed && typeof parsed.vram_mb === "number") {
            detectedVram = parsed.vram_mb;
            found = true;
            logger.info(`[ModelOrchestrator] Loaded hardware capacity vram_mb = ${detectedVram} from ${p}`);
            break;
          }
        }
      }
      if (!found) {
        logger.info(`[ModelOrchestrator] hardware_state.json not found or invalid. Falling back to 8000 MB.`);
      }
    } catch (err) {
      logger.warn(`[ModelOrchestrator] Error loading hardware state. Falling back to 8000 MB: ${err}`);
    }
    this.#vramMb = detectedVram;
    this.#vramMutex = new PreemptiveVramMutex(this.#vramMb);
  }

  public isReady() {
    return this.#isActive;
  }

  public async startSingleExpert(auth?: any): Promise<void> {
    const isNative = ConfigManager.getInstance().isNativeMode;
    if (isNative) {
      logger.info(
        `[ModelOrchestrator] Native Mode: Checking if Hardware Daemon is already running on port ${this.#serverPort}...`,
      );
      const { NativeIPCClient } = await import("../utils/NativeIPCClient");
      const tempClient = new NativeIPCClient();
      let isRunning = false;
      try {
        isRunning = await withSafeTimeout(
          tempClient.healthCheck(),
          2000,
          "Native_HealthCheck_Timeout",
        );
      } catch (e) {
        // Not running
      } finally {
        tempClient.destroy();
      }

      if (isRunning) {
        logger.info(
          `[ModelOrchestrator] Native Mode: Hardware Daemon is already active.`,
        );
        this.#isActive = true;
      } else {
        logger.info(
          `[ModelOrchestrator] Native Mode: Hardware Daemon is offline. Spawning automatically...`,
        );
        await this.spawnNativeEngine();
      }
      return;
    }

    // --- HTTP llama-server Mode (Zero-Latency Blueprint) ---
    logger.info(
      `[ModelOrchestrator] Spawning HTTP llama-server on port 8000...`,
    );
    const cp = await import("child_process");
    const path = await import("path");
    const fs = await import("fs");

    const isWin = process.platform === "win32";
    const defaultModelsDir = isWin ? "E:\\AI_Models" : path.join(process.env.HOME || "/tmp", "AI_Models");
    const modelsDir = process.env.AI_MODELS_DIR || defaultModelsDir;
    const modelName =
      process.env.EXPERT_MODEL_NAME || "gemma-4-26B-A4B-it-UD-Q6_K.gguf";
    const modelPath = path.join(modelsDir, modelName);

    let exePath = "";
    if (isWin) {
      exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
    } else {
      try {
        const whichOut = cp.execSync("which llama-server", { stdio: "pipe" }).toString().trim();
        if (whichOut) {
          exePath = whichOut;
        }
      } catch {
        exePath = path.join(modelsDir, "llama_bin", "llama-server");
      }
    }

    if (!fs.existsSync(exePath)) {
      logger.error(
        `[ModelOrchestrator] Cannot find llama-server at ${exePath}`,
      );
      return;
    }

    const appConfig = ConfigManager.getInstance().get();
    let draftArgs: string[] = [];
    if (appConfig.LIVA_ENABLE_SPECULATIVE && appConfig.LIVA_DRAFT_MODEL_NAME) {
      const draftModelPath = path.join(modelsDir, appConfig.LIVA_DRAFT_MODEL_NAME);
      if (fs.existsSync(draftModelPath)) {
        draftArgs = ["-md", draftModelPath, "--draft", "5"];
      } else {
        logger.warn(
          `[ModelOrchestrator] Draft model not found at ${draftModelPath}. Running without speculative decoding.`
        );
      }
    }

    // Dynamic thread allocation to prevent E-core thrashing on macOS Apple Silicon
    let threads = 4;
    let threadsBatch = 4;
    const envThreads = process.env.LIVA_THREADS ? parseInt(process.env.LIVA_THREADS, 10) : NaN;
    const envThreadsBatch = process.env.LIVA_THREADS_BATCH ? parseInt(process.env.LIVA_THREADS_BATCH, 10) : NaN;

    if (!isNaN(envThreads) || !isNaN(envThreadsBatch)) {
      if (!isNaN(envThreads)) threads = envThreads;
      if (!isNaN(envThreadsBatch)) threadsBatch = envThreadsBatch;
    } else {
      if (process.platform === "darwin") {
        try {
          const pCoresStr = cp.execSync("sysctl -n hw.perflevel0.physicalcpu", { stdio: "pipe" }).toString().trim();
          const pCores = parseInt(pCoresStr, 10);
          if (!isNaN(pCores) && pCores > 0) {
            threads = pCores;
          } else {
            threads = 4; // Default to 4 performance cores on standard Apple Silicon
          }
        } catch {
          threads = 4;
        }
        try {
          const physCoresStr = cp.execSync("sysctl -n hw.physicalcpu", { stdio: "pipe" }).toString().trim();
          const physCores = parseInt(physCoresStr, 10);
          if (!isNaN(physCores) && physCores > 0) {
            threadsBatch = physCores;
          } else {
            threadsBatch = 8;
          }
        } catch {
          threadsBatch = 8;
        }
      } else {
        // Non-macOS/Darwin systems fallback
        try {
          const os = await import("node:os");
          const cpus = os.cpus();
          threads = Math.max(4, Math.floor(cpus.length / 2));
          threadsBatch = cpus.length;
        } catch {
          threads = 4;
          threadsBatch = 4;
        }
      }
    }

    logger.info(
      `[ModelOrchestrator] Dynamically allocated threads: generation threads (-t) = ${threads}, batch threads (-tb) = ${threadsBatch}`
    );
    const serverArgs = [
      "--host",
      "127.0.0.1",
      "--port",
      String(this.#serverPort),
      "-m",
      modelPath,
      "-c",
      String(ConfigManager.getInstance().contextWindowTokens),
      "-ngl",
      String(this.expertGpuLayers), // Offload layers dynamically to GPU based on VRAM capacity
      "-t",
      String(threads),
      "-tb",
      String(threadsBatch),
      "-b",
      "512", // Optimized batch size for VRAM safety
      "--flash-attn",
      "auto", // Optimized Flash Attention flag
      "--embeddings", // Enable embeddings
      "--pooling",
      "mean", // 🚀 [Zero-Latency] Enable mean pooling for OAI embeddings compatibility
      "--cache-reuse",
      "256", // 🚀 [Zero-Latency] Prompt Caching (Radix Tree)
      "--parallel",
      "2", // 🚀 [Zero-Latency] Isolated Slots (Chat + RAG)
      ...draftArgs,
    ];

    this.#llamaProcess = cp.spawn(exePath, serverArgs, {
      detached: false,
      stdio: ["ignore", "pipe", "pipe"],
    });

    this.#llamaProcess.stdout?.on("data", (data: any) => {
      // logger.debug(`[llama-server] ${data}`);
    });

    this.#llamaProcess.stderr?.on("data", (data: any) => {
      // logger.debug(`[llama-server:err] ${data}`);
    });

    this.#llamaProcess.on("error", (err: any) => {
      logger.error(
        `[ModelOrchestrator] ❌ Lỗi Spawn tiến trình: ${err.message}`,
      );
    });

    this.#llamaProcess.on("exit", () => {
      this.#isActive = false;
    });

    this.#isActive = true;
  }

  private async spawnNativeEngine(): Promise<void> {
    if (this.#nativeProcess) {
      logger.info(
        `[ModelOrchestrator] Native process already exists. Skipping spawn.`,
      );
      return;
    }

    this.#isWarmingUp = true;
    try {
      const path = await import("path");
      const fs = await import("fs");
      const cp = await import("child_process");
      const { fileURLToPath } = await import("node:url");

      const _dirname =
        import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));
      const projectRoot = path.join(_dirname, "../../..");

      const isWin = process.platform === "win32";
      const pythonBin = isWin ? ["venv", "Scripts", "python.exe"] : ["venv", "bin", "python"];
      const pythonExe = path.join(
        projectRoot,
        "liva-ai-engine",
        ...pythonBin
      );
      const engineScript = path.join(
        projectRoot,
        "liva-ai-engine",
        "liva_native_engine.py",
      );
      const workingDir = path.join(projectRoot, "liva-ai-engine");

      logger.info(`[ModelOrchestrator] Resolved Python exe: ${pythonExe}`);
      logger.info(`[ModelOrchestrator] Resolved Engine script: ${engineScript}`);

      if (!fs.existsSync(pythonExe)) {
        logger.error(
          `[ModelOrchestrator] Cannot find Python executable at: ${pythonExe}`,
        );
        return;
      }
      if (!fs.existsSync(engineScript)) {
        logger.error(
          `[ModelOrchestrator] Cannot find engine script at: ${engineScript}`,
        );
        return;
      }

      logger.info(`[ModelOrchestrator] Spawning Python Native Engine...`);
      this.#nativeProcess = cp.spawn(pythonExe, [engineScript], {
        cwd: workingDir,
        detached: false,
        stdio: ["ignore", "pipe", "pipe"],
      });

      this.#nativeProcess.stdout?.on("data", (data: any) => {
        logger.debug(`[NativeEngine] ${data.toString().trim()}`);
      });

      this.#nativeProcess.stderr?.on("data", (data: any) => {
        logger.debug(`[NativeEngine:err] ${data.toString().trim()}`);
      });

      this.#nativeProcess.on("error", (err: any) => {
        logger.error(
          `[ModelOrchestrator] ❌ Failed to spawn Python Native Engine: ${err.message}`,
        );
        this.#isWarmingUp = false;
      });

      this.#nativeProcess.on("exit", (code: any, signal: any) => {
        logger.warn(
          `[ModelOrchestrator] Python Native Engine exited with code ${code} and signal ${signal}`,
        );
        this.#nativeProcess = null;
        this.#isActive = false;
        this.#isWarmingUp = false;

        const isNative = ConfigManager.getInstance().isNativeMode;
        if (isNative && code === 0 && !this.#isNativeRestarting) {
          logger.info(
            `[ModelOrchestrator] Reclaiming VRAM: Clean exit detected. Automatically restarting Python Native Engine...`
          );
          this.handleNativeRestart().catch((err: any) => {
            logger.error(
              `[ModelOrchestrator] Failed to reclaim/restart Python Native Engine: ${err.message}`
            );
          });
        }
      });

      // Wait up to 90 seconds for the service to start
      logger.info(
        `[ModelOrchestrator] Waiting for Python Native Engine to start...`,
      );
      const { NativeIPCClient } = await import("../utils/NativeIPCClient");
      for (let i = 0; i < 90; i++) {
        const tempClient = new NativeIPCClient();
        try {
          const alive = await withSafeTimeout(
            tempClient.healthCheck(),
            1000,
            "Native_HealthCheck_Timeout",
          );
          if (alive) {
            logger.info(
              `[ModelOrchestrator] Python Native Engine successfully started on port ${this.#serverPort}`,
            );
            this.#isActive = true;
            tempClient.destroy();
            return;
          }
        } catch (e) {
          // Ignore, wait and retry
        } finally {
          tempClient.destroy();
        }
        await new Promise((r) => setTimeout(r, 1000));
      }

      logger.error(
        `[ModelOrchestrator] Python Native Engine failed to start after 90 seconds.`,
      );
    } finally {
      this.#isWarmingUp = false;
    }
  }

  private async handleNativeRestart(): Promise<void> {
    if (this.#isNativeRestarting) {
      logger.info(`[ModelOrchestrator] Native restart already in progress.`);
      return;
    }
    this.#isNativeRestarting = true;
    this.#isActive = false;
    this.#isWarmingUp = true;

    try {
      logger.warn(
        `[ModelOrchestrator] Initiating Python Native Engine self-healing and recovery...`,
      );
      this.emit("rewarming_ai");

      const cp = await import("child_process");

      // 1. Terminate old process if any
      if (this.#nativeProcess) {
        logger.info(
          `[ModelOrchestrator] Terminating existing native process...`,
        );
        try {
          if (process.platform === "win32" && this.#nativeProcess.pid) {
            cp.execSync(`taskkill /F /T /PID ${this.#nativeProcess.pid}`);
          } else {
            this.#nativeProcess.kill("SIGKILL");
          }
        } catch (e) {
          // Ignore
        }
        this.#nativeProcess = null;
      }

      // 2. Kill any processes holding port 8100 (Windows specific)
      try {
        const port = 8100;
        logger.info(`[ModelOrchestrator] Cleaning up port ${port}...`);
        if (process.platform === "win32") {
          const stdout = cp
            .execSync(`netstat -ano | findstr LISTENING | findstr :${port}`)
            .toString();
          const lines = stdout.split("\n");
          for (const line of lines) {
            const parts = line.trim().split(/\s+/);
            const pid = parts[parts.length - 1];
            if (pid && /^\d+$/.test(pid) && pid !== "0") {
              logger.info(
                `[ModelOrchestrator] Found PID ${pid} listening on port ${port}, killing it...`,
              );
              cp.execSync(`taskkill /F /T /PID ${pid}`);
            }
          }
        } else {
          try {
            const stdout = cp.execSync(`lsof -t -i:${port}`).toString().trim();
            if (stdout) {
              const pids = stdout.split("\n").map(p => p.trim()).filter(Boolean);
              for (const pid of pids) {
                logger.info(`[ModelOrchestrator] Found PID ${pid} listening on port ${port}, killing it...`);
                cp.execSync(`kill -9 ${pid}`);
              }
            }
          } catch (e) {
            // Ignore if port is not in use or lsof fails
          }
        }
      } catch (e) {
        // Ignore errors if no process is holding the port
      }

      // 3. Wait 1 second
      await new Promise((r) => setTimeout(r, 1000));

      // 4. Spawn new engine
      await this.spawnNativeEngine();

      if (this.#isActive) {
        logger.info(
          `[ModelOrchestrator] Self-healing complete. Native Engine restored.`,
        );
        this.emit("rewarming_complete");
      } else {
        logger.error(
          `[ModelOrchestrator] Self-healing failed to restore Native Engine.`,
        );
      }
    } catch (error: any) {
      logger.error(
        `[ModelOrchestrator] Error during native self-healing: ${error.message}`,
      );
    } finally {
      this.#isNativeRestarting = false;
      this.#isWarmingUp = false;
    }
  }

  public async killLlamaServer(): Promise<void> {
    if (this.#llamaProcess) {
      logger.info(`[ModelOrchestrator] Killing local llama-server...`);
      this.#llamaProcess.kill("SIGKILL");
      this.#llamaProcess = null;
    } else {
      logger.info(
        `[ModelOrchestrator] killLlamaServer requested, but no local process found.`,
      );
    }
    if (this.#nativeProcess) {
      logger.info(`[ModelOrchestrator] Killing Python Native Engine...`);
      try {
        const cp = await import("child_process");
        if (process.platform === "win32" && this.#nativeProcess.pid) {
          cp.execSync(`taskkill /F /T /PID ${this.#nativeProcess.pid}`);
        } else {
          this.#nativeProcess.kill("SIGKILL");
        }
      } catch (e) {
        // Ignore
      }
      this.#nativeProcess = null;
    }
    this.#isActive = false;
  }

  public async restartRouter(): Promise<void> {
    logger.info(
      `[ModelOrchestrator] restartRouter requested. Awaiting Hardware Daemon self-healing...`,
    );
    this.emit("rewarming_ai");
    this.#isActive = true;
  }

  public startAnomalyDetection() {
    if (this.#anomalyMonitorTimer) return;
    logger.info("🛡️ [DevSecOps] Tracking External AI Daemon Health...");

    this.#anomalyMonitorTimer = setInterval(async () => {
      if (this.#serverPort) {
        const isNative = ConfigManager.getInstance().isNativeMode;
        const targetPort = isNative ? 8100 : this.#serverPort;
        const targetUrl = isNative
          ? `http://127.0.0.1:${targetPort}/health`
          : `http://127.0.0.1:${targetPort}/v1/models`;

        this.#pingsExecuted++;
        if (this.#pingsExecuted <= 3) return; // Grace period

        try {
          if (isNative) {
            const { NativeIPCClient } =
              await import("../utils/NativeIPCClient");
            const tempClient = new NativeIPCClient();
            try {
              const alive = await withSafeTimeout(
                tempClient.healthCheck(),
                3000,
                "Native_HealthCheck_Timeout",
              );
              if (!alive) throw new Error("Native gRPC returned alive=false");
            } finally {
              tempClient.destroy();
            }
          } else {
            await safeFetch(targetUrl, {}, 3000);
          }
          this.#failedPings = 0;
          if (!this.#isActive) {
            this.#isActive = true;
            logger.info("🟢 [ModelOrchestrator] Hardware Daemon is ONLINE.");
          }
        } catch (e: unknown) {
          this.#failedPings++;
          if (this.#failedPings >= 3) {
            if (this.#isActive) {
              logger.error(
                "🛑 [DevSecOps] Hardware Daemon OFFLINE or VRAM Yielded. Circuit breaker activated.",
              );
              this.#isActive = false;
              this.emit("anomaly_detected");
              if (isNative) {
                this.handleNativeRestart().catch((err: any) => {
                  logger.error(
                    `[ModelOrchestrator] Error during self-healing: ${err.message}`,
                  );
                });
              }
            }
            this.#failedPings = 0;
          }
        }
      }
    }, 15000);
    this.#anomalyMonitorTimer.unref();
  }

  // ═══════════════════════════════════════════════════
  //  [v29] Hot-Swap Controller — Sequential Single Model
  // ═══════════════════════════════════════════════════

  /**
   * Block and wait until the active swap operation finishes (if any).
   */
  public async waitForSwap(): Promise<void> {
    if (!this.#isSwapping) return;
    return new Promise((resolve) => {
      const onComplete = () => {
        this.off("model_swap_complete", onComplete);
        this.off("model_swap_failed", onFailed);
        resolve();
      };
      const onFailed = () => {
        this.off("model_swap_complete", onComplete);
        this.off("model_swap_failed", onFailed);
        resolve();
      };
      this.on("model_swap_complete", onComplete);
      this.on("model_swap_failed", onFailed);
    });
  }

  /**
   * Swap from Router (E4B) to Expert (26B A4B) via gRPC SwapModel.
   * Blocks until swap is complete. Starts Expert Cooldown TTL after swap.
   * @returns true if swap succeeded, false otherwise.
   */
  public async swapToExpert(): Promise<boolean> {
    if (this.#currentModelType === "expert") {
      logger.info(`[ModelOrchestrator] Already on Expert model. Touching cooldown.`);
      this.touchExpertCooldown();
      return true;
    }
    if (this.#isSwapping) {
      logger.warn(`[ModelOrchestrator] Swap already in progress. Ignoring.`);
      return false;
    }

    this.#isSwapping = true;
    this.#isActive = false;
    this.emit("model_swapping", "expert");

    let lockHandle: VramLockHandle | null = null;
    try {
      // Swap away: release existing lock
      if (this.#activeVramLock) {
        this.#activeVramLock.release();
        this.#activeVramLock = null;
      }

      // Acquire lock for Expert
      const requiredMemory = VramCostEstimator.get(TaskType.LLM_EXPERT);
      lockHandle = await this.#vramMutex.acquireWithGraduation(
        "expert",
        requiredMemory,
        12,
        60000
      );
      this.#activeVramLock = lockHandle;

      // Allow VRAM CUDA garbage collection to settle
      const vramDelay = Number(process.env.VRAM_CLEARANCE_DELAY_MS) || 500;
      if (vramDelay > 0) {
        logger.info(`[ModelOrchestrator] Waiting ${vramDelay}ms for VRAM clearance settling...`);
        await new Promise(resolve => setTimeout(resolve, vramDelay));
      }

      const { NativeIPCClient } = await import("../utils/NativeIPCClient");
      const client = new NativeIPCClient();
      const cfgEnv = ConfigManager.getInstance().env;
      const modelsDir = cfgEnv.AI_MODELS_DIR;
      const expertModel = cfgEnv.EXPERT_MODEL_NAME;
      const modelPath = path.join(modelsDir, expertModel);

      logger.info(`[ModelOrchestrator] 🔄 Swapping to Expert: ${expertModel}...`);

      const expertBackend = process.env.EXPERT_ENGINE_BACKEND || (process.platform === "darwin" ? "mlx" : "llama.cpp");
      const swapTimeoutMs = Number(process.env.MODEL_SWAP_TIMEOUT_MS) || 60000;
      const result = await withSafeTimeout(
        client.swapModel(modelPath, 0, this.expertGpuLayers, expertBackend),
        swapTimeoutMs,
        "MODEL_SWAP_TIMEOUT"
      );
      client.destroy();

      if (result.success) {
        this.#currentModelType = "expert";
        this.#isActive = true;
        logger.info(`[ModelOrchestrator] ✅ Expert loaded: ${result.loadedModel} (${result.swapDurationMs}ms)`);
        this.emit("model_swap_complete", "expert", result.swapDurationMs);
        this.startExpertCooldown();
        return true;
      } else {
        logger.error(`[ModelOrchestrator] ❌ Swap to Expert failed: ${result.errorMessage}`);
        this.emit("model_swap_failed", result.errorMessage);
        // Release lock on swap failure
        if (lockHandle) {
          lockHandle.release();
          if (this.#activeVramLock === lockHandle) {
            this.#activeVramLock = null;
          }
        }
        // Try to recover by swapping back to Router
        this.#isSwapping = false;
        await this.swapToRouter().catch(() => {});
        return false;
      }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[ModelOrchestrator] ❌ Swap to Expert error: ${errMsg}`);
      this.emit("model_swap_failed", errMsg);
      // Release lock on swap error
      if (lockHandle) {
        lockHandle.release();
        if (this.#activeVramLock === lockHandle) {
          this.#activeVramLock = null;
        }
      }
      this.#isSwapping = false;
      await this.swapToRouter().catch(() => {});
      return false;
    } finally {
      this.#isSwapping = false;
    }
  }

  /**
   * Swap from Expert (26B A4B) back to Router (E4B) via gRPC SwapModel.
   * Called by Cooldown TTL or manually when Expert is no longer needed.
   * @returns true if swap succeeded, false otherwise.
   */
  public async swapToRouter(): Promise<boolean> {
    if (this.#currentModelType === "router") {
      logger.info(`[ModelOrchestrator] Already on Router model. No swap needed.`);
      return true;
    }
    if (this.#isSwapping) {
      logger.warn(`[ModelOrchestrator] Swap already in progress. Ignoring.`);
      return false;
    }

    this.clearExpertCooldown();
    this.#isSwapping = true;
    this.#isActive = false;
    this.emit("model_swapping", "router");

    let lockHandle: VramLockHandle | null = null;
    try {
      // Swap away: release existing lock
      if (this.#activeVramLock) {
        this.#activeVramLock.release();
        this.#activeVramLock = null;
      }

      // Acquire lock for Router
      const requiredMemory = VramCostEstimator.get(TaskType.LLM_ROUTER);
      lockHandle = await this.#vramMutex.acquire(
        "router",
        requiredMemory,
        10,
        30000
      );
      this.#activeVramLock = lockHandle;

      // Allow VRAM CUDA garbage collection to settle
      const vramDelay = Number(process.env.VRAM_CLEARANCE_DELAY_MS) || 1000;
      if (vramDelay > 0) {
        logger.info(`[ModelOrchestrator] Waiting ${vramDelay}ms for VRAM clearance settling...`);
        await new Promise(resolve => setTimeout(resolve, vramDelay));
      }

      const { NativeIPCClient } = await import("../utils/NativeIPCClient");
      const client = new NativeIPCClient();
      const cfgEnv = ConfigManager.getInstance().env;
      const modelsDir = cfgEnv.AI_MODELS_DIR;
      // ROUTER_MODEL_NAME is from process.env (not in ConfigManager schema yet)
      const routerModel = process.env.ROUTER_MODEL_NAME || "gemma-4-E4B-it-Q6_K.gguf";
      const modelPath = path.join(modelsDir, routerModel);

      logger.info(`[ModelOrchestrator] 🔄 Swapping back to Router: ${routerModel}...`);

      const routerBackend = process.env.ROUTER_ENGINE_BACKEND || "llama.cpp";
      const swapTimeoutMs = Number(process.env.MODEL_SWAP_TIMEOUT_MS) || 60000;
      const result = await withSafeTimeout(
        client.swapModel(modelPath, 0, this.routerGpuLayers, routerBackend),
        swapTimeoutMs,
        "MODEL_SWAP_TIMEOUT"
      );
      client.destroy();

      if (result.success) {
        this.#currentModelType = "router";
        this.#isActive = true;
        logger.info(`[ModelOrchestrator] ✅ Router restored: ${result.loadedModel} (${result.swapDurationMs}ms)`);
        this.emit("model_swap_complete", "router", result.swapDurationMs);
        return true;
      } else {
        logger.error(`[ModelOrchestrator] ❌ Swap to Router failed: ${result.errorMessage}`);
        this.emit("model_swap_failed", result.errorMessage);
        // Release lock on swap failure
        if (lockHandle) {
          lockHandle.release();
          if (this.#activeVramLock === lockHandle) {
            this.#activeVramLock = null;
          }
        }
        return false;
      }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[ModelOrchestrator] ❌ Swap to Router error: ${errMsg}`);
      this.emit("model_swap_failed", errMsg);
      // Release lock on swap error
      if (lockHandle) {
        lockHandle.release();
        if (this.#activeVramLock === lockHandle) {
          this.#activeVramLock = null;
        }
      }
      return false;
    } finally {
      this.#isSwapping = false;
    }
  }

  /**
   * [v29] Expert Cooldown TTL — Prevents VRAM thrashing.
   * After Expert finishes, keep it loaded for EXPERT_COOLDOWN_MS (3 min).
   * If user sends another message during cooldown, reset the timer.
   * Only swap back to Router when cooldown expires with no interaction.
   */
  private startExpertCooldown(): void {
    this.clearExpertCooldown();
    logger.info(`[ModelOrchestrator] ⏱️ Expert Cooldown TTL started: ${this.#EXPERT_COOLDOWN_MS / 1000}s`);
    this.#expertCooldownTimer = setTimeout(() => {
      logger.info(`[ModelOrchestrator] ⏱️ Expert Cooldown TTL expired. Auto-swapping to Router...`);
      this.swapToRouter().catch((err: unknown) => {
        logger.error(`[ModelOrchestrator] Auto-swap to Router failed: ${err instanceof Error ? err.message : String(err)}`);
      });
    }, this.#EXPERT_COOLDOWN_MS);
    this.#expertCooldownTimer.unref(); // Don't block process exit
  }

  /**
   * [v29] Reset Expert Cooldown TTL — called when user interacts during Expert mode.
   * Prevents premature swap-back while user is actively chatting with Expert.
   */
  public touchExpertCooldown(): void {
    if (this.#currentModelType === "expert" && this.#expertCooldownTimer) {
      logger.info(`[ModelOrchestrator] ⏱️ Expert Cooldown TTL refreshed.`);
      this.startExpertCooldown(); // Restart timer
    }
  }

  private clearExpertCooldown(): void {
    if (this.#expertCooldownTimer) {
      clearTimeout(this.#expertCooldownTimer);
      this.#expertCooldownTimer = null;
    }
  }

  public getStatus() {
    return {
      routerActive: this.#isActive,
      routerPort: this.#serverPort,
      expertActive: this.#isActive,
      expertPort: this.#serverPort,
      currentModelType: this.#currentModelType,
      isSwapping: this.#isSwapping,
    };
  }

  public async dispose() {
    this.clearExpertCooldown();
    if (this.#anomalyMonitorTimer) {
      clearInterval(this.#anomalyMonitorTimer);
      this.#anomalyMonitorTimer = null;
    }
    await this.killLlamaServer();
    this.#isActive = false;
    if (this.#activeVramLock) {
      this.#activeVramLock.release();
      this.#activeVramLock = null;
    }
    this.removeAllListeners();
  }
}
