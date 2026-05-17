import * as dotenv from "dotenv";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import * as crypto from "node:crypto";

// [STDOUT_GUARD] - Bảo vệ IPC Handshake khỏi rác Console
// eslint-disable-next-line no-console
console.log = (...args: unknown[]) => console.error('[STDOUT_GUARD]', ...args);
// eslint-disable-next-line no-console
console.warn = (...args: unknown[]) => console.error('[STDOUT_GUARD_WARN]', ...args);
// eslint-disable-next-line no-console
console.info = (...args: unknown[]) => console.error('[STDOUT_GUARD_INFO]', ...args);

import { CoreKernel } from "./core/CoreKernel";
import { logger } from "./utils/logger";
import { AutoGPUSetup } from "./scripts/AutoGPUSetup";

dotenv.config();

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

import { AppConfig } from "./config/AppConfig";
// 🔒 [Zero-Trust] Fail-fast configuration validation
AppConfig.loadAndValidate();

// Global singleton — typed access instead of `(global as any)`
declare global {
    var kernelInstance: CoreKernel | undefined;
}

// [ANTI-ZOMBIE GUARD]
// Khi Tauri Frontend bị đóng, luồng process.stdin sẽ bị cắt (EOF).
process.stdin.resume(); // Giữ luồng mở
process.stdin.on('end', () => {
    logger.warn("🛑 Nhận tín hiệu EOF từ Stdio (Frontend đã đóng). Thực thi Auto-Kill Sidecar...");
    shutdownGracefully();
});

process.on('SIGINT', () => {
    logger.warn("🛑 Nhận tín hiệu SIGINT (Ctrl+C). Đang đóng các file an toàn...");
    shutdownGracefully();
});

process.on('SIGTERM', () => {
    logger.warn("🛑 Nhận tín hiệu SIGTERM. Đang đóng các file an toàn...");
    shutdownGracefully();
});

async function shutdownGracefully() {
    logger.warn("⏳ [Data Loss Prevention] Bắt đầu ép xả (Force Flush) Write-Behind Cache...");
    if (globalThis.kernelInstance) {
        await globalThis.kernelInstance.shutdown();
    }
    // SQLite WAL flush đã được đảm bảo bởi `await db.close()` bên trong memory.dispose()
    // 🚨 Absolutely NO hardcoded sleeps (AI_CONTEXT §11)
    logger.info("✅ [Data Loss Prevention] Đã xả đệm an toàn. Tắt tiến trình.");
    process.exit(0);
}

async function start() {
  try {
    const kernel = new CoreKernel();
    globalThis.kernelInstance = kernel;
    
    await kernel.fetchSystemLocation();

    // Kích hoạt Auto GPU Setup ngầm TRƯỚC bootstrap
    // (WebSocket chưa mở nhưng setup chạy nhanh < 2s, kết quả lưu vào hardware_state.json)
    await AutoGPUSetup.runAutoSetupIfNeeded((msg) => {
        logger.info(`[AutoGPU] ${msg}`);
    });

    await kernel.bootstrap();
  } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
    logger.error({ err: errMsg, stack: e instanceof Error ? e.stack : undefined }, "System Fatal Error");
  }
}

start();
