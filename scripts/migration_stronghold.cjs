const { app, safeStorage } = require('electron');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

// The new AES-256-GCM master key to migrate to (Must match LIVA_ENCRYPTION_KEY in Tauri/Gateway)
const LIVA_ENCRYPTION_KEY = process.env.LIVA_ENCRYPTION_KEY || '12345678901234567890123456789012';

function encryptAES256GCM(text, keyString) {
    const key = Buffer.from(keyString, 'utf8');
    const iv = crypto.randomBytes(16);
    const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
    let encrypted = cipher.update(text, 'utf8', 'hex');
    encrypted += cipher.final('hex');
    const authTag = cipher.getAuthTag().toString('hex');
    return `${iv.toString('hex')}:${authTag}:${encrypted}`;
}

app.whenReady().then(() => {
    console.log("================================================");
    console.log("🛡️ LIVA MIGRATION: STRONGHOLD (Electron -> Tauri)");
    console.log("================================================");

    if (!safeStorage.isEncryptionAvailable()) {
        console.error("❌ Electron safeStorage is not available on this system.");
        app.quit();
        return;
    }

    // Check multiple possible paths for the old vault
    const possiblePaths = [
        path.join(app.getPath('userData'), "liva_vault.json"),
        path.join(app.getPath('appData'), "liva-ui", "liva_vault.json"),
        path.join(app.getPath('appData'), "liva", "liva_vault.json"),
        path.join(app.getPath('appData'), "openclaw-gateway", "liva_vault.json")
    ];

    let vaultPath = null;
    let vaultData = null;

    for (const p of possiblePaths) {
        if (fs.existsSync(p)) {
            try {
                vaultData = JSON.parse(fs.readFileSync(p, 'utf8'));
                vaultPath = p;
                break;
            } catch (e) {}
        }
    }

    if (!vaultData) {
        console.log("⚠️ Không tìm thấy Két sắt (Vault) Electron cũ nào để migrate.");
        app.quit();
        return;
    }

    console.log(`🔍 Tìm thấy Vault cũ tại: ${vaultPath}`);
    console.log(`🔄 Đang giải mã và chuyển sang định dạng AES-256-GCM (Tauri-Ready)...`);

    let migratedCount = 0;
    const newVaultData = {};

    for (const [key, encryptedValue] of Object.entries(vaultData)) {
        if (typeof encryptedValue !== 'string' || encryptedValue.length === 0) continue;

        let plainText = null;

        // B1: Giải mã bằng Electron safeStorage
        try {
            // Thử giải mã nếu dữ liệu cũ lưu dưới dạng Buffer Base64
            plainText = safeStorage.decryptString(Buffer.from(encryptedValue, 'base64'));
        } catch (e1) {
            try {
                // Thử giải mã nếu dữ liệu cũ lưu dưới dạng hex
                plainText = safeStorage.decryptString(Buffer.from(encryptedValue, 'hex'));
            } catch (e2) {
                // Nếu đã là plaintext thì dùng luôn
                plainText = encryptedValue;
            }
        }

        if (plainText) {
            // B2: Mã hóa lại bằng chuẩn chung (EncryptionEngine) của Gateway
            const newEncrypted = encryptAES256GCM(plainText, LIVA_ENCRYPTION_KEY);
            newVaultData[key] = newEncrypted;
            migratedCount++;
        }
    }

    // B3: Ghi ra file Vault mới
    const outputDir = path.join(__dirname, "..", "data");
    if (!fs.existsSync(outputDir)) {
        fs.mkdirSync(outputDir, { recursive: true });
    }
    
    const newVaultPath = path.join(outputDir, "liva_vault.json");
    fs.writeFileSync(newVaultPath, JSON.stringify(newVaultData, null, 2), 'utf8');

    console.log(`✅ Hoàn tất! Đã migrate thành công ${migratedCount} chìa khóa.`);
    console.log(`💾 Két sắt mới (Tauri) được lưu tại: ${newVaultPath}`);
    console.log(`⚠️ Bạn có thể dùng liva_vault.json mới này cho Tauri và xóa bỏ Electron.`);
    console.log("================================================");
    
    app.quit();
});
