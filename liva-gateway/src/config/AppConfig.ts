/**
 * AppConfig — Backward-Compatible Re-export
 * ============================================
 * [v28] This file is now a thin re-export from the unified ConfigManager.
 * All env parsing, validation, and caching is done in core/config/ConfigManager.ts.
 *
 * Existing callers can continue using:
 *   import { AppConfig } from "../config/AppConfig";
 *   const cfg = AppConfig.get();
 *
 * New code should import ConfigManager directly:
 *   import { ConfigManager } from "./config/ConfigManager";
 *   const cfg = ConfigManager.getInstance();
 */

import { ConfigManager } from "../core/config/ConfigManager";

export type { AppConfigType } from "../core/config/ConfigManager";

/**
 * Singleton re-export matching the old `AppConfig` interface.
 * AppConfig.get() → ConfigManager.getInstance().get()
 * AppConfig.loadAndValidate() → ConfigManager.getInstance().loadAndValidate()
 */
export const AppConfig = ConfigManager.getInstance();
