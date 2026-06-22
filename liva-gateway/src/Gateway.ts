import "./bootstrapEnv";
import { CoreKernel } from "./core/CoreKernel";
import { logger } from "./utils/logger";
import { AutoGPUSetup } from "./scripts/AutoGPUSetup";




/**
 * [DevSecOps] Load encrypted vault from Tauri host's data directory.
 * The Tauri host (Rust) provides the LIVA_ENCRYPTION_KEY via environment.
 * Sensitive keys (EMAIL_PASS, ZALO_OA_ACCESS_TOKEN, etc.) are stored
 * encrypted in liva_vault.json using AES-256-GCM (EncryptionEngine).
 * Gateway reads and decrypts these values to make them available via process.env.
 */
import { EncryptionEngine } from "./memory/EncryptionEngine";

// Load vault AFTER dotenv.config() so .env values take precedence
EncryptionEngine.loadVaultIntoEnv();

// Map generic email credentials to IMAP-specific variables for backwards compatibility
if (process.env.EMAIL_HOST && !process.env.EMAIL_IMAP_HOST) {
    process.env.EMAIL_IMAP_HOST = process.env.EMAIL_HOST;
}
if (process.env.EMAIL_USER && !process.env.EMAIL_IMAP_USER) {
    process.env.EMAIL_IMAP_USER = process.env.EMAIL_USER;
}

import { AppConfig } from "./config/AppConfig";
import { SignalTrap } from "./core/kernel/SignalTrap";

// 🔒 [Zero-Trust] Fail-fast configuration validation
AppConfig.loadAndValidate();

// Global singleton — typed access instead of `(global as any)`
declare global {
    var kernelInstance: CoreKernel | undefined;
}

async function start() {
  try {
    const kernel = new CoreKernel();
    globalThis.kernelInstance = kernel;
    SignalTrap.listen(kernel);
    
    // ⚡ [PERF C5] Parallel boot: fetchLocation + AutoGPU are independent
    await Promise.all([
        kernel.fetchSystemLocation().catch(e => {
            logger.warn({ err: e instanceof Error ? e.message : String(e) }, "[Boot] fetchSystemLocation failed, continuing...");
        }),
        AutoGPUSetup.runAutoSetupIfNeeded((msg) => {
            logger.info(`[AutoGPU] ${msg}`);
        }).catch(e => {
            logger.warn({ err: e instanceof Error ? e.message : String(e) }, "[Boot] AutoGPUSetup failed, continuing...");
        }),
    ]);

    await kernel.bootstrap();
  } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
    logger.error({ err: errMsg, stack: e instanceof Error ? e.stack : undefined }, "System Fatal Error");
  }
}

start();

