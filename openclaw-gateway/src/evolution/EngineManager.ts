import { exec, spawn } from "node:child_process";
import { promisify } from "util";
import { safeFetch } from "../utils/HttpClient";
import { evoLogger } from "./EvolutionLogger";
import * as path from "node:path";
import * as fsSync from "node:fs";

const execAsync = promisify(exec);

export async function sleep(ms: number) {
    return new Promise(r => setTimeout(r, ms));
}

export class EngineManager {
    static async killPortWindows(port: number) {
        try {
            const { stdout } = await execAsync(`netstat -ano | findstr :${port}`);
            if (!stdout) return;
            const lines = stdout.trim().split("\n");
            const listeningLine = lines.find(l => l.includes("LISTENING"));
            if (!listeningLine) return;

            const match = listeningLine.trim().split(/\s+/);
            const pid = match[match.length - 1];
            if (pid && Number.parseInt(pid) > 0) {
                evoLogger.info(`[Hot-Swap] Tìm thấy tiến trình (PID: ${pid}) khóa Cổng ${port}. Đang Graceful Shutdown...`);
                try {
                    // Cố gắng tắt mềm trước (không dùng /F)
                    await execAsync(`taskkill /PID ${pid} /T`);
                    await sleep(2000);
                } catch(e) {}
                
                // Tắt ép buộc nếu vẫn còn
                try {
                    await execAsync(`taskkill /PID ${pid} /F /T`);
                } catch(e) {}
                evoLogger.info(`[Hot-Swap] Đã dọn dẹp Cổng ${port}.`);
            }
        } catch (e) {
            // Ignored
        }
    }

    static async pingUvicorn(port: number, retries = 30): Promise<boolean> {
        for (let i = 0; i < retries; i++) {
            try {
                // Sử dụng AbortSignal để đảm bảo không bị treo mạng, đúng chuẩn rule
                const resp = await safeFetch(`http://127.0.0.1:${port}/docs`, {
                    signal: AbortSignal.timeout(3000)
                }, 3000);
                if (resp.status) return true;
            } catch (e) {
                await sleep(2000);
            }
        }
        return false;
    }

    static async checkPortAvailable(port: number): Promise<boolean> {
        for (let i = 0; i < 10; i++) {
            try {
                const { stdout } = await execAsync(`netstat -ano | findstr :${port}`);
                if (!stdout.trim()) return true;
            } catch (e) {
                return true;
            }
            await sleep(1000);
        }
        return false;
    }

    static async waitForVRAMClear(thresholdMB = 2048, timeoutSec = 30): Promise<void> {
        evoLogger.info(`[VRAM Polling] Đang chờ GPU giải phóng bộ nhớ...`);
        for (let i = 0; i < timeoutSec; i++) {
            try {
                const { stdout } = await execAsync(`nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits`);
                if (stdout) {
                    const usedVRAM = Number.parseInt(stdout.trim());
                    if (usedVRAM <= thresholdMB) {
                        evoLogger.info(`[VRAM Polling] OK! VRAM hiện tại: ${usedVRAM} MB.`);
                        return;
                    }
                }
            } catch (e) {}
            await sleep(1000);
        }
        evoLogger.warn(`[VRAM Polling] Timeout chờ VRAM. Có thể OS đang Cache cứng. Tiếp tục...`);
    }

    static async startEngineWindows(fileName: string, args: string[] = []) {
        const roleStr = args.length > 0 ? args.join(" ") : "(Default)";
        evoLogger.info(`[Hot-Swap] Kích nổ động cơ ${fileName} ${roleStr}...`);
        const engineDir = path.join(process.cwd(), "..", "liva-ai-engine");
        const pythonPath = path.join(engineDir, "venv", "Scripts", "python.exe");

        const logDir = path.join(process.cwd(), "logs");
        if (!fsSync.existsSync(logDir)) fsSync.mkdirSync(logDir);
        const out = fsSync.openSync(path.join(logDir, `${fileName}.log`), "a");
        const err = fsSync.openSync(path.join(logDir, `${fileName}.err.log`), "a");
        
        const child = spawn(pythonPath, [fileName, ...args], {
            cwd: engineDir,
            windowsHide: true, 
            stdio: ["ignore", out, err], 
            env: { ...process.env, PYTHONIOENCODING: "utf-8" } 
        });
        child.on('error', (errState) => {
            evoLogger.error({ err: errState }, `[Hot-Swap] Lỗi khởi động Python`);
        });
        child.unref(); 
        
        fsSync.closeSync(out);
        fsSync.closeSync(err);
    }
}
