import path from "path";
import { spawn, ChildProcess } from "child_process";
import treeKill from "tree-kill";
import axios from "axios";
import { logger } from "../utils/logger";

export class ModelOrchestrator {
  private routerProcess: ChildProcess | null = null;
  private expertProcess: ChildProcess | null = null;

  constructor() {
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

  // Tải mô hình Router (Luôn túc trực, chia sẻ tải giữa RAM và VRAM)
  public async startRouter(): Promise<void> {
    if (this.routerProcess) return;

    return new Promise((resolve, reject) => {
      const modelsDir = process.env.AI_MODELS_DIR || "E:\\AI_Models";
      const routerName = process.env.ROUTER_MODEL_NAME || "gemma-4-E4B-it-Q4_K_M.gguf";
      const exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
      const modelPath = path.join(modelsDir, routerName);

      logger.info(`🔥 [Auto-Spawn] Đang đánh thức Router Model (${routerName}) ở nền...`);
      // Đã mở khóa -ngl 99: Đẩy 100% tải của dòng 4B sang GPU để giảm tải hoàn toàn cho CPU
      const args = ["-m", modelPath, "--port", "8000", "-c", "4096", "-ngl", "99"];

      this.routerProcess = spawn(exePath, args, { stdio: "ignore" });

      let isReady = false;
      const healthCheckInterval = setInterval(async () => {
        try {
          const res = await axios.get(`http://127.0.0.1:8000/v1/models`, { timeout: 1000 });
          if (res.status === 200) {
            clearInterval(healthCheckInterval);
            clearTimeout(timeoutTimer);
            isReady = true;
            logger.info("✅ Router Server (Port 8000) đã hoạt động túc trực!");
            resolve();
          }
        } catch (e: any) {
             // Đã fix lỗi nuốt Silent Errors
             logger.debug("Router health check ping fail, retrying: " + e.message);
        }
      }, 500);

      const timeoutTimer = setTimeout(() => {
        if (!isReady) {
          clearInterval(healthCheckInterval);
          this.stopRouter();
          reject(new Error("Timeout (180s) khi khởi động Router Server! Kiểm tra xung đột cổng 8000."));
        }
      }, 180000);

      this.routerProcess.on('exit', (code) => {
        if (!isReady) {
          clearInterval(healthCheckInterval);
          clearTimeout(timeoutTimer);
          this.routerProcess = null;
          reject(new Error(`Router Server crash đột ngột với mã lỗi ${code}`));
        }
      });
    });
  }

  public stopRouter() {
    if (this.routerProcess?.pid) {
      treeKill(this.routerProcess.pid, "SIGKILL");
      this.routerProcess = null;
    }
  }

  // Tải mô hình Chuyên Gia (Chỉ Start trực tiếp lên VRAM khi thực sự cần)
  public async startExpert(): Promise<void> {
    if (this.expertProcess) {
       logger.info("♻️ Expert Model đã tồn tại trên VRAM, dùng lại!");
       return;
    }

    return new Promise((resolve, reject) => {
      const modelsDir = process.env.AI_MODELS_DIR || "E:\\AI_Models";
      const expertName = process.env.EXPERT_MODEL_NAME || "gemma-4-26B-A4B-it-UD-Q4_K_M.gguf";
      const exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
      const modelPath = path.join(modelsDir, expertName);

      logger.info(`🔥 [Handoff] Đang ép toàn bộ Expert Model (${expertName}) lên VRAM...`);
      // -ngl 99 để ép toàn cục lên VRAM cho 26B, -c 8192 để tư duy sâu
      const args = ["-m", modelPath, "--port", "8001", "-c", "8192", "-ngl", "99"];

      this.expertProcess = spawn(exePath, args, { stdio: "ignore" });

      let isReady = false;
      const healthCheckInterval = setInterval(async () => {
        try {
          const res = await axios.get(`http://127.0.0.1:8001/v1/models`, { timeout: 1000 });
          if (res.status === 200) {
            clearInterval(healthCheckInterval);
            clearTimeout(timeoutTimer);
            isReady = true;
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

      this.expertProcess.on('exit', (code) => {
        if (!isReady) {
          clearInterval(healthCheckInterval);
          clearTimeout(timeoutTimer);
          this.expertProcess = null;
          reject(new Error(`Expert Server crash đột ngột với mã lỗi ${code}`));
        }
      });
    });
  }

  public async stopExpert(): Promise<void> {
    return new Promise((resolve) => {
      if (this.expertProcess && this.expertProcess.pid) {
        logger.info("🔪 Đang dập tắt Expert Server, hoàn trả 100% VRAM...");
        treeKill(this.expertProcess.pid, "SIGKILL", (err) => {
          this.expertProcess = null;
          logger.info("♻️ Đã xả VRAM Expert hoàn tất!");
          setTimeout(() => resolve(), 1000);
        });
      } else {
        resolve();
      }
    });
  }
}
