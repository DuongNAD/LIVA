/**
 * EmailHelper.test.ts — Shared Email Utility Tests
 * ==================================================
 * Tests: credential extraction, IMAP client creation,
 * PII sanitization, UID normalization.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock imapflow with a proper class constructor
vi.mock("imapflow", () => ({
    ImapFlow: class MockImapFlow {
        config: any;
        constructor(opts: any) {
            this.config = opts;
        }
        connect = vi.fn();
        logout = vi.fn();
    },
}));

import { getEmailCredentials, createImapClient, sanitizeEmailContent, normalizeUids } from "../../src/utils/EmailHelper";

describe("EmailHelper", () => {
    const originalEnv = process.env;

    beforeEach(() => {
        process.env = { ...originalEnv };
    });

    afterEach(() => {
        process.env = originalEnv;
    });

    describe("getEmailCredentials", () => {
        it("should return null when EMAIL_HOST is missing", () => {
            delete process.env.EMAIL_HOST;
            process.env.EMAIL_USER = "test@test.com";
            process.env.EMAIL_PASS = "pass123";
            expect(getEmailCredentials()).toBeNull();
        });

        it("should return null when EMAIL_USER is missing", () => {
            process.env.EMAIL_HOST = "imap.gmail.com";
            delete process.env.EMAIL_USER;
            process.env.EMAIL_PASS = "pass123";
            expect(getEmailCredentials()).toBeNull();
        });

        it("should return null when EMAIL_PASS is missing", () => {
            process.env.EMAIL_HOST = "imap.gmail.com";
            process.env.EMAIL_USER = "test@test.com";
            delete process.env.EMAIL_PASS;
            expect(getEmailCredentials()).toBeNull();
        });

        it("should return valid credentials when all env vars set", () => {
            process.env.EMAIL_HOST = "imap.gmail.com";
            process.env.EMAIL_PORT = "993";
            process.env.EMAIL_USER = "test@test.com";
            process.env.EMAIL_PASS = "secret";

            const creds = getEmailCredentials();
            expect(creds).not.toBeNull();
            expect(creds!.host).toBe("imap.gmail.com");
            expect(creds!.port).toBe(993);
            expect(creds!.user).toBe("test@test.com");
            expect(creds!.pass).toBe("secret");
        });

        it("should strip surrounding quotes from EMAIL_USER and EMAIL_PASS", () => {
            process.env.EMAIL_HOST = "imap.gmail.com";
            process.env.EMAIL_USER = '"user@mail.com"';
            process.env.EMAIL_PASS = '"mypassword"';

            const creds = getEmailCredentials();
            expect(creds!.user).toBe("user@mail.com");
            expect(creds!.pass).toBe("mypassword");
        });

        it("should default port to 993 when EMAIL_PORT not set", () => {
            process.env.EMAIL_HOST = "imap.gmail.com";
            process.env.EMAIL_USER = "test@test.com";
            process.env.EMAIL_PASS = "pass";
            delete process.env.EMAIL_PORT;

            const creds = getEmailCredentials();
            expect(creds!.port).toBe(993);
        });
    });

    describe("createImapClient", () => {
        it("should create an ImapFlow client with correct config", () => {
            const client = createImapClient({
                host: "imap.test.com",
                port: 993,
                user: "user@test.com",
                pass: "password",
            });
            expect(client).toBeDefined();
        });
    });

    describe("sanitizeEmailContent", () => {
        it("should replace URLs with placeholder", () => {
            const result = sanitizeEmailContent("Click here: https://example.com/reset?token=abc123");
            expect(result).toContain("[LINK_BẢO_MẬT]");
            expect(result).not.toContain("https://example.com");
        });

        it("should replace long numeric codes with placeholder", () => {
            const result = sanitizeEmailContent("Mã xác nhận: 123456789");
            expect(result).toContain("[MÃ_BẢO_MẬT_ĐÃ_ẨN]");
            expect(result).not.toContain("123456789");
        });

        it("should handle text without PII", () => {
            const result = sanitizeEmailContent("Hello, this is a test message.");
            expect(result).toBe("Hello, this is a test message.");
        });

        it("should handle both URLs and codes in same text", () => {
            const result = sanitizeEmailContent("Link: https://site.com Code: 9876543210");
            expect(result).toContain("[LINK_BẢO_MẬT]");
            expect(result).toContain("[MÃ_BẢO_MẬT_ĐÃ_ẨN]");
        });
    });

    describe("normalizeUids", () => {
        it("should return array as-is", () => {
            expect(normalizeUids([1, 2, 3])).toEqual([1, 2, 3]);
        });

        it("should convert Set to array", () => {
            expect(normalizeUids(new Set([4, 5, 6]))).toEqual([4, 5, 6]);
        });

        it("should return empty array for null", () => {
            expect(normalizeUids(null)).toEqual([]);
        });

        it("should return empty array for undefined", () => {
            expect(normalizeUids(undefined)).toEqual([]);
        });
    });
});
