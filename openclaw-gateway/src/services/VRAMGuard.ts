import { exec } from "node:child_process";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { EventEmitter } from "node:events";
import { logger } from "../utils/logger";

/**
 * VRAMGuard — Preemptive VRAM Yielding Service (LIVA v24 Pillar 1)
 * ================================================================
 * Monitors system GPU load and detects heavy applications (games, renderers).
 * When a VRAM-heavy app is detected, emits 'yield_vram' so CoreKernel can
 * kill llama-server and route AI traffic to Cloud API.
 * When the app exits, emits 'reclaim_vram' to re-warm the local model.
 *
 * Detection Strategy (Layered):
 *   Layer 1: Process name whitelist matching (instant, <1ms)
 *   Layer 2: nvidia-smi GPU utilization threshold (opt-in, 3s poll)
 *
 * Architecture Constraints:
 *   - NEVER block Event Loop (child_process.exec with timeout)
 *   - All timers use .unref() to prevent shutdown hang
 *   - Single interval, configurable polling rate (default 10s)
 */

/** Well-known GPU-heavy processes (games, render engines, video editors) */
const HEAVY_APP_PROCESSES = new Set([
    // --- AAA Games ---
    "blackmythwukong", "cyberpunk2077", "eldenring", "starfield",
    "hogwartslegacy", "baldursgate3", "rdr2", "gtav", "witcher3",
    "cs2", "valorant", "overwatch", "fortnite", "pubg",
    "dota2", "leagueoflegends", "apexlegends", "callofduty",
    "palworld", "enshrouded", "helldivers2", "blackops6",
    // --- Render / Creative ---
    "blender", "unrealengine", "unity", "davinciresolve",
    "afterfx", "premiere", "nuke", "houdini", "maya",
    "3dsmax", "cinema4d", "substance",
    // --- ML / Mining ---
    // NOTE: "python" is checked at RUNTIME in #detectHeavyProcess,
    // because process.env may not be loaded when this module initializes.
    "python",
    "ollama",
]);

/** Minimum GPU utilization % to trigger yield (nvidia-smi layer) */
const GPU_UTIL_THRESHOLD = 75;

/** Minimum free VRAM (MB) before yielding — if more than this is free, coexist peacefully */
const VRAM_SAFETY_MB = 1024; // 1GB safety buffer

/** Polling interval in ms (default 10s — low overhead) */
const DEFAULT_POLL_MS = 10_000;

export interface VRAMGuardEvents {
    yield_vram: [{ reason: string; appName?: string; gpuUtil?: number }];
    reclaim_vram: [{ reason: string }];
}

export class VRAMGuard extends EventEmitter {
    #pollTimer: NodeJS.Timeout | null = null;
    #isYielded = false;
    #lastHeavyApp: string | null = null;
    #coexistLogged = false;
    #pollIntervalMs: number;
    #enabled = true;

    // [v25 FIX] Inject AgentLoop busy checker to prevent false positive GPU alarm
    // When AI is actively generating tokens, GPU utilization is naturally high (>75%).
    // VRAMGuard must skip Layer 2 GPU check in this case to avoid killing llama-server.
    #isAgentBusyCheck: () => boolean = () => false;

    constructor(pollIntervalMs: number = DEFAULT_POLL_MS) {
        super();
        this.#pollIntervalMs = pollIntervalMs;
    }

    /** Start monitoring. Idempotent. */
    public start(): void {
        if (this.#pollTimer) return;
        if (process.platform !== "win32") {
            logger.info("[VRAMGuard] Skipping — only supported on Windows (nvidia-smi / tasklist).");
            return;
        }

        logger.info(`[VRAMGuard] 🎮 Started GPU monitoring (poll: ${this.#pollIntervalMs}ms)`);
        this.#pollTimer = setInterval(() => {
            if (this.#enabled) this.#tick().catch(() => {});
        }, this.#pollIntervalMs);
        this.#pollTimer.unref();
    }

    /** Stop monitoring and clean up. */
    public dispose(): void {
        if (this.#pollTimer) {
            clearInterval(this.#pollTimer);
            this.#pollTimer = null;
        }
        logger.info("[VRAMGuard] Disposed.");
    }

    /** Get current yield state. */
    public get isYielded(): boolean {
        return this.#isYielded;
    }

    /** Temporarily disable monitoring (e.g., during shutdown). */
    public disable(): void {
        this.#enabled = false;
    }

    /** Re-enable monitoring. */
    public enable(): void {
        this.#enabled = true;
    }

    /** 
     * Inject a callback to check if the AI agent is currently busy (generating text).
     * Used to prevent false positives when the AI itself is maxing out the GPU.
     */
    public setAgentBusyCheck(fn: () => boolean): void {
        this.#isAgentBusyCheck = fn;
    }

    /**
     * Core polling tick — runs every N seconds.
     * Layer 1: Check running processes against heavy app whitelist.
     * Layer 2: (Optional) Query nvidia-smi for GPU utilization.
     */
    async #tick(): Promise<void> {
        try {
            // --- Layer 1: Process Name Detection ---
            const heavyApp = await this.#detectHeavyProcess();

            if (heavyApp && !this.#isYielded) {
                // Heavy app detected — but check actual VRAM before yielding.
                // On high-VRAM GPUs (16GB+), game + AI model can coexist.
                const freeVram = await this.#queryFreeVram();
                if (freeVram !== null && freeVram > VRAM_SAFETY_MB) {
                    // Enough VRAM for both — don't yield, log once only
                    if (!this.#coexistLogged) {
                        logger.info(`[VRAMGuard] 🎮 Heavy app "${heavyApp}" detected but ${freeVram}MB VRAM free (>${VRAM_SAFETY_MB}MB) — coexisting.`);
                        this.#coexistLogged = true;
                    }
                    return;
                }
                // Low VRAM or can't query → yield to be safe
                this.#isYielded = true;
                this.#lastHeavyApp = heavyApp;
                logger.warn(`[VRAMGuard] 🎮 Detected heavy app: "${heavyApp}" — free VRAM: ${freeVram ?? "unknown"}MB < ${VRAM_SAFETY_MB}MB — yielding!`);
                this.emit("yield_vram", { reason: `Heavy app detected: ${heavyApp}`, appName: heavyApp });
                return;
            }

            if (!heavyApp && this.#isYielded) {
                // Heavy app exited → reclaim VRAM
                const prevApp = this.#lastHeavyApp || "unknown";
                this.#isYielded = false;
                this.#lastHeavyApp = null;
                this.#coexistLogged = false;
                logger.info(`[VRAMGuard] ✅ Heavy app "${prevApp}" exited — reclaiming VRAM for local AI.`);
                this.emit("reclaim_vram", { reason: `App exited: ${prevApp}` });
                return;
            }

            // Reset coexist flag when heavy app exits (coexist mode — never yielded)
            if (!heavyApp && this.#coexistLogged) {
                this.#coexistLogged = false;
            }

            // --- Layer 2: nvidia-smi GPU Utilization (only if no whitelist match) ---
            // [v25 FIX] Skip Layer 2 if the AI itself is currently processing (generating text)
            // otherwise the AI's own 100% GPU usage will trigger a false-positive yield!
            if (!heavyApp && !this.#isYielded && !this.#isAgentBusyCheck()) {
                const gpuUtil = await this.#queryGpuUtilization();
                if (gpuUtil !== null && gpuUtil > GPU_UTIL_THRESHOLD) {
                    this.#isYielded = true;
                    logger.warn(`[VRAMGuard] ⚡ GPU utilization ${gpuUtil}% > ${GPU_UTIL_THRESHOLD}% — yielding VRAM!`);
                    this.emit("yield_vram", { reason: `GPU util ${gpuUtil}%`, gpuUtil });
                }
            }

            // Check if GPU freed up after high utilization yield
            if (this.#isYielded && !this.#lastHeavyApp) {
                const gpuUtil = await this.#queryGpuUtilization();
                if (gpuUtil !== null && gpuUtil < GPU_UTIL_THRESHOLD - 20) {
                    this.#isYielded = false;
                    logger.info(`[VRAMGuard] ✅ GPU utilization dropped to ${gpuUtil}% — reclaiming VRAM.`);
                    this.emit("reclaim_vram", { reason: `GPU util dropped to ${gpuUtil}%` });
                }
            }
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.debug(`[VRAMGuard] Tick error (non-critical): ${errMsg}`);
        }
    }

    /**
     * Layer 1: Detect heavy GPU processes using `tasklist /FO CSV`.
     * Non-blocking: exec with 3s timeout. Returns process name or null.
     */
    #detectHeavyProcess(): Promise<string | null> {
        // Runtime check: skip python detection when native engine is active
        const isNativeMode = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";

        return new Promise((resolve) => {
            exec("tasklist /FO CSV /NH", { timeout: 3000, windowsHide: true }, (err, stdout) => {
                if (err || !stdout) return resolve(null);

                const lines = stdout.split("\n");
                for (const line of lines) {
                    // CSV format: "process.exe","PID","SessionName","Session#","Mem"
                    const match = line.match(/^"([^"]+\.exe)"/i);
                    if (!match) continue;

                    const processName = match[1].replace(/\.exe$/i, "").toLowerCase();

                    // Skip python when it IS the AI engine (native mode)
                    if (processName === "python" && isNativeMode) continue;

                    if (HEAVY_APP_PROCESSES.has(processName)) {
                        return resolve(match[1]);
                    }
                }
                resolve(null);
            });
        });
    }

    /**
     * Layer 2: Query nvidia-smi for GPU utilization percentage.
     * Returns 0-100 or null if nvidia-smi is unavailable.
     */
    #queryGpuUtilization(): Promise<number | null> {
        return new Promise((resolve) => {
            exec(
                "nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits",
                { timeout: 3000, windowsHide: true },
                (err, stdout) => {
                    if (err || !stdout) return resolve(null);
                    const val = parseInt(stdout.trim(), 10);
                    resolve(isNaN(val) ? null : val);
                }
            );
        });
    }

    /**
     * Layer 3: Query nvidia-smi for FREE VRAM in MB.
     * Used to decide if game + AI model can coexist on high-VRAM GPUs.
     */
    #queryFreeVram(): Promise<number | null> {
        return new Promise((resolve) => {
            exec(
                "nvidia-smi --query-gpu=memory.free --format=csv,noheader,nounits",
                { timeout: 3000, windowsHide: true },
                (err, stdout) => {
                    if (err || !stdout) return resolve(null);
                    const val = parseInt(stdout.trim(), 10);
                    resolve(isNaN(val) ? null : val);
                }
            );
        });
    }

    /**
     * Load custom heavy app list from data/vram_guard_apps.json (optional).
     * Merges with built-in list. Non-blocking, fire-and-forget.
     */
    public async loadCustomApps(): Promise<void> {
        try {
            const customPath = path.join(process.cwd(), "data", "vram_guard_apps.json");
            const raw = await fsp.readFile(customPath, "utf-8");
            const apps = JSON.parse(raw);
            if (Array.isArray(apps)) {
                for (const app of apps) {
                    if (typeof app === "string") {
                        HEAVY_APP_PROCESSES.add(app.toLowerCase().replace(/\.exe$/i, ""));
                    }
                }
                logger.info(`[VRAMGuard] Loaded ${apps.length} custom heavy apps from vram_guard_apps.json`);
            }
        } catch {
            // No custom file — that's fine
        }
    }
}
