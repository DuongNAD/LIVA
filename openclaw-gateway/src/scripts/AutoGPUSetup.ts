import { exec } from "child_process";
import * as path from "path";
import * as fs from "fs";
import { logger } from "../utils/logger";

interface HardwareState {
    gpu_model: string;
    cuda_version: string;
    vram_mb: number;
    llama_server_ok: boolean;
    status: string;
}

/**
 * AutoGPUSetup — Smart Hardware Detection & Validation
 * 
 * Thay vì cố pip install Python wheels (bất khả thi trên Python 3.13),
 * Script này kiểm tra tính toàn vẹn của file llama-server.exe (C++ native)
 * và phát hiện GPU NVIDIA để cấu hình tối ưu n_gpu_layers.
 */
export class AutoGPUSetup {
    private static getStateFilePath(): string {
        return path.join(process.cwd(), "data", "hardware_state.json");
    }

    private static readHardwareState(): HardwareState | null {
        try {
            const file = this.getStateFilePath();
            if (fs.existsSync(file)) {
                return JSON.parse(fs.readFileSync(file, "utf-8"));
            }
        } catch (e) {
            // Ignore, treat as first run
        }
        return null;
    }

    private static saveHardwareState(state: HardwareState) {
        try {
            const dataDir = path.join(process.cwd(), "data");
            if (!fs.existsSync(dataDir)) fs.mkdirSync(dataDir, { recursive: true });
            fs.writeFileSync(this.getStateFilePath(), JSON.stringify(state, null, 2), "utf-8");
        } catch (e) {
            logger.error("Không thể lưu hardware_state.json:", e);
        }
    }

    private static execPromise(command: string): Promise<string> {
        return new Promise((resolve, reject) => {
            exec(command, { timeout: 5000 }, (error, stdout, stderr) => {
                if (error) {
                    reject(error);
                    return;
                }
                resolve(stdout ? stdout.trim() : stderr.trim());
            });
        });
    }

    private static async getNvidiaInfo(): Promise<{ model: string; cuda: string; vram_mb: number } | null> {
        try {
            const gpuOut = await this.execPromise("nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits");
            const firstLine = gpuOut.split("\n")[0].trim();
            const parts = firstLine.split(",").map(s => s.trim());
            if (parts.length < 2) return null;

            const model = parts[0];
            const vram_mb = parseInt(parts[1]) || 0;

            const smiOut = await this.execPromise("nvidia-smi");
            const cudaMatch = smiOut.match(/CUDA Version:\s*(\d+\.\d+)/);
            const cuda = cudaMatch ? cudaMatch[1] : "N/A";

            return { model, cuda, vram_mb };
        } catch (e) {
            return null; // No NVIDIA GPU or drivers not installed
        }
    }

    public static async runAutoSetupIfNeeded(onProgress: (msg: string) => void): Promise<void> {
        try {
            onProgress("Đang kiểm tra phần cứng AI...");

            // 1. Kiểm tra sự tồn tại của llama-server.exe
            const modelsDir = process.env.AI_MODELS_DIR || "E:\\AI_Models";
            const exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
            const modelName = process.env.ROUTER_MODEL_NAME || "gemma-4-E2B-it-Q4_K_M.gguf";
            const modelPath = path.join(modelsDir, modelName);

            if (!fs.existsSync(exePath)) {
                logger.error(`🛑 [AutoGPU] Không tìm thấy llama-server.exe tại: ${exePath}`);
                onProgress("⚠️ Thiếu file llama-server.exe. Vui lòng kiểm tra thư mục AI_Models.");
                return;
            }

            if (!fs.existsSync(modelPath)) {
                logger.error(`🛑 [AutoGPU] Không tìm thấy model GGUF tại: ${modelPath}`);
                onProgress(`⚠️ Thiếu model ${modelName}. Vui lòng tải model vào thư mục AI_Models.`);
                return;
            }

            // 2. Quét GPU NVIDIA
            const nvidiaInfo = await this.getNvidiaInfo();
            const currentState = this.readHardwareState();

            if (nvidiaInfo) {
                // Kiểm tra xem phần cứng có thay đổi không
                if (currentState && currentState.status === "success" && currentState.gpu_model === nvidiaInfo.model) {
                    logger.info(`✅ [AutoGPU] Phần cứng không thay đổi (${nvidiaInfo.model}, ${nvidiaInfo.vram_mb}MB VRAM). Bỏ qua Setup.`);
                    return;
                }

                logger.info(`🎮 [AutoGPU] GPU: ${nvidiaInfo.model} | VRAM: ${nvidiaInfo.vram_mb}MB | CUDA: ${nvidiaInfo.cuda}`);
                onProgress(`✅ Phát hiện GPU ${nvidiaInfo.model} (${nvidiaInfo.vram_mb}MB VRAM). Model sẽ được tải lên GPU!`);

                this.saveHardwareState({
                    gpu_model: nvidiaInfo.model,
                    cuda_version: nvidiaInfo.cuda,
                    vram_mb: nvidiaInfo.vram_mb,
                    llama_server_ok: true,
                    status: "success"
                });
            } else {
                logger.info("ℹ️ [AutoGPU] Không phát hiện NVIDIA GPU. LIVA sẽ chạy bằng CPU.");
                onProgress("ℹ️ Không tìm thấy GPU NVIDIA. LIVA sẽ chạy ở chế độ CPU.");

                this.saveHardwareState({
                    gpu_model: "CPU_Only",
                    cuda_version: "N/A",
                    vram_mb: 0,
                    llama_server_ok: true,
                    status: "success"
                });
            }

            // Đợi 1.5s để UI hiển thị thông báo
            await new Promise(r => setTimeout(r, 1500));

        } catch (e: any) {
            logger.error("❌ [AutoGPU] Lỗi kiểm tra phần cứng:", e.message);
            onProgress("⚠️ Không thể kiểm tra phần cứng. Tiếp tục khởi động...");
            await new Promise(r => setTimeout(r, 2000));
        }
    }
}
