import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import { safeFetch, withSafeTimeout } from "../utils/HttpClient";

/**
 * ModelOrchestrator — Phase 3 Hardware Decoupled Facade
 * =========================================================
 * C++ process spawning, VRAMGuard, and AutoGPUSetup have been 
 * moved out to the Python Hardware Resource Daemon.
 * Gateway Node.js is now completely blind to the hardware 
 * and only monitors HTTP/gRPC health.
 */
export class ModelOrchestrator extends EventEmitter {
    #isActive: boolean = false;
    #serverPort: number = 8100;
    #failedPings = 0;
    #pingsExecuted = 0;
    #anomalyMonitorTimer: NodeJS.Timeout | null = null;
    #llamaProcess: any = null;

    public get routerPort() { return this.#serverPort; }
    public get expertPort() { return this.#serverPort; }

    constructor() {
        super();
        const isNative = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";
        this.#serverPort = isNative ? 8100 : 8000;
    }

    public isReady() {
        return this.#isActive;
    }

    public async startSingleExpert(auth?: any): Promise<void> {
        const isNative = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";
        if (isNative) {
            logger.info(`[ModelOrchestrator] Native Mode: Hardware Daemon is expected to be running on port ${this.#serverPort}`);
            this.#isActive = true;
            return;
        }

        // --- HTTP llama-server Mode (Zero-Latency Blueprint) ---
        logger.info(`[ModelOrchestrator] Spawning HTTP llama-server on port 8000...`);
        const cp = await import("child_process");
        const path = await import("path");
        const fs = await import("fs");

        const modelsDir = process.env.AI_MODELS_DIR || "E:\\AI_Models";
        const modelName = process.env.EXPERT_MODEL_NAME || "gemma-4-26B-A4B-it-UD-Q6_K.gguf";
        const exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
        const modelPath = path.join(modelsDir, modelName);

        if (!fs.existsSync(exePath)) {
            logger.error(`[ModelOrchestrator] Cannot find llama-server.exe at ${exePath}`);
            return;
        }

        const serverArgs = [
            "--host", "127.0.0.1",
            "--port", String(this.#serverPort),
            "-m", modelPath,
            "-c", "8192",
            "-ngl", "-1", // Offload all layers to GPU
            "-t", "4",
            "-b", "2048",
            "-fa", "on", // Flash attention
            "--embeddings", // Enable embeddings
            "--pooling", "mean", // 🚀 [Zero-Latency] Enable mean pooling for OAI embeddings compatibility
            "--cache-reuse", "256", // 🚀 [Zero-Latency] Prompt Caching (Radix Tree)
            "--parallel", "2"       // 🚀 [Zero-Latency] Isolated Slots (Chat + RAG)
        ];

        this.#llamaProcess = cp.spawn(exePath, serverArgs, {
            detached: false,
            stdio: ['ignore', 'pipe', 'pipe']
        });

        this.#llamaProcess.stdout?.on('data', (data: any) => {
            // logger.debug(`[llama-server] ${data}`);
        });

        this.#llamaProcess.stderr?.on('data', (data: any) => {
            // logger.debug(`[llama-server:err] ${data}`);
        });

        this.#llamaProcess.on('error', (err: any) => {
            logger.error(`[llama-server] ❌ Lỗi Spawn tiến trình: ${err.message}`);
        });
        
        this.#llamaProcess.on('exit', () => {
            this.#isActive = false;
        });

        this.#isActive = true;
    }

    public async killLlamaServer(): Promise<void> {
        if (this.#llamaProcess) {
            logger.info(`[ModelOrchestrator] Killing local llama-server...`);
            this.#llamaProcess.kill('SIGKILL');
            this.#llamaProcess = null;
        } else {
            logger.info(`[ModelOrchestrator] killLlamaServer requested, but no local process found.`);
        }
        this.#isActive = false;
    }

    public async restartRouter(): Promise<void> {
        logger.info(`[ModelOrchestrator] restartRouter requested. Awaiting Hardware Daemon self-healing...`);
        this.emit("rewarming_ai");
        this.#isActive = true;
    }

    public startAnomalyDetection() {
        if (this.#anomalyMonitorTimer) return;
        logger.info("🛡️ [DevSecOps] Tracking External AI Daemon Health...");
        
        this.#anomalyMonitorTimer = setInterval(async () => {
            if (this.#serverPort) {
                const isNative = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";
                const targetPort = isNative ? 8100 : this.#serverPort;
                const targetUrl = isNative ? `http://127.0.0.1:${targetPort}/health` : `http://127.0.0.1:${targetPort}/v1/models`;

                this.#pingsExecuted++;
                if (this.#pingsExecuted <= 3) return; // Grace period

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
                            logger.error("🛑 [DevSecOps] Hardware Daemon OFFLINE or VRAM Yielded. Circuit breaker activated.");
                            this.#isActive = false;
                            this.emit("anomaly_detected");
                        }
                        this.#failedPings = 0;
                    }
                }
            }
        }, 15000);
        this.#anomalyMonitorTimer.unref();
    }

    public getStatus() {
        return {
            routerActive: this.#isActive,
            routerPort: this.#serverPort,
            expertActive: this.#isActive,
            expertPort: this.#serverPort
        };
    }

    public async dispose() {
        if (this.#anomalyMonitorTimer) {
            clearInterval(this.#anomalyMonitorTimer);
            this.#anomalyMonitorTimer = null;
        }
        await this.killLlamaServer();
        this.#isActive = false;
        this.removeAllListeners();
    }
}