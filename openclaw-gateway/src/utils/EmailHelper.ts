/**
 * EmailHelper — Shared IMAP Connection & PII Sanitization Utility
 * ================================================================
 * Eliminates code duplication between ReadEmails.ts and
 * CheckImportantEmailsToday.ts.
 *
 * Provides:
 *   - createImapClient() — shared IMAP connection factory
 *   - sanitizeEmailContent() — shared PII redaction for email output
 *   - getEmailCredentials() — env var extraction with quote stripping
 */
import { ImapFlow } from "imapflow";

// ============================================================
// Credential Extraction
// ============================================================

export interface EmailCredentials {
    host: string;
    port: number;
    user: string;
    pass: string;
}

/**
 * Extract and validate email credentials from environment variables.
 * Strips surrounding quotes from EMAIL_USER and EMAIL_PASS.
 * Returns null if any required credential is missing.
 */
export function getEmailCredentials(): EmailCredentials | null {
    const host = process.env.EMAIL_HOST;
    const port = Number.parseInt(process.env.EMAIL_PORT || "993", 10);
    const user = process.env.EMAIL_USER?.replaceAll(/^"|"$/g, "");
    const pass = process.env.EMAIL_PASS?.replaceAll(/^"|"$/g, "").replace(/\s+/g, "");

    if (!host || !user || !pass) return null;

    return { host, port, user, pass };
}

// ============================================================
// IMAP Client Factory
// ============================================================

/**
 * Create a configured ImapFlow client from environment credentials.
 * Returns null if credentials are missing.
 */
export function createImapClient(credentials: EmailCredentials): ImapFlow {
    return new ImapFlow({
        host: credentials.host,
        port: credentials.port,
        secure: credentials.port === 993,
        auth: { user: credentials.user, pass: credentials.pass },
        logger: false, // Suppress verbose ImapFlow logs
    });
}

// ============================================================
// PII Sanitization
// ============================================================

/**
 * Sanitize email content by redacting URLs and numeric codes.
 * Shared between ReadEmails and CheckImportantEmailsToday output.
 */
export function sanitizeEmailContent(str: string): string {
    return str
        .replaceAll(/https?:\/\/[^\s]+/g, "[SECURE_LINK]")
        .replaceAll(/\d{5,15}/g, "[REDACTED_CODE]");
}

// ============================================================
// UID Array Normalization
// ============================================================

/**
 * Normalize IMAP search results to a number array.
 * ImapFlow may return Array, Set, or other iterable formats.
 */
export function normalizeUids(uids: unknown): number[] {
    if (Array.isArray(uids)) return uids;
    if (uids && typeof uids === "object") return Array.from(uids as Iterable<number>);
    return [];
}
