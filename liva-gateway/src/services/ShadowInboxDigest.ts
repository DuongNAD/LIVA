import { logger } from "../utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

/**
 * ShadowInboxDigest — LIVA Background Daemon
 * =============================================
 * Monitors user idle state via Windows GetLastInputInfo().
 * When user returns from an idle period (>30 min), delivers a
 * spoken + push digest of missed messages grouped by source.
 *
 * Architecture:
 *   1. Polls system idle time every 60s via PowerShell
 *   2. Tracks idle→active transitions
 *   3. Collects missed messages while user is idle
 *   4. On return: TTS greeting + push notification digest
 *
 * @module ShadowInboxDigest
 */

// ============================================================
// Deps Interface (Dependency Injection — never import CoreKernel)
// ============================================================

export interface ShadowInboxDigestDeps {
    speakTTS: (text: string) => Promise<void>;
    pushNotification: (title: string, body: string) => void;
    getUnreadZaloCount: () => number;
    getUnreadEmailCount: () => number;
    getUnreadTelegramCount: () => number;
    isAgentBusy: () => boolean;
}

// ============================================================
// Types
// ============================================================

interface MissedMessage {
    source: string;
    sender: string;
    preview: string;
    timestamp: number;
}

// ============================================================
// Constants
// ============================================================

/** User is considered idle after 30 minutes of no input */
const IDLE_THRESHOLD_MS = 30 * 60 * 1000;

/** User is considered "returned" when idle drops below 5 seconds */
const RETURN_THRESHOLD_MS = 5_000;

/** Check interval: every 60 seconds */
const CHECK_INTERVAL_MS = 60_000;

/**
 * PowerShell script to retrieve system idle time in milliseconds.
 * Uses Win32 GetLastInputInfo via P/Invoke — no native C++ addons.
 */
const PS_GET_IDLE_MS = `
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class IdleTime {
    [StructLayout(LayoutKind.Sequential)]
    struct LASTINPUTINFO {
        public uint cbSize;
        public uint dwTime;
    }
    [DllImport("user32.dll")]
    static extern bool GetLastInputInfo(ref LASTINPUTINFO plii);
    public static int GetIdleMs() {
        LASTINPUTINFO lii = new LASTINPUTINFO();
        lii.cbSize = (uint)Marshal.SizeOf(typeof(LASTINPUTINFO));
        if (!GetLastInputInfo(ref lii)) return 0;
        return (int)(Environment.TickCount - (int)lii.dwTime);
    }
}
'@
[IdleTime]::GetIdleMs()
`.trim();

// ============================================================
// ShadowInboxDigest Daemon
// ============================================================

export class ShadowInboxDigest {
    #deps: ShadowInboxDigestDeps;
    #intervalRef: ReturnType<typeof setInterval> | null = null;
    #isUserIdle = false;
    #lastActiveTime = Date.now();
    #missedMessages: MissedMessage[] = [];
    #idleStartTime: number | null = null;

    constructor(deps: ShadowInboxDigestDeps) {
        this.#deps = deps;
    }

    // ---- Lifecycle ----

    public start(): void {
        if (this.#intervalRef) return;

        this.#intervalRef = setInterval(() => {
            this.#tick().catch(err => {
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[ShadowInboxDigest] Tick error: ${msg}`);
            });
        }, CHECK_INTERVAL_MS);
        this.#intervalRef.unref(); // Prevent zombie timer

        logger.info("[ShadowInboxDigest] 👁️ Started — monitoring user idle state (check: 60s, threshold: 30min).");
    }

    public dispose(): void {
        if (this.#intervalRef) {
            clearInterval(this.#intervalRef);
            this.#intervalRef = null;
        }
        this.#missedMessages = [];
        this.#isUserIdle = false;
        this.#idleStartTime = null;
        logger.info("[ShadowInboxDigest] 🛑 Disposed.");
    }

    // ---- Public API (for external hooks) ----

    /**
     * External services call this to register missed messages while user is idle.
     * E.g., ZaloPolling, TelegramBridge, EmailClientManager.
     */
    public addMissedMessage(source: string, sender: string, preview: string): void {
        if (!this.#isUserIdle) return; // Only collect while user is away

        this.#missedMessages.push({
            source,
            sender,
            preview: preview.substring(0, 200),
            timestamp: Date.now(),
        });
        logger.debug(`[ShadowInboxDigest] Recorded missed message from ${source}:${sender}`);
    }

    /**
     * Returns whether the user is currently considered idle.
     */
    public isIdle(): boolean {
        return this.#isUserIdle;
    }

    // ---- Internal Tick ----

    async #tick(): Promise<void> {
        const idleMs = await this.#getSystemIdleMs();

        if (idleMs >= IDLE_THRESHOLD_MS && !this.#isUserIdle) {
            // === User went idle ===
            this.#isUserIdle = true;
            this.#idleStartTime = Date.now() - idleMs;
            logger.info(`[ShadowInboxDigest] 💤 User idle detected (${Math.round(idleMs / 60000)} min). Collecting missed messages...`);
        }

        if (idleMs < RETURN_THRESHOLD_MS && this.#isUserIdle) {
            // === User returned ===
            this.#isUserIdle = false;
            this.#lastActiveTime = Date.now();

            if (this.#missedMessages.length > 0 || this.#hasUnreadCounts()) {
                await this.#deliverDigest();
            }

            this.#idleStartTime = null;
        }
    }

    /**
     * Check if there are unread messages via deps counters.
     */
    #hasUnreadCounts(): boolean {
        try {
            const zalo = this.#deps.getUnreadZaloCount();
            const email = this.#deps.getUnreadEmailCount();
            const telegram = this.#deps.getUnreadTelegramCount();
            return (zalo + email + telegram) > 0;
        } catch {
            return false;
        }
    }

    /**
     * Deliver the digest to user via TTS + push notification.
     */
    async #deliverDigest(): Promise<void> {
        const messages = [...this.#missedMessages];
        this.#missedMessages = [];

        // Group by source
        const groups = new Map<string, MissedMessage[]>();
        for (const msg of messages) {
            const existing = groups.get(msg.source) ?? [];
            existing.push(msg);
            groups.set(msg.source, existing);
        }

        // Also pull current unread counts
        let zaloCount = 0;
        let emailCount = 0;
        let telegramCount = 0;
        try {
            zaloCount = this.#deps.getUnreadZaloCount();
            emailCount = this.#deps.getUnreadEmailCount();
            telegramCount = this.#deps.getUnreadTelegramCount();
        } catch {
            // Non-critical — counters may fail
        }

        // Build digest summary
        const parts: string[] = [];

        if (groups.has("zalo") || zaloCount > 0) {
            const zaloMsgs = groups.get("zalo") ?? [];
            const count = Math.max(zaloMsgs.length, zaloCount);
            const senders = [...new Set(zaloMsgs.map(m => m.sender))].slice(0, 3);
            const senderStr = senders.length > 0 ? ` (từ ${senders.join(", ")})` : "";
            parts.push(`${count} tin nhắn Zalo${senderStr}`);
        }

        if (groups.has("telegram") || telegramCount > 0) {
            const tgMsgs = groups.get("telegram") ?? [];
            const count = Math.max(tgMsgs.length, telegramCount);
            parts.push(`${count} tin Telegram`);
        }

        if (groups.has("email") || emailCount > 0) {
            const emailMsgs = groups.get("email") ?? [];
            const count = Math.max(emailMsgs.length, emailCount);
            parts.push(`${count} email`);
        }

        if (parts.length === 0) {
            logger.debug("[ShadowInboxDigest] User returned but no digest to deliver.");
            return;
        }

        // Calculate idle duration
        const idleDuration = this.#idleStartTime
            ? Math.round((Date.now() - this.#idleStartTime) / 60000)
            : 0;
        const durationStr = idleDuration > 0 ? ` (sếp đi vắng ${idleDuration} phút)` : "";

        // Compose TTS greeting
        const digestSummary = parts.join(", ");
        const ttsText = `Chào sếp quay lại${durationStr}. Trong lúc sếp đi vắng có ${digestSummary}. Không có gì cháy nhà đâu ạ.`;

        // Build push notification body with details
        let pushBody = `📬 Tổng hợp tin nhắn:\n`;
        for (const [source, msgs] of groups) {
            pushBody += `\n🔹 ${source.toUpperCase()} (${msgs.length}):\n`;
            for (const msg of msgs.slice(0, 5)) {
                pushBody += `   • ${msg.sender}: "${msg.preview.substring(0, 80)}"\n`;
            }
            if (msgs.length > 5) {
                pushBody += `   ... và ${msgs.length - 5} tin khác\n`;
            }
        }

        // Deliver
        try {
            if (!this.#deps.isAgentBusy()) {
                await this.#deps.speakTTS(ttsText);
            }
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[ShadowInboxDigest] TTS failed: ${errMsg}`);
        }

        try {
            this.#deps.pushNotification("📬 Tổng hợp khi sếp vắng", pushBody);
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[ShadowInboxDigest] Push notification failed: ${errMsg}`);
        }

        logger.info(`[ShadowInboxDigest] ✅ Digest delivered: ${digestSummary}`);
    }

    /**
     * Get system idle time in milliseconds via PowerShell GetLastInputInfo.
     * Returns 0 on error (treats as "not idle").
     */
    async #getSystemIdleMs(): Promise<number> {
        try {
            const { stdout } = await execAsync(
                `powershell -NoProfile -NonInteractive -Command "${PS_GET_IDLE_MS.replace(/\n/g, " ")}"`,
                { timeout: 10_000 }
            );
            const ms = parseInt(stdout.trim(), 10);
            return isNaN(ms) ? 0 : ms;
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.debug(`[ShadowInboxDigest] GetLastInputInfo failed: ${errMsg}`);
            return 0;
        }
    }
}
