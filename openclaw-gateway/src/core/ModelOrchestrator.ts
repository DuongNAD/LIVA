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
        logger.info(`[ModelOrchestrator] Decoupled Mode: Hardware Daemon is expected to be running on port ${this.#serverPort}`);
        this.#isActive = true;
    }

    public async killLlamaServer(): Promise<void> {
        // Managed by Hardware Daemon VRAMGuard automatically.
        logger.info(`[ModelOrchestrator] killLlamaServer requested, but hardware is managed externally by Daemon.`);
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
            if (this.#isActive && this.#serverPort) {
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
        this.#isActive = false;
        this.removeAllListeners();
    }
}