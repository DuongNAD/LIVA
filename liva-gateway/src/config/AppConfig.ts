import { z } from "zod";
import { logger } from "../utils/logger";

const AppConfigSchema = z.object({
  // Application Mode
  NODE_ENV: z.enum(["development", "production", "test"]).default("development"),
  IS_DEV: z.boolean().default(false),
  
  // Ports
  GATEWAY_WS_PORT: z.number().default(0), // 0 means dynamic port (Zero-Trust)
  META_WEBHOOK_PORT: z.number().default(3000),
  CDP_PORT: z.number().default(9222),
  VSCODE_WS_PORT: z.number().default(3710),
  LIVA_ROUTER_PORT: z.number().default(8000),
  
  // Security / Vault
  LIVA_VAULT_PATH: z.string().optional(),
  
  // AI & Inference
  AI_PROVIDER: z.string().default("local"),
  AI_BASE_URL: z.string().optional(),
  AI_API_KEY: z.string().optional(),
  AI_MODEL: z.string().optional(),
  LIVA_TTS_ENGINE: z.string().default("python"),
  
  // Feature Flags
  ENABLE_QUALITY_CHECKER: z.boolean().default(true),
  ENABLE_WEB_RESEARCH: z.boolean().default(true),

  // Nhóm 10: Sentient Gatekeeper
  LIVA_AUTO_RESPONDER_ENABLED: z.boolean().default(false),
  LIVA_URGENCY_BYPASS_ENABLED: z.boolean().default(true),

  // Nhóm 11: Proactive Routines
  LIVA_MORNING_BRIEFING_ENABLED: z.boolean().default(true),
  LIVA_HEALTH_MONITOR_ENABLED: z.boolean().default(true),
  LIVA_MEETING_COPILOT_ENABLED: z.boolean().default(false),

  // Nhóm 12-13: DevSecOps & Ambient Intelligence
  LIVA_STATUS_SYNC_ENABLED: z.boolean().default(false),
  LIVA_FOCUS_WARDEN_ENABLED: z.boolean().default(true),
});

export type AppConfigType = z.infer<typeof AppConfigSchema>;

class ConfigManager {
  private config: AppConfigType | null = null;

  public loadAndValidate(): AppConfigType {
    // Determine IS_DEV from args or env
    const envNodeEnv = process.env.NODE_ENV || "development";
    const isDev = process.argv.includes("--dev") || envNodeEnv === "development";
    
    const envData = {
      NODE_ENV: process.env.NODE_ENV,
      IS_DEV: isDev,
      GATEWAY_WS_PORT: isDev ? 8082 : 0,
      META_WEBHOOK_PORT: Number(process.env.META_WEBHOOK_PORT) || undefined,
      CDP_PORT: Number(process.env.CDP_PORT) || undefined,
      VSCODE_WS_PORT: Number(process.env.VSCODE_WS_PORT) || undefined,
      LIVA_ROUTER_PORT: Number(process.env.LIVA_ROUTER_PORT) || undefined,
      LIVA_VAULT_PATH: process.env.LIVA_VAULT_PATH,
      AI_PROVIDER: process.env.AI_PROVIDER,
      AI_BASE_URL: process.env.AI_BASE_URL,
      AI_API_KEY: process.env.AI_API_KEY,
      AI_MODEL: process.env.AI_MODEL,
      LIVA_TTS_ENGINE: process.env.LIVA_TTS_ENGINE,
      ENABLE_QUALITY_CHECKER: process.env.ENABLE_QUALITY_CHECKER !== "false",
      ENABLE_WEB_RESEARCH: process.env.ENABLE_WEB_RESEARCH !== "false",
      // Nhóm 10-13: Sentient Gatekeeper / Proactive / Ambient
      LIVA_AUTO_RESPONDER_ENABLED: process.env.LIVA_AUTO_RESPONDER_ENABLED === "true",
      LIVA_URGENCY_BYPASS_ENABLED: process.env.LIVA_URGENCY_BYPASS_ENABLED !== "false",
      LIVA_MORNING_BRIEFING_ENABLED: process.env.LIVA_MORNING_BRIEFING_ENABLED !== "false",
      LIVA_HEALTH_MONITOR_ENABLED: process.env.LIVA_HEALTH_MONITOR_ENABLED !== "false",
      LIVA_MEETING_COPILOT_ENABLED: process.env.LIVA_MEETING_COPILOT_ENABLED === "true",
      LIVA_STATUS_SYNC_ENABLED: process.env.LIVA_STATUS_SYNC_ENABLED === "true",
      LIVA_FOCUS_WARDEN_ENABLED: process.env.LIVA_FOCUS_WARDEN_ENABLED !== "false",
    };

    const parsed = AppConfigSchema.safeParse(envData);
    
    if (!parsed.success) {
      const errorMsg = parsed.error.issues.map(i => `${i.path.join(".")}: ${i.message}`).join(", ");
      logger.fatal(`[AppConfig] FATAL ERROR: Configuration Validation Failed. Missing or invalid variables: ${errorMsg}`);
      process.exit(1); // Fail-fast
    }

    this.config = parsed.data;
    logger.info("[AppConfig] Environment configuration validated successfully.");
    return this.config;
  }

  public get(): AppConfigType {
    if (!this.config) {
        return this.loadAndValidate();
    }
    return this.config;
  }
}

export const AppConfig = new ConfigManager();
