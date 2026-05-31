import { logger } from "../utils/logger";

/**
 * ChannelDisambiguationGate — Chốt Phân Kênh Tin Nhắn
 * =====================================================
 * Detects when the user wants to send a message but doesn't specify
 * which channel (Zalo, Messenger, Email). Triggers a clarification
 * question instead of letting the LLM guess.
 * 
 * Architecture: Hybrid Approach
 *   - Code-level gate (deterministic, 100% reliable)
 *   - Few-shot examples (teach LLM format, soft guidance)
 * 
 * Features:
 *   - Regex word boundary matching (no false positives from content)
 *   - StructuredMemory preference learning (bypass after 3 uses)
 *   - Pending State Machine (context retention across turns)
 */

// ===========================
// Constants
// ===========================

/** Messaging tools that require channel disambiguation */
export const MESSAGING_TOOLS = new Set([
    "send_zalo_rpa",
    "send_messenger_rpa",
    "send_email",
]);

/**
 * Channel signal patterns — Regex with word boundaries (\b) to avoid
 * false positives like "gửi file zalo.apk" matching as Zalo channel.
 * 
 * Each key maps a tool name to its unique keyword patterns.
 */
export const CHANNEL_SIGNALS: Record<string, RegExp[]> = {
    "send_zalo_rpa": [
        /\bzalo\b/i,
        /\bnhắn zalo\b/i,
        /\bgửi zalo\b/i,
    ],
    "send_messenger_rpa": [
        /\bmessenger\b/i,
        /\bmessager\b/i,
        /\bmesenger\b/i,
        /\bmess\b/i,
        /\bfacebook\b/i,
        /\bfb\b/i,
        /\bnhắn mess\b/i,
    ],
    "send_email": [
        /\bemail\b/i,
        /\bmail\b/i,
        /\bgửi mail\b/i,
        /\bgửi thư\b/i,
        /thư điện tử/i,
    ],
};

/** 
 * Channel reply patterns — maps user replies to tool names.
 * Used by resolveChannelFromReply() to parse responses like "Zalo", "gửi qua mess", etc. 
 */
const CHANNEL_REPLY_MAP: Array<{ pattern: RegExp; tool: string }> = [
    { pattern: /\bzalo\b/i,       tool: "send_zalo_rpa" },
    { pattern: /\bmessenger\b/i,  tool: "send_messenger_rpa" },
    { pattern: /\bmessager\b/i,   tool: "send_messenger_rpa" },
    { pattern: /\bmesenger\b/i,   tool: "send_messenger_rpa" },
    { pattern: /\bmassage\b/i,    tool: "send_messenger_rpa" },
    { pattern: /\bmess\b/i,       tool: "send_messenger_rpa" },
    { pattern: /\bfacebook\b/i,   tool: "send_messenger_rpa" },
    { pattern: /\bfb\b/i,         tool: "send_messenger_rpa" },
    { pattern: /\bemail\b/i,      tool: "send_email" },
    { pattern: /\bmail\b/i,       tool: "send_email" },
];

/** Structured Memory key prefix for channel preferences */
export const CHANNEL_PREF_PREFIX = "channel_pref::";

/** Number of successful sends before auto-bypass gate */
export const PREFERENCE_BYPASS_THRESHOLD = 3;

// ===========================
// Pending State Machine
// ===========================

/** Holds the pending action when gate is activated (context retention) */
export interface PendingChannelAction {
    recipientName: string;
    message: string;
    originalUserText: string;
    timestamp: number;
}

// ===========================
// Core Gate Logic
// ===========================

/**
 * Determines if the user's messaging request is ambiguous (no channel specified).
 * 
 * @param userText - The raw user input text
 * @param toolName - The tool name chosen by the LLM
 * @param recipientName - The recipient name extracted from tool args
 * @param channelPreference - Optional: stored preference from StructuredMemory
 *                            (format: "send_zalo_rpa:5" = tool:count)
 * @returns true if channel is ambiguous and clarification is needed
 */
export function isAmbiguousChannel(
    userText: string,
    toolName: string,
    recipientName?: string,
    channelPreference?: string | null,
): boolean {
    // 1. Not a messaging tool → not our concern
    if (!MESSAGING_TOOLS.has(toolName)) {
        return false;
    }

    // 2. Check StructuredMemory preference (bypass gate if learned)
    if (channelPreference) {
        const [prefTool, countStr] = channelPreference.split(":");
        const count = parseInt(countStr, 10) || 0;
        if (prefTool === toolName && count >= PREFERENCE_BYPASS_THRESHOLD) {
            logger.debug(`[ChannelGate] Bypassing gate for "${recipientName}" — preference: ${prefTool} (${count} uses)`);
            return false;
        }
    }

    // 3. Scan for explicit channel signals using Regex word boundaries
    let foundAnySignal = false;

    for (const [, patterns] of Object.entries(CHANNEL_SIGNALS)) {
        const hasSignal = patterns.some(pattern => pattern.test(userText));
        if (hasSignal) {
            foundAnySignal = true;
            break;
        }
    }

    // 4. No channel signal found → ambiguous!
    if (!foundAnySignal) {
        logger.info(`[ChannelGate] 🔔 Ambiguous channel detected for "${recipientName || "unknown"}": "${userText.substring(0, 60)}"`);
        return true;
    }

    // 5. Signal found → user specified a channel, not ambiguous
    return false;
}

/**
 * Resolves a specific channel signal from user text, returning the matching tool name.
 * Used when the LLM picked a tool but user explicitly mentioned a different channel.
 * 
 * @returns The correct tool name if a channel signal is found, null otherwise
 */
export function resolveChannelSignal(userText: string): string | null {
    for (const [tool, patterns] of Object.entries(CHANNEL_SIGNALS)) {
        if (patterns.some(pattern => pattern.test(userText))) {
            return tool;
        }
    }
    return null;
}

/**
 * Builds a user-friendly clarification message asking which channel to use.
 * Supports Vietnamese and English.
 * 
 * @param recipientName - The recipient name
 * @param userLang - User's language code (e.g., "vi-VN", "en-US")
 * @returns Formatted clarification message for UI
 */
export function buildClarificationMessage(recipientName: string, userLang: string = "vi-VN"): string {
    const isVietnamese = (userLang || "").toLowerCase().startsWith("vi");
    
    if (isVietnamese) {
        return `Bạn muốn nhắn tin cho **${recipientName}** qua kênh nào?\n- 💬 Zalo\n- 📘 Messenger\n- 📧 Email`;
    }
    
    return `Which channel would you like to message **${recipientName}** on?\n- 💬 Zalo\n- 📘 Messenger\n- 📧 Email`;
}

/**
 * Parses the user's reply to a channel clarification question.
 * Returns the resolved tool name, or null if no channel was detected.
 * 
 * @param replyText - The user's reply (e.g., "Zalo", "gửi qua mess", "email")
 * @returns The matching tool name, or null
 */
export function resolveChannelFromReply(replyText: string): string | null {
    if (!replyText || replyText.trim().length === 0) return null;

    for (const entry of CHANNEL_REPLY_MAP) {
        if (entry.pattern.test(replyText)) {
            return entry.tool;
        }
    }

    return null;
}

/**
 * Builds the StructuredMemory key for a recipient's channel preference.
 */
export function buildPreferenceKey(recipientName: string): string {
    return `${CHANNEL_PREF_PREFIX}${recipientName.toLowerCase().trim()}`;
}

/**
 * Builds the preference value string (tool:count format).
 */
export function buildPreferenceValue(toolName: string, currentValue?: string | null): string {
    if (currentValue) {
        const [prefTool, countStr] = currentValue.split(":");
        const count = parseInt(countStr, 10) || 0;
        if (prefTool === toolName) {
            return `${toolName}:${count + 1}`;
        }
        // Different tool → reset count (user changed preference)
    }
    return `${toolName}:1`;
}
