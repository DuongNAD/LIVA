import * as fs from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";

/**
 * RPAGuardrails — Centralized Security Module for All RPA Skills
 * ===============================================================
 * Provides multi-layer defense for AI-driven desktop automation:
 *   1. PII Detection — Prevents leaking personal identifiable information
 *   2. Credential Leak Prevention — Blocks API keys, tokens, passwords
 *   3. Action Audit Log — Records every RPA action for accountability
 *   4. Sensitive Domain Detection — Warns on banking/payment sites
 *   5. Rate Limiting — Prevents spam/rapid-fire actions
 *   6. Content Filtering — Blocks harmful or suspicious content
 * 
 * Philosophy: "Security through guardrails, NOT through capability removal"
 */

// ===========================
// Types & Interfaces
// ===========================

export interface PIIScanResult {
    hasPII: boolean;
    detectedTypes: string[];
    redactedText: string;
    warnings: string[];
}

export interface FilterResult {
    safe: boolean;
    reason: string;
    filteredContent: string;
}

export interface AuditEntry {
    timestamp: string;
    skillName: string;
    action: string;
    target: string;
    contentPreview: string;
    piiDetected: boolean;
    outcome: "allowed" | "blocked" | "warned";
}

// ===========================
// PII Patterns
// ===========================

const PII_PATTERNS = {
    // Vietnamese CMND/CCCD (9 or 12 digits)
    CCCD: {
        regex: /\b(0[0-9]{2}[0-9]{3}[0-9]{6})\b|\b([0-9]{9})\b/g,
        label: "Số CMND/CCCD",
        mask: "***CCCD***"
    },
    // Credit card numbers (13-19 digits, with optional spaces/dashes)
    CREDIT_CARD: {
        regex: /\b(?:\d[ -]*?){13,19}\b/g,
        label: "Số thẻ ngân hàng",
        mask: "***CARD***",
        // Extra validation: Luhn check
        validate: (match: string) => {
            const digits = match.replaceAll(/\D/g, "");
            if (digits.length < 13 || digits.length > 19) return false;
            // Luhn algorithm to validate credit card numbers
            let sum = 0;
            let isEven = false;
            for (let i = digits.length - 1; i >= 0; i--) {
                let digit = Number.parseInt(digits[i], 10);
                if (isEven) {
                    digit *= 2;
                    if (digit > 9) digit -= 9;
                }
                sum += digit;
                isEven = !isEven;
            }
            return sum % 10 === 0;
        }
    },
    // Vietnamese phone numbers
    PHONE_VN: {
        regex: /\b(0[235789][0-9]{8})\b|\b(\+84[235789][0-9]{8})\b/g,
        label: "Số điện thoại VN",
        mask: "***PHONE***"
    },
    // Email addresses
    EMAIL: {
        regex: /\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b/g,
        label: "Email",
        mask: "***EMAIL***"
    },
    // Vietnamese bank account numbers (common formats)
    BANK_ACCOUNT: {
        regex: /\b(STK|stk|Số tài khoản|số tài khoản)[:\s]*([0-9]{6,20})\b/gi,
        label: "Số tài khoản ngân hàng",
        mask: "***BANK***"
    }
};

// ===========================
// Credential Patterns
// ===========================

const CREDENTIAL_PATTERNS = [
    // API Keys
    { regex: /\b(api[_-]?key|apikey)\s*[=:]\s*['"]?([a-zA-Z0-9_\-]{20,})['"]?/gi, label: "API Key" },
    // Bearer tokens
    { regex: /Bearer\s+[a-zA-Z0-9_\-\.]{20,}/gi, label: "Bearer Token" },
    // Generic secrets
    { regex: /\b(secret|password|passwd|pwd)\s*[=:]\s*['"]?([^\s'"]{8,})['"]?/gi, label: "Secret/Password" },
    // AWS-style keys
    { regex: /\b(AKIA[0-9A-Z]{16})\b/g, label: "AWS Access Key" },
    // Environment variable patterns
    { regex: /\b(process\.env\.[A-Z_]+)\b/g, label: "Env Variable Reference" },
    // Private keys
    { regex: /-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY-----/g, label: "Private Key" },
    // Zalo/Bot tokens
    { regex: /\b[0-9]+:[A-Za-z0-9_-]{30,}\b/g, label: "Bot Token" },
];

// ===========================
// Prompt Injection Patterns
// ===========================

const INJECTION_PATTERNS = [
    /IGNORE\s+(ALL\s+)?PREVIOUS\s+INSTRUCTIONS/i,
    /IGNORE\s+(ALL\s+)?ABOVE/i,
    /DISREGARD\s+(ALL\s+)?PREVIOUS/i,
    /YOU\s+ARE\s+NOW\s+(?:A|AN)\s+/i,
    /\<\s*system\s*\>/i,
    /\<\s*\/\s*system\s*\>/i,
    /OVERRIDE\s+SAFETY/i,
    /JAILBREAK/i,
    /DO\s+NOT\s+FOLLOW\s+RULES/i,
    /ACT\s+AS\s+(?:IF|THOUGH)\s+YOU\s+(?:ARE|HAVE)\b/i,
];

// ===========================
// Sensitive Domains
// ===========================

const SENSITIVE_DOMAINS = [
    // Banking
    "vietcombank.com.vn", "techcombank.com.vn", "tpbank.vn", "mbbank.com.vn",
    "acb.com.vn", "bidv.com.vn", "agribank.com.vn", "sacombank.com.vn",
    "vpbank.com", "vib.com.vn", "hdbank.com.vn", "shinhanbank.com.vn",
    // Payment
    "momo.vn", "zalopay.vn", "vnpay.vn", "paypal.com", "stripe.com",
    // Crypto
    "binance.com", "coinbase.com", "blockchain.com",
    // Government
    "dichvucong.gov.vn", "thuohuong.gov.vn",
    // Social (login pages only)
    "accounts.google.com", "login.microsoftonline.com",
];

// ===========================
// Rate Limiter State
// ===========================

const rateLimitState = new Map<string, { count: number; windowStart: number }>();
const RATE_LIMIT_WINDOW_MS = 10_000; // 10 seconds
const RATE_LIMIT_MAX = 5; // max 5 actions per window per skill

// ===========================
// Audit Log Config
// ===========================

const AUDIT_LOG_DIR = path.join(process.cwd(), "data", "agents", "liva_core");
const AUDIT_LOG_FILE = path.join(AUDIT_LOG_DIR, "rpa_audit_log.jsonl");

// ===========================
// Main Class
// ===========================

export class RPAGuardrails {

    /**
     * 1. PII Scanner — Detects personal identifiable information in text
     * Returns detailed scan result with detection types and redacted version
     */
    public static scanForPII(text: string): PIIScanResult {
        if (!text) return { hasPII: false, detectedTypes: [], redactedText: text, warnings: [] };

        let redacted = text;
        const detected: string[] = [];
        const warnings: string[] = [];

        for (const [name, pattern] of Object.entries(PII_PATTERNS)) {
            const matches = text.match(pattern.regex);
            if (matches) {
                for (const match of matches) {
                    // Extra validation for credit cards (Luhn check)
                    if ('validate' in pattern && typeof pattern.validate === 'function') {
                        if (!pattern.validate(match)) continue;
                    }

                    // Skip very short matches that might be false positives
                    const cleanMatch = match.replaceAll(/\D/g, "");
                    if (name === "CCCD" && (cleanMatch.length !== 9 && cleanMatch.length !== 12)) continue;

                    detected.push(pattern.label);
                    warnings.push(`⚠️ Phát hiện ${pattern.label}: "${match.substring(0, 4)}****"`);
                    redacted = redacted.replace(match, pattern.mask);
                }
            }
        }

        return {
            hasPII: detected.length > 0,
            detectedTypes: [...new Set(detected)],
            redactedText: redacted,
            warnings
        };
    }

    /**
     * 2. Credential Leak Detector — Scans for API keys, tokens, passwords
     */
    public static scanForCredentials(text: string): { hasCredentials: boolean; types: string[]; warnings: string[] } {
        if (!text) return { hasCredentials: false, types: [], warnings: [] };

        const detected: string[] = [];
        const warnings: string[] = [];

        for (const pattern of CREDENTIAL_PATTERNS) {
            if (pattern.regex.test(text)) {
                detected.push(pattern.label);
                warnings.push(`🔐 Phát hiện ${pattern.label} trong nội dung`);
                // Reset regex lastIndex
                pattern.regex.lastIndex = 0;
            }
        }

        return {
            hasCredentials: detected.length > 0,
            types: [...new Set(detected)],
            warnings
        };
    }

    /**
     * 3. Prompt Injection Guard — Detects attempts to manipulate AI behavior via tool output
     */
    public static detectPromptInjection(text: string): { isInjection: boolean; pattern: string } {
        if (!text) return { isInjection: false, pattern: "" };

        for (const pattern of INJECTION_PATTERNS) {
            if (pattern.test(text)) {
                return { isInjection: true, pattern: pattern.source };
            }
        }

        return { isInjection: false, pattern: "" };
    }

    /**
     * 4. Action Audit Logger — Records every RPA action to JSONL file
     */
    public static logAction(
        skillName: string,
        action: string,
        target: string,
        contentPreview: string = "",
        piiDetected: boolean = false,
        outcome: "allowed" | "blocked" | "warned" = "allowed"
    ): void {
        try {
            if (!fs.existsSync(AUDIT_LOG_DIR)) {
                fs.mkdirSync(AUDIT_LOG_DIR, { recursive: true });
            }

            const entry: AuditEntry = {
                timestamp: new Date().toISOString(),
                skillName,
                action,
                target,
                contentPreview: contentPreview.substring(0, 100), // Only first 100 chars
                piiDetected,
                outcome
            };

            fs.appendFileSync(AUDIT_LOG_FILE, JSON.stringify(entry) + "\n", "utf-8");
        } catch (e) {
            // Audit log failure should never crash the system
            logger.warn(`[RPAGuardrails] Failed to write audit log: ${e}`);
        }
    }

    /**
     * 5. Sensitive Domain Checker — Returns true if URL belongs to a sensitive domain
     */
    public static isSensitiveDomain(url: string): boolean {
        try {
            const hostname = new URL(url).hostname.toLowerCase();
            return SENSITIVE_DOMAINS.some(domain => 
                hostname === domain || hostname.endsWith(`.${domain}`)
            );
        } catch {
            return false;
        }
    }

    /**
     * 6. Rate Limiter — Prevents rapid-fire RPA actions (>5 per 10 seconds per skill)
     */
    public static checkRateLimit(skillName: string): { allowed: boolean; retryAfterMs: number } {
        const now = Date.now();
        const state = rateLimitState.get(skillName);

        if (!state || (now - state.windowStart) > RATE_LIMIT_WINDOW_MS) {
            // New window
            rateLimitState.set(skillName, { count: 1, windowStart: now });
            return { allowed: true, retryAfterMs: 0 };
        }

        if (state.count >= RATE_LIMIT_MAX) {
            const retryAfter = RATE_LIMIT_WINDOW_MS - (now - state.windowStart);
            logger.warn(`[RPAGuardrails] Rate limit hit for ${skillName}: ${state.count}/${RATE_LIMIT_MAX} in window`);
            return { allowed: false, retryAfterMs: retryAfter };
        }

        state.count++;
        return { allowed: true, retryAfterMs: 0 };
    }

    /**
     * 7. Content Filter — Comprehensive pre-send content check
     * Combines PII, credentials, and injection checks into one call
     */
    public static filterContent(content: string, skillName: string = "unknown"): FilterResult {
        // Check PII
        const piiResult = this.scanForPII(content);
        if (piiResult.hasPII) {
            logger.warn(`[RPAGuardrails] PII detected in ${skillName}: ${piiResult.detectedTypes.join(", ")}`);
            this.logAction(skillName, "content_filter", "pii_detection", content, true, "warned");
            return {
                safe: false,
                reason: `Phát hiện thông tin nhạy cảm (${piiResult.detectedTypes.join(", ")}). Nội dung đã được che giấu tự động.`,
                filteredContent: piiResult.redactedText
            };
        }

        // Check credentials
        const credResult = this.scanForCredentials(content);
        if (credResult.hasCredentials) {
            logger.warn(`[RPAGuardrails] Credentials detected in ${skillName}: ${credResult.types.join(", ")}`);
            this.logAction(skillName, "content_filter", "credential_leak", content, false, "blocked");
            return {
                safe: false,
                reason: `CHẶN: Phát hiện ${credResult.types.join(", ")} trong nội dung. Không được gửi thông tin xác thực qua tin nhắn.`,
                filteredContent: "[NỘI DUNG ĐÃ BỊ HỆ THỐNG BẢO MẬT CHẶN]"
            };
        }

        // Check prompt injection in received content
        const injResult = this.detectPromptInjection(content);
        if (injResult.isInjection) {
            logger.warn(`[RPAGuardrails] Prompt injection in ${skillName} output`);
            this.logAction(skillName, "content_filter", "prompt_injection", content, false, "blocked");
            return {
                safe: false,
                reason: `CHẶN: Phát hiện mẫu tấn công injection trong nội dung tool.`,
                filteredContent: "[NỘI DUNG CHỨA MẪU TẤN CÔNG - ĐÃ CHẶN]"
            };
        }

        return { safe: true, reason: "", filteredContent: content };
    }

    /**
     * 8. Full Pre-Action Check — Call before ANY RPA action
     * Returns a go/no-go decision with reasons
     */
    public static preActionCheck(
        skillName: string,
        action: string,
        target: string,
        content: string
    ): { proceed: boolean; warnings: string[]; filteredContent: string } {
        const warnings: string[] = [];

        // Rate limit check
        const rateCheck = this.checkRateLimit(skillName);
        if (!rateCheck.allowed) {
            this.logAction(skillName, action, target, "", false, "blocked");
            return {
                proceed: false,
                warnings: [`⛔ Rate limit: Quá nhiều hành động trong thời gian ngắn. Thử lại sau ${Math.ceil(rateCheck.retryAfterMs / 1000)}s.`],
                filteredContent: content
            };
        }

        // Content filter (PII + credentials + injection)
        const filterCheck = this.filterContent(content, skillName);
        if (!filterCheck.safe) {
            warnings.push(filterCheck.reason);
        }

        // Log the action regardless
        this.logAction(skillName, action, target, content, !filterCheck.safe, filterCheck.safe ? "allowed" : "warned");

        return {
            proceed: true, // Allow action but with warnings
            warnings,
            filteredContent: filterCheck.filteredContent
        };
    }
}
