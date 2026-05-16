import path from "node:path";
import { promises as fsp, constants as fsc } from "node:fs";
import net from 'node:net';
import { spawn, ChildProcess } from "node:child_process";
import treeKill from "tree-kill";
import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import { safeFetch, withSafeTimeout } from "../utils/HttpClient";

/**
 * [EVOLUTION: BRANDED TYPES]
 * Non-forgeable branded type to authorize specific execution paths.
 * T represents the target state (e.g., 'ROUTER_READY').
 */
export type TaskToken<T extends string> = T & { readonly __brand: unique symbol };

/**
 * [EVOLUTION: CORE KERNEL LOGIC]
 * Internal authority to emit non-forgeable tokens.
 */
const CoreKernel = {
  issueToken<T extends string>(state: T): TaskToken<T> {
    return state as TaskToken<T>;
  }
};

/**
 * [VRAM-AWARE CONFIGURATION]
 * Đọc hardware_state.json và tính toán tham số tối ưu cho llama-server.exe
 */
interface HardwareConfig {
    ngl: string;        // n_gpu_layers
    contextSize: string; // -c context window
    threads: string;     // -t cpu threads
    vram_mb: number;
    ram_mb: number;
    is_battery: boolean;
    gpu_model: string;
}

async function readHardwareConfig(): Promise<HardwareConfig> {
    const defaults: HardwareConfig = { ngl: "99", contextSize: "4096", threads: "4", vram_mb: 0, ram_mb: 16000, is_battery: false, gpu_model: "Unknown" };
    try {
        const statePath = path.join(process.cwd(), "data", "hardware_state.json");
        try { await fsp.access(statePath, fsc.F_OK); } catch { return defaults; }
        
        const raw = await fsp.readFile(statePath, "utf-8");
        const hwState = JSON.parse(raw);
        const vram = hwState.vram_mb || 0;
        const ram = hwState.ram_mb || 16000;
        const cpus = hwState.cpu_threads || 4;
        const isBatt = hwState.is_battery === true;

        const config: HardwareConfig = {
            vram_mb: vram,
            ram_mb: ram,
            is_battery: isBatt,
            gpu_model: hwState.gpu_model || "Unknown",
            ngl: "99",
            contextSize: "4096",
            threads: cpus.toString()
        };

        // --- ADAPTIVE RAM & VRAM ---
        if (vram >= 8192) {
            config.ngl = "99";
            config.contextSize = "8192";
        } else if (vram >= 4096) {
            config.ngl = "99";
            config.contextSize = "4096";
        } else if (vram >= 2048) {
            config.ngl = "20";
            config.contextSize = "2048";
        } else {
            config.ngl = "0";
            config.contextSize = (ram < 16000) ? "2048" : "4096"; // Low End RAM Mode
        }

        // --- ENERGY AWARENESS (Battery Mode) ---
        if (isBatt) {
            // Cut CPU threads in half to save battery
            config.threads = Math.max(1, Math.floor(cpus / 2)).toString();
            logger.warn(`🔋 [EnergyAwareness] Laptop đang dùng Pin! Tự động hạ luồng LLM xuống ${config.threads}/${cpus} threads để tiết kiệm pin.`);
        } else {
            // Full power but leave 1-2 threads for OS
            config.threads = Math.max(1, cpus - 1).toString();
        }

        // --- GPU SYNCHRONIZATION OVERHEAD FIX ---
        // If all layers are offloaded to GPU (-ngl 99), high CPU threads will SEVERELY degrade performance 
        // due to context switching and sync overhead in llama.cpp. Keep it low.
        if (config.ngl === "99") {
            config.threads = Math.min(parseInt(config.threads), 4).toString();
        }

        logger.info(`🎮 [Auto-VRAM] GPU: ${config.gpu_model} | VRAM: ${vram}MB | RAM: ${ram}MB → ngl=${config.ngl}, ctx=${config.contextSize}, threads=${config.threads}`);
        return config;
    } catch {
        logger.debug("[Auto-VRAM] Không đọc được hardware_state.json, dùng mặc định");
        return defaults;
    }
}

/**
 * [DYNAMIC PORT ALLOCATION]
 * Tìm một cổng TCP đang trống trên hệ điều hành để tránh xung đột cổng.
 * Ưu tiên port mặc định (preferred), nếu bị chiếm thì tự tìm port rỗng.
 */
function getAvailablePort(preferred: number): Promise<number> {
    return new Promise((resolve, reject) => {
        const server = net.createServer();
        server.listen(preferred, "127.0.0.1", () => {
            // Port rỗng, dùng luôn
            server.close(() => resolve(preferred));
        });
        server.on("error", () => {
            // Port bị chiếm, tìm port rỗng bất kỳ
            const fallback = net.createServer();
            fallback.listen(0, "127.0.0.1", () => {
                const addr = fallback.address();
                const port = (addr && typeof addr !== "string") ? addr.port : 0;
                fallback.close(() => {
                    if (port > 0) {
                        logger.warn(`⚠️ [Port] Cổng ${preferred} bị chiếm! Đã cấp phát động cổng ${port}.`);
                        resolve(port);
                    } else {
                        reject(new Error("Không thể tìm cổng rỗng!"));
                    }
                });
            });
        });
    });
}

/**
 * ModelOrchestrator — Single Expert Model Architecture (P4)
 * =========================================================
 * Quản lý DUY NHẤT 1 tiến trình llama-server.exe trên 1 port.
 * 100% VRAM dành cho Single Expert. Không còn Dual-Port.
 *
 * Public API:
 *   - startSingleExpert(auth) — Khởi động llama-server với EXPERT_MODEL_NAME
 *   - killLlamaServer()       — Tắt llama-server và giải phóng VRAM
 *   - restartRouter()         — Self-healing: restart khi anomaly detected
 *   - startAnomalyDetection() — Health check định kỳ
 *   - getStatus()             — Trạng thái hiện tại
 *
 * @deprecated methods: startRouter() → alias cho startSingleExpert()
 *                      stopRouter()  → alias cho killLlamaServer()
 */
export class ModelOrchestrator extends EventEmitter {
  /**
   * [SINGLE EXPERT MODEL] Chỉ có DUY NHẤT 1 tiến trình C++ LLM.
   */
  #llamaProcess: ChildProcess | null = null;
  #nativeProcess: ChildProcess | null = null;
  #isActive: boolean = false;

  // [DYNAMIC PORT] Port thực tế được cấp phát — mặc định 8000
  #serverPort: number = 8000;

  /** Port hiện tại của Single Expert Server */
  public get routerPort() { return this.#serverPort; }
  /** @deprecated Alias — không còn Expert Port riêng. Trả về cùng port. */
  public get expertPort() { return this.#serverPort; }

  constructor() {
    super();
    // [GRACEFUL SHUTDOWN] Đăng ký dọn dẹp triệt để ở mọi kịch bản thoát
    const cleanup = () => {
      logger.info("🧹 [Lifecycle] Đang dọn dẹp tiến trình C++ LLM...");
      this.#killProcess(this.#llamaProcess, "SingleExpert");
      this.#llamaProcess = null;
    };

    // Bắt TẤT CẢ kịch bản thoát để không bao giờ để lại zombie
    if (process.env.NODE_ENV !== 'test' && !process.env.VITEST) {
        process.on("exit", cleanup);
        process.on("SIGINT", () => { cleanup(); process.exit(0); });
        process.on("SIGTERM", () => { cleanup(); process.exit(0); });
        // [CRITICAL] uncaughtException: Nếu Gateway crash do lỗi, vẫn phải dọn C++
        process.on("uncaughtException", (err) => {
            logger.error({ err }, "🛑 [FATAL] Uncaught Exception — Đang dọn VRAM trước khi chết:");
            cleanup();
            process.exit(1);
        });
    }
  }

  // --- AI SELF-HEALING PIPELINE ---
  #failedPings = 0;
  #pingsExecuted = 0;
  #anomalyMonitorTimer: NodeJS.Timeout | null = null;

  public startAnomalyDetection() {
      if (this.#anomalyMonitorTimer) return;
      logger.info("🛡️ [DevSecOps] Kích hoạt AI Self-Healing Pipeline (Anomaly Detection)");
      
      this.#anomalyMonitorTimer = setInterval(async () => {
          if (this.#isActive && this.#serverPort) {
              // Nếu đang dùng Native gRPC Engine, ping port 8100/health thay vì v1/models
              const isNative = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";
              const targetPort = isNative ? 8100 : this.#serverPort;
              const targetUrl = isNative ? `http://127.0.0.1:${targetPort}/health` : `http://127.0.0.1:${targetPort}/v1/models`;

              this.#pingsExecuted++;
              // Grace Period 45s (3 nhịp x 15s)
              if (this.#pingsExecuted <= 3) {
                  logger.info("⏳ [Anomaly Detection] Đang chờ AI Engine nạp model (Grace Period)...");
                  return;
              }

              try {
                  if (isNative) {
                      const { NativeIPCClient } = await import("../utils/NativeIPCClient");
                      const tempClient = new NativeIPCClient();
                      try {
                          const alive = await withSafeTimeout(tempClient.healthCheck(), 3000, "Native_HealthCheck_Timeout");
                          if (!alive) throw new Error("Native gRPC returned alive=false");
                      } finally {
                          tempClient.destroy();
                      }
                  } else {
                      // Lightweight health check, timeout 3s for Legacy LLama-server
                      await safeFetch(targetUrl, {}, 3000);
                  }
                  this.#failedPings = 0; // Reset on success
              } catch (e: unknown) {
                  const errMsg = e instanceof Error ? e.message : String(e);
                  this.#failedPings++;
                  logger.warn(`⚠️ [Anomaly Detection] Llama-server không phản hồi (Lỗi ${this.#failedPings}/3)`);
                  if (this.#failedPings >= 3) {
                      logger.error("🛑 [DevSecOps] Phát hiện LLM bị treo/nghẽn VRAM. Kích hoạt RollbackManager...");
                      this.#failedPings = 0;
                      this.emit("anomaly_detected");
                      this.restartRouter(); // Tự phục hồi
                  }
              }
          }
      }, 15000); // Check every 15s
      this.#anomalyMonitorTimer.unref(); // Don't prevent process exit
  }

  public async restartRouter() {
      const isNative = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";

      logger.warn("♻️ [RollbackManager] Đang khởi động lại Single Expert...");
      this.emit("rewarming_ai"); // Báo cho Optimistic UI
      await this.killLlamaServer();
      
      // Delay để nhả VRAM
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      try {
          if (isNative) {
              // [v25 FIX] Native Mode: Must re-spawn the Python gRPC engine.
              // startSingleExpert() in native mode is a no-op (just sets isActive=true),
              // so we must explicitly spawn the process here.
              logger.info("🔄 [RollbackManager] Native mode — Spawning Python Engine...");
              const cp = await import("child_process");
              const engineDir = process.env.AI_ENGINE_DIR
                  || path.join(process.cwd(), "..", "liva-ai-engine");
              const venvPython = process.platform === "win32"
                  ? path.join(engineDir, "venv", "Scripts", "python.exe")
                  : path.join(engineDir, "venv", "bin", "python");
              const engineScript = path.join(engineDir, "liva_native_engine.py");

              this.#nativeProcess = cp.spawn(venvPython, [engineScript], {
                  cwd: engineDir,
                  stdio: ["ignore", "pipe", "pipe"],
                  detached: false,
                  windowsHide: true,
              });

              this.#nativeProcess.stdout?.on("data", (data: Buffer) => {
                  logger.info(`[NativeEngine] ${data.toString().trim()}`);
              });
              this.#nativeProcess.stderr?.on("data", (data: Buffer) => {
                  logger.warn(`[NativeEngine] ${data.toString().trim()}`);
              });
              this.#nativeProcess.on("exit", (code: number | null) => {
                  logger.warn(`[NativeEngine] Process exited with code ${code}`);
                  this.#nativeProcess = null;
              });

              // Wait for gRPC health check (engine needs time to load model into VRAM)
              const { NativeIPCClient } = await import("../utils/NativeIPCClient");
              const tempClient = new NativeIPCClient();
              let engineReady = false;
              for (let attempt = 0; attempt < 15; attempt++) {
                  await new Promise(resolve => setTimeout(resolve, 2000));
                  try {
                      const alive = await tempClient.healthCheck();
                      if (alive) {
                          engineReady = true;
                          break;
                      }
                  } catch { /* engine still loading */ }
                  logger.info(`[RollbackManager] Waiting for Python Engine... (${attempt + 1}/15)`);
              }
              tempClient.destroy();

              if (engineReady) {
                  this.#isActive = true;
                  this.emit("rewarming_complete");
                  logger.info("✅ [RollbackManager] Native Python Engine re-spawned and healthy!");
              } else {
                  logger.error("❌ [RollbackManager] Native Engine failed to start within 30s timeout.");
              }
          } else {
              // Legacy llama-server.exe path
              const auth = CoreKernel.issueToken("ROUTER_START_AUTH");
              await this.startSingleExpert(auth);
              this.emit("rewarming_complete");
              logger.info("✅ [RollbackManager] Single Expert đã được phục hồi thành công!");
          }
      } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          logger.error(`❌ [RollbackManager] Phục hồi thất bại: ${errMsg}`);
      }
  }

  /**
   * [PRIVATE] Tiêu diệt tiến trình C++ bằng tree-kill (giết cả cây con)
   * Sử dụng SIGTERM trước (graceful), fallback SIGKILL (cưỡng chế) sau 3s.
   */
  #killProcess(proc: ChildProcess | null, name: string) {
      if (!proc?.pid) return;
      try {
          logger.info(`🔪 [Lifecycle] Đang gửi SIGTERM cho ${name} (PID: ${proc.pid})...`);
          treeKill(proc.pid, "SIGTERM", (err) => {
              if (err) {
                  // Nếu SIGTERM thất bại, cưỡng chế bằng SIGKILL
                  logger.warn(`⚠️ [Lifecycle] SIGTERM thất bại cho ${name}, dùng SIGKILL...`);
                  try { treeKill(proc.pid!, "SIGKILL"); } catch (e) { /* ignore */ }
              }
          });
      } catch {
          // Last resort
          try { proc.kill("SIGKILL"); } catch { /* ignore */ }
      }
  }

  /**
   * [P4: SINGLE EXPERT MODEL]
   * Khởi động DUY NHẤT 1 llama-server.exe với EXPERT_MODEL_NAME.
   * 100% VRAM dành cho model này. Không swap, không dual-port.
   */
  public async startSingleExpert(auth: TaskToken<"ROUTER_START_AUTH">): Promise<void> {
      // Validate token authenticity via branding check
      if (auth !== "ROUTER_START_AUTH") {
        throw new Error("Unauthorized: Invalid TaskToken for Single Expert transition.");
      }

      if (this.#llamaProcess) return;

      return new Promise((resolve, reject) => {
        (async () => {
        const modelsDir = process.env.AI_MODELS_DIR || "E:\\AI_Models";
        const expertName = process.env.EXPERT_MODEL_NAME || "gemma-4-E2B-it-Q4_K_M.gguf";
        const exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
        const modelPath = path.join(modelsDir, expertName);

        // [ZERO-PYTHON PIVOT] Native IPC mode bypass
        const isNative = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";
        if (isNative) {
            logger.info("✅ Native Engine (IPC:8100) được uỷ quyền bỏ qua Health Check HTTP!");
            this.#isActive = true;
            return resolve();
        }

        // [AUTO-VRAM] Tự động tính toán tham số dựa trên phần cứng
        const hwConfig = await readHardwareConfig();

        // [DYNAMIC PORT] Cấp phát cổng động, ưu tiên 8000
        try {
            this.#serverPort = await getAvailablePort(8000);
            // Broadcast port cho các module-level singletons (LivaEngine.ts)
            process.env.LIVA_ROUTER_PORT = this.#serverPort.toString();
        } catch (e) {
            logger.error("🛑 Không thể cấp phát cổng cho Single Expert!");
            return reject(e);
        }

        logger.info(`🔥 [C++ Native] Khởi động llama-server.exe | Model: ${expertName} | Port: ${this.#serverPort} | ngl=${hwConfig.ngl} | ctx=${hwConfig.contextSize}`);
        const args = [
            "-m", modelPath,
            "--port", this.#serverPort.toString(),
            "-c", hwConfig.contextSize,
            "-ngl", hwConfig.ngl,
            "-t", hwConfig.threads,
            "--host", "127.0.0.1",
            "--parallel", "1",    // Single-user mode: tối ưu throughput cho desktop
            "--cache-reuse", hwConfig.contextSize // Tối đa hoá KV Cache Reuse bằng với Context Size để tránh đọc lại System Prompt
        ];
        this.#llamaProcess = spawn(exePath, args, { 
            stdio: "pipe",
            windowsHide: true 
        });

        
            let stderrLog = "";
            if (this.#llamaProcess.stdout) {
                this.#llamaProcess.stdout.on('data', (data) => {
                    // We log stdout as debug to avoid flooding the info logs, but keep it available
                    logger.debug(`[llama-server:stdout] ${data.toString().trim()}`);
                });
            }
            if (this.#llamaProcess.stderr) {
                this.#llamaProcess.stderr.on('data', (data) => {
                    const msg = data.toString();
                    stderrLog += msg;
                    // Keep only the last 4000 characters to prevent memory bloating
                    if (stderrLog.length > 4000) {
                        stderrLog = stderrLog.substring(stderrLog.length - 4000);
                    }
                    // llama.cpp prints its boot logs (and errors) to stderr, log them to debug
                    logger.debug(`[llama-server:stderr] ${msg.trim()}`);
                });
            }
            this.#llamaProcess.on('error', (err) => {
                logger.error(`[llama-server] ❌ Lỗi Spawn tiến trình: ${err.message}`);
            });
      let isReady = false;
        const healthCheckInterval = setInterval(async () => {
          try {
            await safeFetch(`http://127.0.0.1:${this.#serverPort}/v1/models`, {}, 1000);
            clearInterval(healthCheckInterval);
            clearTimeout(timeoutTimer);
            isReady = true;
            this.#isActive = true;
            logger.info(`✅ Single Expert (Port ${this.#serverPort}) đã sẵn sàng! GPU: ${hwConfig.gpu_model}`);
            resolve();
          } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            const causeMsg = (e instanceof Error && e.cause instanceof Error ? e.cause.message : null) || errMsg || "";
            logger.debug("Single Expert health check ping, retrying: " + causeMsg);
          }
        }, 500);
        healthCheckInterval.unref(); // Don't prevent process exit

        const timeoutTimer = setTimeout(() => {
          if (!isReady) {
            clearInterval(healthCheckInterval);
            this.killLlamaServer();
            logger.warn("⚠️ Timeout khởi động Single Expert (90s)! Model có thể quá nặng cho VRAM.");
            resolve(); // Resolve anyway để UI không bị treo
          }
        }, 90000);

        this.#llamaProcess.on('exit', (code, signal) => {
          if (!isReady) {
            clearInterval(healthCheckInterval);
            clearTimeout(timeoutTimer);
            this.#llamaProcess = null;
            this.#isActive = false;
            
                const exitMsg = `Single Expert crash (code ${code}, signal ${signal}). Kiểm tra model hoặc VRAM.\n[Chi tiết StdErr]:\n${stderrLog.trim()}`;
                logger.error(`🛑 [FATAL] ${exitMsg}`);
                reject(new Error(exitMsg));
              
          } else {
            // Nếu tiến trình chết SAU KHI đã ready (crash runtime)
            
                logger.error(`🛑 [Runtime Crash] Single Expert đã sập bất ngờ (code ${code}, signal ${signal})! VRAM đã được giải phóng.\n[Chi tiết StdErr]:\n${stderrLog.trim()}`);
              
            this.#llamaProcess = null;
            this.#isActive = false;
          }
        });
        })().catch(reject);
      });
  }

  /**
   * [P4] Alias — backward compatibility for callers still using startRouter()
   * @deprecated Use startSingleExpert() instead.
   */
  public async startRouter(auth: TaskToken<"ROUTER_START_AUTH">): Promise<void> {
    return this.startSingleExpert(auth);
  }

  /**
   * [P4: VRAM RELEASE]
   * Tắt llama-server.exe hoặc Python Engine và giải phóng toàn bộ VRAM.
   * [v25] In Native IPC mode, terminates the external Python engine via OS command
   * to free GPU VRAM when VRAMGuard detects a heavy app (game, renderer).
   */
  public async killLlamaServer(): Promise<void> {
    const isNative = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";
    if (isNative) {
        logger.warn("💀 [ModelOrchestrator] VRAMGuard: Terminating Python Native Engine to free VRAM for user!");
        // First: kill the process handle if we spawned it via restartRouter
        if (this.#nativeProcess?.pid) {
            this.#killProcess(this.#nativeProcess, "NativeEngine");
            this.#nativeProcess = null;
        }
        // Also: hunt any externally-spawned process via OS command (start_all.bat)
        try {
            const cp = await import("child_process");
            const cmd = process.platform === "win32"
                ? `wmic process where "commandline like '%liva_native_engine.py%'" call terminate`
                : `pkill -f liva_native_engine.py`;
            cp.exec(cmd, { timeout: 5000, windowsHide: true }, (err) => {
                if (err) logger.debug(`[ModelOrchestrator] Engine kill command returned: ${err.message}`);
            });
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[ModelOrchestrator] Failed to terminate native engine: ${errMsg}`);
        }
        this.#isActive = false;
        return Promise.resolve();
    }

    this.#killProcess(this.#llamaProcess, "SingleExpert");
    this.#llamaProcess = null;
    this.#isActive = false;
    
    return Promise.resolve();
  }

  /**
   * [LIFECYCLE] Dispose orchestrator resources cleanly
   */
  public async dispose(): Promise<void> {
    if (this.#anomalyMonitorTimer) {
        clearInterval(this.#anomalyMonitorTimer);
        this.#anomalyMonitorTimer = null;
    }
    await this.killLlamaServer();
  }

  /**
   * [P4] Alias — backward compatibility for callers still using stopRouter()
   * @deprecated Use killLlamaServer() instead.
   */
  public async stopRouter(): Promise<void> {
    return this.killLlamaServer();
  }

  /**
   * @deprecated Expert is no longer a separate process. Use killLlamaServer().
   * No-op for backward compatibility.
   */
  public async stopExpert(): Promise<void> {
    return Promise.resolve();
  }

  /**
   * [EVOLUTION: STATE VERIFICATION]
   * Publicly check status without exposing process handles.
   */
  public getStatus() {
    return {
      routerActive: this.#isActive,
      expertActive: false, // P4: No separate expert
      routerPort: this.#serverPort,
      expertPort: this.#serverPort // Same port — no dual-port
    };
  }

  public isReady(): boolean {
    return this.#isActive;
  }

  /**
   * [EVOLUTION: TOKEN FACTORY ACCESS]
   * Provides a controlled way to obtain tokens for authorized users.
   */
  public static getAuthorizedTokenFactory() {
    return CoreKernel;
  }
}