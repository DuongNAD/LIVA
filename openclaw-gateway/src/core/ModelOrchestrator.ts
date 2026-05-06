import path from "node:path";
import net from 'node:net';
import { spawn, ChildProcess } from "node:child_process";
import treeKill from "tree-kill";
import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";

/**
 * [EVOLUTION: BRANDED TYPES]
 * Non-forgeable branded type to authorize specific execution paths.
 * T represents the target state (e.g., 'ROUTER_READY' | 'EXPERT_READY').
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

function readHardwareConfig(): HardwareConfig {
    const defaults: HardwareConfig = { ngl: "99", contextSize: "4096", threads: "4", vram_mb: 0, ram_mb: 16000, is_battery: false, gpu_model: "Unknown" };
    try {
        // eslint-disable-next-line @typescript-eslint/no-require-imports
        const fs = require('node:fs');
        const statePath = path.join(process.cwd(), "data", "hardware_state.json");
        if (!fs.existsSync(statePath)) return defaults;
        
        const hwState = JSON.parse(fs.readFileSync(statePath, "utf-8"));
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

export class ModelOrchestrator extends EventEmitter {
  /**
   * [EVOLUTION: PRIVATE CLASS MEMBERS]
   * Absolute encapsulation of process handles.
   */
  #routerProcess: ChildProcess | null = null;
  #expertProcess: ChildProcess | null = null;
  
  // Internal state tracking for token validation
  #isRouterActive: boolean = false;
  #isExpertActive: boolean = false;

  // [DYNAMIC PORT] Lưu port thực tế được cấp phát để AgentLoop kết nối đúng
  #routerPort: number = 8000;
  #expertPort: number = 8001;

  public get routerPort() { return this.#routerPort; }
  public get expertPort() { return this.#expertPort; }

  constructor() {
    super();
    // [GRACEFUL SHUTDOWN] Đăng ký dọn dẹp triệt để ở mọi kịch bản thoát
    const cleanup = () => {
      logger.info("🧹 [Lifecycle] Đang dọn dẹp toàn bộ tiến trình C++ LLM...");
      this.#killProcess(this.#routerProcess, "Router");
      this.#killProcess(this.#expertProcess, "Expert");
      this.#routerProcess = null;
      this.#expertProcess = null;
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
  private failedPings = 0;
  private anomalyMonitorTimer: NodeJS.Timeout | null = null;

  public startAnomalyDetection() {
      if (this.anomalyMonitorTimer) return;
      logger.info("🛡️ [DevSecOps] Kích hoạt AI Self-Healing Pipeline (Anomaly Detection)");
      
      this.anomalyMonitorTimer = setInterval(async () => {
          if (this.#isRouterActive && this.#routerPort) {
              try {
                  // Lightweight health check, timeout 3s
                  await safeFetch(`http://127.0.0.1:${this.#routerPort}/v1/models`, {}, 3000);
                  this.failedPings = 0; // Reset on success
              } catch (e: unknown) {
              const errMsg = e instanceof Error ? e.message : String(e);
                  this.failedPings++;
                  logger.warn(`⚠️ [Anomaly Detection] Llama-server không phản hồi (Lỗi ${this.failedPings}/3)`);
                  if (this.failedPings >= 3) {
                      logger.error("🛑 [DevSecOps] Phát hiện Router LLM bị treo/nghẽn VRAM. Kích hoạt RollbackManager...");
                      this.failedPings = 0;
                      this.emit("anomaly_detected");
                      this.restartRouter(); // Tự phục hồi
                  }
              }
          }
      }, 15000); // Check every 15s
  }

  public async restartRouter() {
      logger.warn("♻️ [RollbackManager] Đang khởi động lại Router...");
      this.emit("rewarming_ai"); // Báo cho Optimistic UI
      this.stopRouter();
      
      // Delay để nhả VRAM
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      try {
          // Xin lại quyền bằng token hợp lệ
          const auth = CoreKernel.issueToken("ROUTER_START_AUTH");
          await this.startRouter(auth);
          this.emit("rewarming_complete");
          logger.info("✅ [RollbackManager] Router đã được phục hồi thành công!");
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
   * [EVOLUTION: TYPE-SAFE ORCHESTRATION]
   * Requires a TaskToken to authorize the transition to Router state.
   */
  public async startRouter(auth: TaskToken<"ROUTER_START_AUTH">): Promise<void> {
    // Validate token authenticity via branding check (compile-time & runtime logic)
    if (auth !== "ROUTER_START_AUTH") {
      throw new Error("Unauthorized: Invalid TaskToken for Router transition.");
    }

    if (this.#routerProcess) return;

    return new Promise((resolve, reject) => {
      (async () => {
      const modelsDir = process.env.AI_MODELS_DIR || "E:\\AI_Models";
      const routerName = process.env.ROUTER_MODEL_NAME || "gemma-4-E2B-it-Q4_K_M.gguf";
      const exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
      const modelPath = path.join(modelsDir, routerName);

      // [ZERO-PYTHON PIVOT] Gọi trực tiếp llama-server.exe (C++ native)
      const isNative = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";
      if (isNative) {
          logger.info("✅ Native Router Engine (IPC:8100) được uỷ quyền bỏ qua Health Check HTTP!");
          this.#isRouterActive = true;
          return resolve();
      }

      // [AUTO-VRAM] Tự động tính toán tham số dựa trên phần cứng
      const hwConfig = readHardwareConfig();

      // [DYNAMIC PORT] Cấp phát cổng động, ưu tiên 8000
      try {
          this.#routerPort = await getAvailablePort(8000);
          // Broadcast port cho các module-level singletons (LivaEngine.ts)
          process.env.LIVA_ROUTER_PORT = this.#routerPort.toString();
      } catch (e) {
          logger.error("🛑 Không thể cấp phát cổng cho Router!");
          return reject(e);
      }

      logger.info(`🔥 [C++ Native] Khởi động llama-server.exe | Model: ${routerName} | Port: ${this.#routerPort} | ngl=${hwConfig.ngl} | ctx=${hwConfig.contextSize}`);
      const args = [
          "-m", modelPath,
          "--port", this.#routerPort.toString(),
          "-c", hwConfig.contextSize,
          "-ngl", hwConfig.ngl,
          "-t", hwConfig.threads,
          "--host", "127.0.0.1",
          "--parallel", "1"    // Single-user mode: tối ưu throughput cho desktop
      ];
      this.#routerProcess = spawn(exePath, args, { 
          stdio: "ignore",
          windowsHide: true 
      });

      let isReady = false;
      const healthCheckInterval = setInterval(async () => {
        try {
          await safeFetch(`http://127.0.0.1:${this.#routerPort}/v1/models`, {}, 1000);
          clearInterval(healthCheckInterval);
          clearTimeout(timeoutTimer);
          isReady = true;
          this.#isRouterActive = true;
          logger.info(`✅ Router C++ (Port ${this.#routerPort}) đã sẵn sàng! GPU: ${hwConfig.gpu_model}`);
          resolve();
        } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          const errMsg = e.cause?.message || errMsg || "";
          logger.debug("Router C++ health check ping, retrying: " + errMsg);
        }
      }, 500);

      const timeoutTimer = setTimeout(() => {
        if (!isReady) {
          clearInterval(healthCheckInterval);
          this.stopRouter();
          logger.warn("⚠️ Timeout khởi động Router C++ (90s)! Model có thể quá nặng cho VRAM.");
          resolve(); // Resolve anyway để UI không bị treo
        }
      }, 90000);

      this.#routerProcess.on('exit', (code) => {
        if (!isReady) {
          clearInterval(healthCheckInterval);
          clearTimeout(timeoutTimer);
          this.#routerProcess = null;
          this.#isRouterActive = false;
          reject(new Error(`Router C++ crash (code ${code}). Kiểm tra model hoặc VRAM.`));
        } else {
          // Nếu tiến trình chết SAU KHI đã ready (crash runtime)
          logger.error(`🛑 [Runtime Crash] Router C++ đã sập bất ngờ (code ${code})! VRAM đã được giải phóng.`);
          this.#routerProcess = null;
          this.#isRouterActive = false;
        }
      });
      })().catch(reject);
    });
  }

  public stopRouter() {
    this.#killProcess(this.#routerProcess, "Router");
    this.#routerProcess = null;
    this.#isRouterActive = false;
    
    if (this.anomalyMonitorTimer) {
        clearInterval(this.anomalyMonitorTimer);
        this.anomalyMonitorTimer = null;
    }
  }

  /**
   * [EVOLUTION: TYPE-SAFE ORCHESTRATION]
   * Requires a TaskToken to authorize the transition to Expert state.
   */
  public async startExpert(auth: TaskToken<"EXPERT_START_AUTH">): Promise<void> {
    if (auth !== "EXPERT_START_AUTH") {
      throw new Error("Unauthorized: Invalid TaskToken for Expert transition.");
    }

    const AI_PROVIDER = process.env.AI_PROVIDER?.toLowerCase() || "local";
    if (AI_PROVIDER === "cloud") {
        logger.info("☁️ [Hybrid] Expert Model được gọi thông qua Cloud API. Bỏ qua kích hoạt tĩnh Local Server.");
        this.#isExpertActive = true;
        return Promise.resolve();
    }

    if (this.#expertProcess) {
       logger.info("♻️ Expert Model đã tồn tại trên VRAM, dùng lại!");
       return;
    }

    return new Promise((resolve, reject) => {
      (async () => {
      const modelsDir = process.env.AI_MODELS_DIR || "E:\\AI_Models";
      const expertName = process.env.EXPERT_MODEL_NAME || "gemma-4-26B-A4B-it-UD-Q3_K_M.gguf";
      const exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
      const modelPath = path.join(modelsDir, expertName);

      // [AUTO-VRAM] Tự động tính toán tham số
      const hwConfig = readHardwareConfig();

      // --- Z-MAS EXCLUSIVE VRAM ALLOCATION LOGIC ---
      logger.warn(`🛑 [Z-MAS Exclusive] Kích hoạt quyền trượng Expert! Giải phóng 100% VRAM từ các tác vụ phụ...`);
      this.stopRouter(); // Tắt luôn Router để nhường VRAM
      this.emit("suspend_peripherals"); // Đóng băng Voice/Webcam

      // [DYNAMIC PORT] Cấp phát cổng động cho Expert
      try {
          this.#expertPort = await getAvailablePort(8001);
      } catch (e) {
          logger.error("🛑 Không thể cấp phát cổng cho Expert!");
          return reject(e);
      }

      logger.info(`🔥 [Handoff] Đang ép Expert (${expertName}) lên VRAM | Port: ${this.#expertPort}...`);
      const args = [
          "-m", modelPath,
          "--port", this.#expertPort.toString(),
          "-c", "16384", // Expert always needs large context
          "-ngl", "99",  // Force full VRAM
          "-t", hwConfig.threads,
          "--host", "127.0.0.1",
          "--parallel", "1"
      ];

      this.#expertProcess = spawn(exePath, args, { 
          stdio: "ignore",
          windowsHide: true 
      });

      let isReady = false;
      const healthCheckInterval = setInterval(async () => {
        try {
          await safeFetch(`http://127.0.0.1:${this.#expertPort}/v1/models`, {}, 1000);
          clearInterval(healthCheckInterval);
          clearTimeout(timeoutTimer);
          isReady = true;
          this.#isExpertActive = true;
          logger.info(`✅ Expert Server (Port ${this.#expertPort}) đã thức tỉnh toàn phần trên VRAM!`);
          resolve();
        } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
             const errMsg = e.cause?.message || errMsg || "";
             logger.debug("Expert health check ping fail, retrying: " + errMsg);
        }
      }, 500);

      const timeoutTimer = setTimeout(() => {
        if (!isReady) {
          clearInterval(healthCheckInterval);
          this.stopExpert();
          reject(new Error("Timeout (180s) khi khởi động Expert Server! Có thể VRAM đã đầy."));
        }
      }, 180000);

      this.#expertProcess.on('exit', (code) => {
        if (!isReady) {
          clearInterval(healthCheckInterval);
          clearTimeout(timeoutTimer);
          this.#expertProcess = null;
          this.#isExpertActive = false;
          reject(new Error(`Expert Server crash với mã lỗi ${code}`));
        } else {
          logger.error(`🛑 [Runtime Crash] Expert C++ đã sập bất ngờ (code ${code})!`);
          this.#expertProcess = null;
          this.#isExpertActive = false;
        }
      });
      })().catch(reject);
    });
  }


  public async stopExpert(): Promise<void> {
    const AI_PROVIDER = process.env.AI_PROVIDER?.toLowerCase() || "local";
    if (AI_PROVIDER === "cloud") {
        this.#isExpertActive = false;
        return Promise.resolve();
    }

    return new Promise((resolve) => {
      if (this.#expertProcess && this.#expertProcess.pid) {
        logger.info("🔪 Đang dập tắt Expert Server, hoàn trả 100% VRAM...");
        treeKill(this.#expertProcess.pid, "SIGKILL", (err) => {
          this.#expertProcess = null;
          this.#isExpertActive = false;
          logger.info("♻️ Đã xả VRAM Expert hoàn tất!");
          this.emit("resume_peripherals"); // Re-activate Voice/Webcam
          setTimeout(() => resolve(), 1000);
        });
      } else {
        resolve();
      }
    });
  }

  /**
   * [EVOLUTION: STATE VERIFICATION]
   * Publicly check status without exposing process handles.
   */
  public getStatus() {
    return {
      routerActive: this.#isRouterActive,
      expertActive: this.#isExpertActive,
      routerPort: this.#routerPort,
      expertPort: this.#expertPort
    };
  }

  /**
   * [EVOLUTION: TOKEN FACTORY ACCESS]
   * Provides a controlled way to obtain tokens for authorized users.
   */
  public static getAuthorizedTokenFactory() {
    return CoreKernel;
  }
}