import { z } from "zod";
import { promises as fsp } from "node:fs";
import path from "node:path";
import { logger } from "../../utils/logger";

/**
 * ConfigManager — Single Source of Truth for Environment & Runtime Config
 * ========================================================================
 * [v28] Full Merge: Absorbs both `core/config/ConfigManager` (AI routing) and
 * `config/AppConfig` (ports, feature flags). All env vars parsed exactly ONCE
 * at construction via Zod.
 *
 * Solves 3 critical architectural issues:
 *
 * 1. **Inconsistent env parsing** (Bug 1): `LIVA_USE_NATIVE` was parsed
 *    3 different ways across modules. Now all modules read from a single
 *    validated source.
 *
 * 2. **Config file re-read per inference** (Bug 7): `liva-config.json` was
 *    parsed from disk on every LLM inference call. Now cached with 30s TTL.
 *
 * 3. **Duplicate ConfigManagers** (Bug C-2): `AppConfig` and `ConfigManager`
 *    parsed the same env vars independently. Now unified into one singleton.
 *
 * Architecture:
 *   - Singleton — boot-time validation via Zod
 *   - TTL-cached JSON config (30s default)
 *   - All env vars parsed exactly ONCE at construction
 *   - Pure config class: depends only on node:fs, node:path, zod, logger
 *
 * @module ConfigManager
 */

const EnvSchema = z.object({
    // ─── AI & Inference ───
    LIVA_USE_NATIVE: z.string().optional().transform(val => val === "true"),
    AI_PROVIDER: z.string().optional().transform(val => (val?.toLowerCase() || "local") as "local" | "cloud" | "hybrid"),
    AI_BASE_URL: z.string().optional().default(""),
    AI_API_KEY: z.string().optional().default(""),
    AI_MODEL: z.string().optional().default("gpt-4"),
    FALLBACK_AI_BASE_URL: z.string().optional().default(""),
    FALLBACK_AI_API_KEY: z.string().optional().default(""),
    FALLBACK_AI_MODEL: z.string().optional().default("gpt-4o-mini"),
    AI_MODELS_DIR: z.string().optional().default("E:\\AI_Models"),
    EXPERT_MODEL_NAME: z.string().optional().default("gemma-4-26B-A4B-it-UD-Q6_K.gguf"),
    AI_CONTEXT_WINDOW: z.coerce.number().optional().default(8192),

    // ─── Application Mode ───
    NODE_ENV: z.enum(["development", "production", "test"]).default("development"),

    // ─── Ports ───
    GATEWAY_WS_PORT: z.coerce.number().default(0),
    META_WEBHOOK_PORT: z.coerce.number().default(3000),
    CDP_PORT: z.coerce.number().default(9222),
    VSCODE_WS_PORT: z.coerce.number().default(3710),
    LIVA_ROUTER_PORT: z.coerce.number().default(8000),

    // ─── Security / Vault ───
    LIVA_VAULT_PATH: z.string().optional(),

    // ─── Voice ───
    LIVA_TTS_ENGINE: z.string().default("python"),

    // ─── Feature Flags ───
    ENABLE_QUALITY_CHECKER: z.string().optional().transform(val => val !== "false"),
    ENABLE_WEB_RESEARCH: z.string().optional().transform(val => val !== "false"),

    // ─── Sentient Gatekeeper (Nhóm 10) ───
    LIVA_AUTO_RESPONDER_ENABLED: z.string().optional().transform(val => val === "true"),
    LIVA_URGENCY_BYPASS_ENABLED: z.string().optional().transform(val => val !== "false"),

    // ─── Proactive Routines (Nhóm 11) ───
    LIVA_MORNING_BRIEFING_ENABLED: z.string().optional().transform(val => val !== "false"),
    LIVA_HEALTH_MONITOR_ENABLED: z.string().optional().transform(val => val !== "false"),
    LIVA_MEETING_COPILOT_ENABLED: z.string().optional().transform(val => val === "true"),

    // ─── DevSecOps & Ambient Intelligence (Nhóm 12-13) ───
    LIVA_STATUS_SYNC_ENABLED: z.string().optional().transform(val => val === "true"),
    LIVA_FOCUS_WARDEN_ENABLED: z.string().optional().transform(val => val !== "false"),

    // ─── Whisper STT ───
    WHISPER_URL: z.string().optional(),
    WHISPER_CLOUD_URL: z.string().optional(),
    ROUTER_PORT: z.coerce.number().optional(),
});

export type ValidatedEnv = z.infer<typeof EnvSchema>;

/**
 * [v28 Backward Compat] AppConfigType — matches the old AppConfig.get() shape.
 * Consumers like UIController, CoreKernel, VoiceOrchestrator can keep using
 * `AppConfig.get().IS_DEV`, `AppConfig.get().GATEWAY_WS_PORT`, etc.
 */
export interface AppConfigType {
    NODE_ENV: "development" | "production" | "test";
    IS_DEV: boolean;
    GATEWAY_WS_PORT: number;
    META_WEBHOOK_PORT: number;
    CDP_PORT: number;
    VSCODE_WS_PORT: number;
    LIVA_ROUTER_PORT: number;
    LIVA_VAULT_PATH?: string;
    AI_PROVIDER: string;
    AI_BASE_URL?: string;
    AI_API_KEY?: string;
    AI_MODEL?: string;
    LIVA_TTS_ENGINE: string;
    ENABLE_QUALITY_CHECKER: boolean;
    ENABLE_WEB_RESEARCH: boolean;
    LIVA_AUTO_RESPONDER_ENABLED: boolean;
    LIVA_URGENCY_BYPASS_ENABLED: boolean;
    LIVA_MORNING_BRIEFING_ENABLED: boolean;
    LIVA_HEALTH_MONITOR_ENABLED: boolean;
    LIVA_MEETING_COPILOT_ENABLED: boolean;
    LIVA_STATUS_SYNC_ENABLED: boolean;
    LIVA_FOCUS_WARDEN_ENABLED: boolean;
}

export interface LivaRuntimeConfig {
    ai?: {
        temperature?: number;
        maxTokens?: number;
        topP?: number;
        contextWindow?: number;
    };
    [key: string]: unknown;
}

export class ConfigManager {
    static #instance: ConfigManager;

    #envConfig: ValidatedEnv;
    #appConfig: AppConfigType | null = null;
    #livaConfig: LivaRuntimeConfig | null = null;
    #lastFetch: number = 0;
    readonly #TTL = 30_000; // 30 seconds cache

    private constructor() {
        this.#envConfig = EnvSchema.parse(process.env);
        logger.info(`[ConfigManager] ✅ Env validated at boot. isNativeMode=${this.#envConfig.LIVA_USE_NATIVE}, aiProvider=${this.#envConfig.AI_PROVIDER}`);
    }

    public static getInstance(): ConfigManager {
        if (!ConfigManager.#instance) {
            ConfigManager.#instance = new ConfigManager();
        }
        return ConfigManager.#instance;
    }

    /** Whether to use NativeIPCClient (gRPC port 8100) vs OpenAI HTTP (port 8000) */
    public get isNativeMode(): boolean {
        return this.#envConfig.LIVA_USE_NATIVE;
    }

    /** AI provider mode: local | cloud | hybrid */
    public get aiProvider(): "local" | "cloud" | "hybrid" {
        return this.#envConfig.AI_PROVIDER;
    }

    /** Context window size in tokens — auto-scales for cloud providers */
    public get contextWindowTokens(): number {
        // Cloud APIs (Gemini, GPT) typically have 128K+ context
        if (this.#envConfig.AI_PROVIDER === "cloud") {
            return Math.max(this.#envConfig.AI_CONTEXT_WINDOW, 32768);
        }
        return this.#envConfig.AI_CONTEXT_WINDOW;
    }

    /** Full validated environment config */
    public get env(): Readonly<ValidatedEnv> {
        return this.#envConfig;
    }

    // ═══════════════════════════════════════════════════════
    //  [v28] Backward-compatible AppConfig interface
    // ═══════════════════════════════════════════════════════

    /**
     * [v28] Backward compat: mimics the old `AppConfig.loadAndValidate()`.
     * Now a no-op since validation happens at construction time.
     * Returns the AppConfigType shape for existing callers.
     */
    public loadAndValidate(): AppConfigType {
        return this.get();
    }

    /**
     * [v28] Backward compat: mimics the old `AppConfig.get()`.
     * Returns a flattened config object matching the old AppConfigType shape.
     */
    public get(): AppConfigType {
        if (this.#appConfig) return this.#appConfig;

        const isDev = process.argv.includes("--dev") || this.#envConfig.NODE_ENV === "development";

        this.#appConfig = {
            NODE_ENV: this.#envConfig.NODE_ENV,
            IS_DEV: isDev,
            GATEWAY_WS_PORT: isDev ? (this.#envConfig.GATEWAY_WS_PORT || 8082) : this.#envConfig.GATEWAY_WS_PORT,
            META_WEBHOOK_PORT: this.#envConfig.META_WEBHOOK_PORT,
            CDP_PORT: this.#envConfig.CDP_PORT,
            VSCODE_WS_PORT: this.#envConfig.VSCODE_WS_PORT,
            LIVA_ROUTER_PORT: this.#envConfig.LIVA_ROUTER_PORT,
            LIVA_VAULT_PATH: this.#envConfig.LIVA_VAULT_PATH,
            AI_PROVIDER: this.#envConfig.AI_PROVIDER,
            AI_BASE_URL: this.#envConfig.AI_BASE_URL,
            AI_API_KEY: this.#envConfig.AI_API_KEY,
            AI_MODEL: this.#envConfig.AI_MODEL,
            LIVA_TTS_ENGINE: this.#envConfig.LIVA_TTS_ENGINE,
            ENABLE_QUALITY_CHECKER: this.#envConfig.ENABLE_QUALITY_CHECKER,
            ENABLE_WEB_RESEARCH: this.#envConfig.ENABLE_WEB_RESEARCH,
            LIVA_AUTO_RESPONDER_ENABLED: this.#envConfig.LIVA_AUTO_RESPONDER_ENABLED,
            LIVA_URGENCY_BYPASS_ENABLED: this.#envConfig.LIVA_URGENCY_BYPASS_ENABLED,
            LIVA_MORNING_BRIEFING_ENABLED: this.#envConfig.LIVA_MORNING_BRIEFING_ENABLED,
            LIVA_HEALTH_MONITOR_ENABLED: this.#envConfig.LIVA_HEALTH_MONITOR_ENABLED,
            LIVA_MEETING_COPILOT_ENABLED: this.#envConfig.LIVA_MEETING_COPILOT_ENABLED,
            LIVA_STATUS_SYNC_ENABLED: this.#envConfig.LIVA_STATUS_SYNC_ENABLED,
            LIVA_FOCUS_WARDEN_ENABLED: this.#envConfig.LIVA_FOCUS_WARDEN_ENABLED,
        };

        return this.#appConfig;
    }

    // ═══════════════════════════════════════════════════════
    //  Runtime Config (liva-config.json) — TTL-cached
    // ═══════════════════════════════════════════════════════

    /**
     * Read liva-config.json with 30s TTL cache.
     * Returns cached config if within TTL, otherwise re-reads from disk.
     * Falls back to empty defaults if file is missing or malformed.
     */
    public async getLivaConfig(): Promise<LivaRuntimeConfig> {
        const now = Date.now();
        if (this.#livaConfig && (now - this.#lastFetch < this.#TTL)) {
            return this.#livaConfig;
        }

        try {
            const configPath = path.join(process.cwd(), "..", "data", "liva-config.json");
            const raw = await fsp.readFile(configPath, "utf8");
            this.#livaConfig = JSON.parse(raw);
            this.#lastFetch = now;
            // [v28] Sync context window from runtime config if specified
            if (this.#livaConfig?.ai?.contextWindow) {
                this.#envConfig = { ...this.#envConfig, AI_CONTEXT_WINDOW: this.#livaConfig.ai.contextWindow };
            }
        } catch {
            // File missing or malformed — use defaults
            if (!this.#livaConfig) {
                this.#livaConfig = {};
            }
        }

        return this.#livaConfig!;
    }

    /**
     * Force invalidate the config cache (e.g., after config file change).
     */
    public invalidateCache(): void {
        this.#livaConfig = null;
        this.#lastFetch = 0;
    }
}
