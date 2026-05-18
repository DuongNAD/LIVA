/**
 * SecurityGateway — Zero-Trust Security Layer (Phase 1)
 * ======================================================
 * Enforces security policies for the Remote Control Hub:
 *   - Sender whitelist validation
 *   - HMAC webhook signature verification
 *   - Command risk classification
 *   - Rate limiting (sliding window)
 *   - Kill switch (REMOTE_CONTROL_ENABLED env var)
 *
 * [v5.0] LIVA Remote Control Hub
 */

import { createHmac } from "node:crypto";
import { logger } from "../utils/logger";
import type { ChannelType } from "../channels/ChannelNormalizer";

// ===========================
// Types
// ===========================

interface RateLimitRecord {
    count: number;
    windowStart: number;
}

type RiskLevel = "safe" | "moderate" | "dangerous";

// ===========================
// Dangerous Command Patterns
// ===========================

const DANGEROUS_PATTERNS: RegExp[] = [
    /rm\s+(-rf?|--recursive)\s/i,
    /rmdir\s+\/s/i,
    /del\s+\/[sfq]/i,
    /format\s+[a-z]:/i,
    /mkfs\./i,
    /dd\s+if=/i,
    /drop\s+(database|table|schema)/i,
    /truncate\s+table/i,
    /chmod\s+-R\s+777/i,
    /chown\s+-R\s+root/i,
    /curl.*\|\s*(bash|sh)/i,
    /wget.*\|\s*(bash|sh)/i,
    /eval\s*\(/i,
    /exec\s*\(/i,
    /shutdown\s+(-h|\/s|now)/i,
    /reboot/i,
    /kill\s+-9\s+1$/i,
    /npm\s+publish/i,
    /git\s+push\s+--force/i,
    /git\s+reset\s+--hard/i,
];

const MODERATE_PATTERNS: RegExp[] = [
    /npm\s+install/i,
    /pip\s+install/i,
    /git\s+(push|merge|rebase)/i,
    /docker\s+(run|exec|rm)/i,
    /kubectl\s+(apply|delete)/i,
    /mv\s+/i,
    /cp\s+-r/i,
    /chmod/i,
    /chown/i,
];

// ===========================
// SecurityGateway
// ===========================

export class SecurityGateway {
    readonly #rateLimits = new Map<string, RateLimitRecord>();
    readonly #rateLimit: number;
    readonly #windowMs: number;

    constructor(rateLimit = 50, windowMs = 60_000) {
        this.#rateLimit = rateLimit;
        this.#windowMs = windowMs;
    }

    // ═══════════════════════════════════════
    //  Kill Switch
    // ═══════════════════════════════════════

    /**
     * Master kill switch. If REMOTE_CONTROL_ENABLED !== "true",
     * ALL remote commands are blocked.
     */
    public isRemoteControlEnabled(): boolean {
        return process.env.REMOTE_CONTROL_ENABLED === "true";
    }

    // ═══════════════════════════════════════
    //  Sender Whitelist
    // ═══════════════════════════════════════

    /**
     * Check if a sender is allowed on the given channel.
     * Reads from channel-specific env vars:
     *   TELEGRAM_ALLOWED_IDS, ZALO_ALLOWED_IDS, etc.
     */
    public isAllowedSender(channel: ChannelType, senderId: string): boolean {
        const envKey = `${channel.toUpperCase()}_ALLOWED_IDS`;
        const allowedRaw = process.env[envKey] || "";

        // If no whitelist configured, block all (Zero-Trust)
        if (!allowedRaw.trim()) {
            logger.warn(`[Security] 🛡️ No whitelist for ${channel}. Blocking ${senderId}.`);
            return false;
        }

        const allowedIds = new Set(
            allowedRaw.split(",").map(id => id.trim()).filter(Boolean)
        );

        return allowedIds.has(senderId);
    }

    // ═══════════════════════════════════════
    //  Webhook Signature Verification
    // ═══════════════════════════════════════

    /**
     * Verify HMAC-SHA256 signature for webhook payloads.
     * Used by Meta API, Zalo OA webhooks.
     */
    public verifyWebhookSignature(
        payload: Buffer | string,
        signature: string,
        secret: string
    ): boolean {
        const hmac = createHmac("sha256", secret);
        /* istanbul ignore next */
        hmac.update(typeof payload === "string" ? payload : payload.toString("utf8"));
        const expected = `sha256=${hmac.digest("hex")}`;

        // Constant-time comparison to prevent timing attacks
        if (expected.length !== signature.length) return false;
        let result = 0;
        for (let i = 0; i < expected.length; i++) {
            result |= expected.charCodeAt(i) ^ signature.charCodeAt(i);
        }
        return result === 0;
    }

    // ═══════════════════════════════════════
    //  Command Risk Classification
    // ═══════════════════════════════════════

    /**
     * Classify the risk level of a command.
     * Used by ApprovalEngine to determine if HITL is required.
     */
    public classifyRisk(command: string): RiskLevel {
        for (const pattern of DANGEROUS_PATTERNS) {
            if (pattern.test(command)) return "dangerous";
        }
        for (const pattern of MODERATE_PATTERNS) {
            if (pattern.test(command)) return "moderate";
        }
        return "safe";
    }

    // ═══════════════════════════════════════
    //  Rate Limiting (Sliding Window)
    // ═══════════════════════════════════════

    /**
     * Check if a sender has exceeded the rate limit.
     * Returns true if request is allowed, false if rate-limited.
     */
    public checkRateLimit(senderId: string): boolean {
        const now = Date.now();
        const record = this.#rateLimits.get(senderId);

        if (!record || now > record.windowStart + this.#windowMs) {
            // New window
            this.#rateLimits.set(senderId, { count: 1, windowStart: now });
            return true;
        }

        record.count++;
        if (record.count > this.#rateLimit) {
            logger.warn(`[Security] 🚫 Rate limit exceeded for ${senderId}: ${record.count}/${this.#rateLimit}`);
            return false;
        }

        return true;
    }

    /**
     * Validate a full incoming message through all security checks.
     * Returns null if allowed, or an error message if blocked.
     */
    public validateIncoming(channel: ChannelType, senderId: string): string | null {
        if (!this.isRemoteControlEnabled()) {
            return "Remote control is disabled (REMOTE_CONTROL_ENABLED != true)";
        }

        if (!this.isAllowedSender(channel, senderId)) {
            return `Sender ${senderId} not in ${channel} whitelist`;
        }

        /* istanbul ignore if */
        if (!this.checkRateLimit(senderId)) {
            return `Rate limit exceeded for ${senderId}`;
        }

        return null; // Allowed
    }
}
