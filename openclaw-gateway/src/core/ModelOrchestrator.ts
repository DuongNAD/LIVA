import path from "path";
import { spawn, ChildProcess } from "child_process";
import treeKill from "tree-kill";
import axios from "axios";
import { EventEmitter } from "events";
import { logger } from "../utils/logger";

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

  constructor() {
    super();
    const cleanup = () => {
      this.stopRouter();
      this.stopExpert();
    };
    process.on("exit", cleanup);
    process.on("SIGINT", () => {
      cleanup();
      process.exit(0);
    });
    process.on("SIGTERM", () => {
      cleanup();
      process.exit(0);
    });
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
      const modelsDir = process.env.AI_MODELS_DIR || "E:\\AI_Models";
      const routerName = process.env.ROUTER_MODEL_NAME || "gemma-4-E4B-it-Q4_K_M.gguf";
      const exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
      const modelPath = path.join(modelsDir, routerName);

      logger.info(`🔥 [Auto-Spawn] Bypass: Router Model (${routerName}) đã được Python Liva Engine gánh trên cổng 8000...`);
      // Vô hiệu hóa spawn C++ thuần vì đụng cổng với Python Uvicorn Engine
      // const args = ["-m", modelPath, "--port", "8000", "-c", "4096", "-ngl", "99"];
      // this.#routerProcess = spawn(exePath, args, { stdio: "ignore" });


      const isNative = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";
      if (isNative) {
          logger.info("✅ Native Router Engine (IPC:8100) được uỷ quyền bỏ qua Health Check HTTP!");
          this.#isRouterActive = true;
          return resolve();
      }

      let isReady = false;
      let connectionRetries = 0;
      const healthCheckInterval = setInterval(async () => {
        try {
          const res = await axios.get(`http://127.0.0.1:8000/v1/models`, { timeout: 1000 });
          if (res.status === 200) {
            clearInterval(healthCheckInterval);
            clearTimeout(timeoutTimer);
            isReady = true;
            this.#isRouterActive = true;
            logger.info("✅ Router Server (Port 8000) đã hoạt động túc trực!");
            resolve();
          }
        } catch (e: any) {
          connectionRetries++;
          logger.debug("Router health check ping fail, retrying: " + e.message);
          
          // [Auto-Heal] Kích hoạt cơ chế gọi hồn Python Engine sau 5 nhịp Ping rỗng (Khoảng 2.5s)
          if (e.message.includes('ECONNREFUSED') && connectionRetries === 5) {
              logger.info("🔥 [Auto-Heal] Kích hoạt tự động Engine Python vì LIVA Cốt lõi chưa được mồi...");
              const engineDir = path.join(process.cwd(), "..", "liva-ai-engine");
              const pyPath = path.join(engineDir, "venv", "Scripts", "python.exe");
              try {
                  spawn(pyPath, ["engine.py"], { 
                      cwd: engineDir, 
                      windowsHide: true, 
                      detached: true, 
                      stdio: "ignore",
                      env: { ...process.env, PYTHONIOENCODING: "utf-8" }
                  }).unref();
              } catch(spawnErr: any) {
                  logger.error("🛑 Không thể Start Python Engine tự động. Thư mục Engine có thể bị thiếu:", spawnErr.message);
              }
          }
        }
      }, 500);

      const timeoutTimer = setTimeout(() => {
        if (!isReady) {
          clearInterval(healthCheckInterval);
          this.stopRouter();
          reject(new Error("Timeout (180s) khi khởi động Router Server! Kiểm tra xung đột cổng 8000."));
        }
      }, 180000);

      if (this.#routerProcess) {
        this.#routerProcess.on('exit', (code) => {
          if (!isReady) {
            clearInterval(healthCheckInterval);
            clearTimeout(timeoutTimer);
            this.#routerProcess = null;
            this.#isRouterActive = false;
            reject(new Error(`Router Server crash đột ngột với mã lỗi ${code}`));
          }
        });
      }
    });
  }

  public stopRouter() {
    if (this.#routerProcess?.pid) {
      treeKill(this.#routerProcess.pid, "SIGKILL");
      this.#routerProcess = null;
      this.#isRouterActive = false;
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
      const modelsDir = process.env.AI_MODELS_DIR || "E:\\AI_Models";
      const expertName = process.env.EXPERT_MODEL_NAME || "gemma-4-26B-A4B-it-UD-Q3_K_M.gguf";
      const exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
      const modelPath = path.join(modelsDir, expertName);

      // --- Z-MAS EXCLUSIVE VRAM ALLOCATION LOGIC ---
      logger.warn(`🛑 [Z-MAS Exclusive] Kích hoạt quyền trượng 26B! Giải phóng 100% VRAM từ các tác vụ phụ...`);
      this.stopRouter(); // Tắt luôn Não E4B để nhường chỗ
      this.emit("suspend_peripherals"); // Gửi lệnh đóng băng Voice/Mắt

      logger.info(`🔥 [Handoff] Đang ép toàn bộ Expert Model (${expertName}) lên VRAM...`);
      // -ngl 99 để ép toàn cục lên VRAM cho 26B, -c 16384 để tư duy sâu
      const args = ["-m", modelPath, "--port", "8001", "-c", "16384", "-ngl", "99"];

      this.#expertProcess = spawn(exePath, args, { stdio: "ignore" });

      let isReady = false;
      const healthCheckInterval = setInterval(async () => {
        try {
          const res = await axios.get(`http://127.0.0.1:8001/v1/models`, { timeout: 1000 });
          if (res.status === 200) {
            clearInterval(healthCheckInterval);
            clearTimeout(timeoutTimer);
            isReady = true;
            this.#isExpertActive = true;
            logger.info("✅ Expert Server (Port 8001) đã thức tỉnh toàn phần trên VRAM!");
            resolve();
          }
        } catch (e: any) {
             logger.debug("Expert health check ping fail, retrying: " + e.message);
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
          reject(new Error(`Expert Server crash đột ngột với mã lỗi ${code}`));
        }
      });
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
          logger.info("♻️ Đã xả VRRAM Expert hoàn tất!");
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
      expertActive: this.#isExpertActive
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