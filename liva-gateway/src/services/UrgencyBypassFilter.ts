import { logger } from "../utils/logger";
import LRUCache from "lru-cache";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const execAsync = promisify(exec);

// ─── Config Shape ──────────────────────────────────────────────────────────────
interface UrgencyConfig {
    vipContacts: string[];
    emergencyKeywords: string[];
    alertMode: "screen_flash" | "tts_loud" | "both";
    cooldownMinutes: number;
}

const DEFAULT_CONFIG: UrgencyConfig = {
    vipContacts: [],
    emergencyKeywords: ["gấp", "khẩn cấp", "emergency", "urgent", "cứu", "sập", "down", "sự cố", "báo động"],
    alertMode: "both",
    cooldownMinutes: 5,
};

const CONFIG_PATH = join(process.cwd(), "data", "urgency_config.json");

// ─── Dependency Interface ──────────────────────────────────────────────────────
export interface UrgencyBypassDeps {
    /** Đọc TTS cảnh báo */
    speakTTS: (text: string) => Promise<void>;
    /** Push toast notification lên UI */
    pushNotification: (title: string, body: string) => void;
    /** Flash screen đỏ */
    flashScreen: () => Promise<void>;
}

// ─── PowerShell: Flash Screen Red ──────────────────────────────────────────────
const PS_FLASH_SCREEN = `
Add-Type -AssemblyName System.Windows.Forms
$form = New-Object System.Windows.Forms.Form
$form.BackColor = [System.Drawing.Color]::Red
$form.WindowState = 'Maximized'
$form.FormBorderStyle = 'None'
$form.TopMost = $true
$form.Opacity = 0.5
$form.Show()
Start-Sleep -Milliseconds 800
$form.Close()
`.trim().replace(/\n/g, "; ");

// ─── Service Class ─────────────────────────────────────────────────────────────
export class UrgencyBypassFilter {
    #deps: UrgencyBypassDeps;
    #config: UrgencyConfig;

    /**
     * LRU cooldown: chống spam alert cho cùng sender.
     * Key = sender (lowercased), Value = timestamp.
     */
    #cooldownCache: LRUCache<string, number>;

    constructor(deps: UrgencyBypassDeps) {
        this.#deps = deps;
        this.#config = this.#loadConfig();
        this.#cooldownCache = this.#buildCooldownCache();
        logger.info(
            `[UrgencyBypass] 🚨 Initialized — ${this.#config.vipContacts.length} VIP contacts, ${this.#config.emergencyKeywords.length} keywords, mode=${this.#config.alertMode}, cooldown=${this.#config.cooldownMinutes}min`,
        );
    }

    /**
     * Phân loại tin nhắn: NORMAL | VIP | EMERGENCY.
     *
     * @param sender Tên/ID người gửi
     * @param content Nội dung tin nhắn
     */
    public checkMessage(sender: string, content: string): "NORMAL" | "VIP" | "EMERGENCY" {
        const senderLower = sender.toLowerCase().trim();
        const contentLower = content.toLowerCase();

        // Kiểm tra emergency keywords trước (ưu tiên cao nhất)
        const isEmergency = this.#config.emergencyKeywords.some(keyword => {
            const kw = keyword.toLowerCase();
            return contentLower.includes(kw);
        });
        if (isEmergency) {
            logger.info(`[UrgencyBypass] 🔴 EMERGENCY detected — sender: '${sender}', matched keyword in content`);
            return "EMERGENCY";
        }

        // Kiểm tra VIP contacts (case-insensitive partial match)
        const isVip = this.#config.vipContacts.some(vip => {
            const vipLower = vip.toLowerCase();
            return senderLower.includes(vipLower) || vipLower.includes(senderLower);
        });
        if (isVip) {
            logger.info(`[UrgencyBypass] 🟡 VIP detected — sender: '${sender}'`);
            return "VIP";
        }

        return "NORMAL";
    }

    /**
     * Kích hoạt cảnh báo cho tin nhắn VIP hoặc EMERGENCY.
     * Có cooldown để chống spam.
     *
     * @param level Mức cảnh báo
     * @param sender Người gửi
     * @param content Nội dung tin nhắn
     */
    public async triggerAlert(
        level: "VIP" | "EMERGENCY",
        sender: string,
        content: string,
    ): Promise<void> {
        const senderKey = sender.toLowerCase().trim();

        // Cooldown check — chống spam alert cho cùng sender
        if (this.#cooldownCache.has(senderKey)) {
            logger.debug(`[UrgencyBypass] ⏳ Cooldown active cho '${sender}', bỏ qua alert`);
            return;
        }

        // Set cooldown
        this.#cooldownCache.set(senderKey, Date.now());

        const emoji = level === "EMERGENCY" ? "🚨" : "⭐";
        const levelVi = level === "EMERGENCY" ? "KHẨN CẤP" : "VIP";
        const truncatedContent = content.length > 100 ? content.substring(0, 100) + "..." : content;

        logger.info(`[UrgencyBypass] ${emoji} Triggering ${level} alert cho '${sender}'`);

        const mode = this.#config.alertMode;

        // ── Screen Flash ──
        if (mode === "screen_flash" || mode === "both") {
            try {
                await this.#flashScreenRed();
            } catch (err: unknown) {
                const errMsg = err instanceof Error ? err.message : String(err);
                logger.warn(`[UrgencyBypass] Lỗi flash screen: ${errMsg}`);
            }
        }

        // ── TTS Alert ──
        if (mode === "tts_loud" || mode === "both") {
            const ttsMessage = level === "EMERGENCY"
                ? `Cảnh báo khẩn cấp! Tin nhắn từ ${sender}: ${truncatedContent}`
                : `Tin nhắn quan trọng từ ${sender}: ${truncatedContent}`;
            try {
                await this.#deps.speakTTS(ttsMessage);
            } catch (err: unknown) {
                const errMsg = err instanceof Error ? err.message : String(err);
                logger.warn(`[UrgencyBypass] Lỗi TTS: ${errMsg}`);
            }
        }

        // ── Push Critical Notification ──
        try {
            this.#deps.pushNotification(
                `${emoji} [${levelVi}] Tin nhắn từ ${sender}`,
                truncatedContent,
            );
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[UrgencyBypass] Lỗi push notification: ${errMsg}`);
        }

        logger.info(`[UrgencyBypass] ✅ ${level} alert đã gửi cho '${sender}'`);
    }

    /**
     * Hot-reload config từ file.
     */
    public reloadConfig(): void {
        const oldConfig = { ...this.#config };
        this.#config = this.#loadConfig();
        this.#cooldownCache = this.#buildCooldownCache();
        logger.info(
            `[UrgencyBypass] 🔄 Config reloaded — VIP: ${oldConfig.vipContacts.length}→${this.#config.vipContacts.length}, keywords: ${oldConfig.emergencyKeywords.length}→${this.#config.emergencyKeywords.length}`,
        );
    }

    /**
     * Cleanup resources.
     */
    public dispose(): void {
        this.#cooldownCache.clear();
        logger.info("[UrgencyBypass] 🛑 Disposed.");
    }

    // ─── Private Helpers ───────────────────────────────────────────────────────

    /**
     * Load config từ data/urgency_config.json.
     * Fallback về DEFAULT_CONFIG nếu file không tồn tại hoặc parse lỗi.
     */
    #loadConfig(): UrgencyConfig {
        try {
            if (!existsSync(CONFIG_PATH)) {
                logger.warn(`[UrgencyBypass] Config file không tồn tại tại ${CONFIG_PATH}, dùng default`);
                return { ...DEFAULT_CONFIG };
            }

            const raw = readFileSync(CONFIG_PATH, "utf-8");
            const parsed = JSON.parse(raw);

            // Validate shape cơ bản
            const config: UrgencyConfig = {
                vipContacts: Array.isArray(parsed.vipContacts) ? parsed.vipContacts : DEFAULT_CONFIG.vipContacts,
                emergencyKeywords: Array.isArray(parsed.emergencyKeywords)
                    ? parsed.emergencyKeywords
                    : DEFAULT_CONFIG.emergencyKeywords,
                alertMode: ["screen_flash", "tts_loud", "both"].includes(parsed.alertMode)
                    ? parsed.alertMode
                    : DEFAULT_CONFIG.alertMode,
                cooldownMinutes: typeof parsed.cooldownMinutes === "number" && parsed.cooldownMinutes > 0
                    ? parsed.cooldownMinutes
                    : DEFAULT_CONFIG.cooldownMinutes,
            };

            logger.info(`[UrgencyBypass] 📄 Loaded config từ ${CONFIG_PATH}`);
            return config;
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.error(`[UrgencyBypass] Lỗi đọc config: ${errMsg}, dùng default`);
            return { ...DEFAULT_CONFIG };
        }
    }

    /**
     * Tạo LRU cooldown cache với TTL từ config.
     */
    #buildCooldownCache(): LRUCache<string, number> {
        return new LRUCache<string, number>({
            max: 200,
            ttl: this.#config.cooldownMinutes * 60 * 1000,
        });
    }

    /**
     * Flash screen đỏ qua PowerShell WinForms overlay.
     */
    async #flashScreenRed(): Promise<void> {
        // Ưu tiên dùng deps.flashScreen nếu có implementation
        try {
            await this.#deps.flashScreen();
            return;
        } catch {
            // deps.flashScreen có thể chỉ là placeholder → fallback PowerShell
        }

        // Fallback: PowerShell WinForms
        await execAsync(`powershell.exe -NoProfile -Command "${PS_FLASH_SCREEN}"`, {
            timeout: 5_000,
        });
        logger.debug("[UrgencyBypass] 🔴 Screen flash via PowerShell WinForms");
    }
}
