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
 * [DevSecOps] Load encrypted vault from Electron's userData directory.
 * Electron (electron.cjs) migrates sensitive keys (EMAIL_PASS, ZALO_OA_ACCESS_TOKEN, etc.)
 * from .env into liva_vault.json using AES-256-GCM encryption (compatible with EncryptionEngine).
 * Gateway reads and decrypts these values to make them available via process.env.
 */
function loadSecureVault(): void {
  const encryptionKey = process.env.LIVA_ENCRYPTION_KEY;
  
  // Need encryption key to decrypt vault
  if (!encryptionKey || Buffer.byteLength(encryptionKey, 'utf8') !== 32) {
    // eslint-disable-next-line no-console
    console.error('[Vault] LIVA_ENCRYPTION_KEY not set or invalid (must be 32 bytes). Cannot load vault.');
    return;
  }

  // Vault is stored in Electron's userData directory
  // Pattern: %APPDATA%\{appName}\liva_vault.json on Windows
  const homeDir = os.homedir();
  const appDataDir = process.env.APPDATA || path.join(homeDir, "AppData", "Roaming");
  
  // Try common Electron app names used in this project
  const possibleVaultPaths = [
    path.join(appDataDir, "liva-ui", "liva_vault.json"),
    path.join(appDataDir, "liva", "liva_vault.json"),
    path.join(appDataDir, "openclaw-gateway", "liva_vault.json"),
    path.join(homeDir, ".liva", "liva_vault.json"),
  ];

  for (const vaultPath of possibleVaultPaths) {
    if (fs.existsSync(vaultPath)) {
      try {
        const vaultData = JSON.parse(fs.readFileSync(vaultPath, "utf8"));
        let loadedCount = 0;

        // Decrypt each key and inject into process.env
        for (const [key, encryptedValue] of Object.entries(vaultData)) {
          if (typeof encryptedValue !== "string" || encryptedValue.length === 0) continue;
          
          // Skip if already set in .env (explicit config takes precedence)
          if (process.env[key]) continue;

          // Try to decrypt using AES-256-GCM format (iv:authTag:ciphertext)
          try {
            const decrypted = decryptVaultValue(String(encryptedValue), encryptionKey);
            if (decrypted) {
              process.env[key] = decrypted;
              loadedCount++;
            }
          } catch {
            // Skip keys that can't be decrypted
          }
        }
        
        if (loadedCount > 0) {
          // eslint-disable-next-line no-console
          console.error(`[Vault] ✅ Loaded ${loadedCount} keys from ${vaultPath}`);
        }
        return;
      } catch (e) {
        // eslint-disable-next-line no-console
        console.error(`[Vault] Error reading vault at ${vaultPath}:`, e);
      }
    }
  }
}

/**
 * Decrypt vault value using AES-256-GCM (compatible with EncryptionEngine)
 * Format: iv:authTag:ciphertext (all hex)
 */
function decryptVaultValue(encryptedText: string, key: string): string | null {
  try {
    const parts = encryptedText.split(':');
    if (parts.length !== 3) {
      // Not encrypted format - might be plain text (backward compatibility)
      return encryptedText;
    }
    
    const iv = Buffer.from(parts[0], 'hex');
    const authTag = Buffer.from(parts[1], 'hex');
    const encrypted = parts[2];
    
    const decipher = crypto.createDecipheriv('aes-256-gcm', Buffer.from(key), iv);
    decipher.setAuthTag(authTag);
    let decrypted = decipher.update(encrypted, 'hex', 'utf8');
    decrypted += decipher.final('utf8');
    return decrypted;
  } catch {
    // Decryption failed - might be from safeStorage (can't decrypt without Electron)
    return null;
  }
}

// Load vault AFTER dotenv.config() so .env values take precedence
loadSecureVault();

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
