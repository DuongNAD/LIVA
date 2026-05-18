/**
 * FeatureFlags — Zero-Downtime Rollout Controller
 * =================================================
 * Centralized feature flag management for canary deployments
 * and instant rollback of v4.0 Enterprise features.
 *
 * Usage:
 *   import { FF } from "./utils/FeatureFlags";
 *   if (FF.isEnabled("L2_INJECTION")) { ... }
 *
 * Override via environment variables:
 *   FF_ENABLE_L2_INJECTION=true
 *   FF_ENABLE_ENCRYPTION=false
 *
 * [v4.0] Supports: env vars > defaults
 * [Future] Ready for remote config (LaunchDarkly, Unleash, etc.)
 */

import { logger } from "./logger";

// ===========================
// Flag Definitions
// ===========================

interface FlagDef {
    /** Environment variable name */
    envKey: string;
    /** Default value when env var is not set */
    defaultValue: boolean;
    /** Human-readable description */
    description: string;
}

const FLAG_REGISTRY: Record<string, FlagDef> = {
    L2_INJECTION: {
        envKey: "FF_ENABLE_L2_INJECTION",
        defaultValue: false,
        description: "Enable L2 semantic search injection in PromptBuilder (G-1)",
    },
    ENCRYPTION: {
        envKey: "FF_ENABLE_ENCRYPTION",
        defaultValue: true,
        description: "Enable AES-256-GCM encryption for fact values (W-7)",
    },
    CROSS_SESSION_WARMUP: {
        envKey: "FF_ENABLE_CROSS_SESSION",
        defaultValue: true,
        description: "Enable cross-session warm-up from L1 turns (G-4)",
    },
    PKE_MICRO_BATCH: {
        envKey: "FF_ENABLE_PKE_BATCH",
        defaultValue: true,
        description: "Enable PKE buffered micro-batching (MEM-104)",
    },
    FACT_RECONCILIATION: {
        envKey: "FF_ENABLE_RECONCILIATION",
        defaultValue: true,
        description: "Enable fact reconciliation via importance scoring (G-9)",
    },
    TELEMETRY: {
        envKey: "FF_ENABLE_TELEMETRY",
        defaultValue: true,
        description: "Enable memory system telemetry counters",
    },
    NOMIC_EMBED: {
        envKey: "FF_ENABLE_NOMIC_EMBED",
        defaultValue: true,
        description: "Use nomic-embed-text-v1.5 (768D, 8192 ctx) instead of MiniLM-L6-v2 (384D, 512 ctx). Phase 1 RAG upgrade.",
    },
    HYBRID_SEARCH: {
        envKey: "FF_ENABLE_HYBRID_SEARCH",
        defaultValue: false,
        description: "Enable Hybrid Search (Vector + BM25 RRF) in StructuredMemory. Phase 1 RAG upgrade.",
    },
    HIERARCHICAL_SEARCH: {
        envKey: "FF_ENABLE_HIERARCHICAL_SEARCH",
        defaultValue: true,
        description: "Enable H-MEM v18 hierarchical domain-routed search in StructuredMemory.",
    },
};

// ===========================
// Feature Flag Manager
// ===========================

class FeatureFlagManager {
    private readonly overrides = new Map<string, boolean>();

    /**
     * Check if a feature flag is enabled.
     * Priority: runtime override > env var > default
     */
    public isEnabled(flagName: string): boolean {
        // Runtime override (for testing)
        if (this.overrides.has(flagName)) {
            return this.overrides.get(flagName)!;
        }

        const def = FLAG_REGISTRY[flagName];
        if (!def) {
            logger.warn(`[FeatureFlags] Unknown flag: ${flagName}, defaulting to false`);
            return false;
        }

        const envVal = process.env[def.envKey];
        if (envVal !== undefined) {
            return envVal === "true" || envVal === "1";
        }

        return def.defaultValue;
    }

    /** Set a runtime override (for testing / admin commands) */
    public setOverride(flagName: string, value: boolean): void {
        this.overrides.set(flagName, value);
        logger.info(`[FeatureFlags] Override: ${flagName} = ${value}`);
    }

    /** Clear a runtime override */
    public clearOverride(flagName: string): void {
        this.overrides.delete(flagName);
    }

    /** Clear all overrides */
    public clearAllOverrides(): void {
        this.overrides.clear();
    }

    /** Get all flags and their current states (for admin/debug) */
    public getAllFlags(): Record<string, { enabled: boolean; source: string; description: string }> {
        const result: Record<string, { enabled: boolean; source: string; description: string }> = {};
        for (const [name, def] of Object.entries(FLAG_REGISTRY)) {
            let source = "default";
            let enabled = def.defaultValue;

            if (this.overrides.has(name)) {
                source = "override";
                enabled = this.overrides.get(name)!;
            } else if (process.env[def.envKey] !== undefined) {
                source = "env";
/* istanbul ignore next */
                enabled = process.env[def.envKey] === "true" || process.env[def.envKey] === "1";
            }

            result[name] = { enabled, source, description: def.description };
        }
        return result;
    }
}

/** Singleton instance */
export const FF = new FeatureFlagManager();
