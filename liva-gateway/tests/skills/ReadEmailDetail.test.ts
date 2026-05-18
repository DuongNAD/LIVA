/**
 * ReadEmailDetail.test.ts — Read Full Email by UID
 * ==================================================
 * Tests: config validation, UID fetch, full content display,
 * PII sanitization, attachments, truncation, error handling.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

const createMockClient = () => ({
    connect: vi.fn().mockResolvedValue(undefined),
    getMailboxLock: vi.fn().mockResolvedValue({ release: vi.fn() }),
    fetchOne: vi.fn().mockResolvedValue(null),
    logout: vi.fn().mockResolvedValue(undefined),
});

let mockClient = createMockClient();

vi.mock("imapflow", () => {
    const MockImapFlow = function(this: any, _opts: any) {
        Object.assign(this, mockClient);
    };
    return { ImapFlow: MockImapFlow };
});

const mockSimpleParser = vi.fn();
vi.mock("mailparser", () => ({
    simpleParser: (...args: any[]) => mockSimpleParser(...args),
}));

import { metadata, execute } from "../../src/skills/social/ReadEmailDetail";

describe("ReadEmailDetail", () => {
    const originalEnv = process.env;

    beforeEach(() => {
        vi.clearAllMocks();
        mockClient = createMockClient();
        process.env = {
            ...originalEnv,
            EMAIL_HOST: "imap.test.com",
            EMAIL_PORT: "993",
            EMAIL_USER: "test@test.com",
            EMAIL_PASS: "testpass123",
        };
    });

    // ──────────────────────────────────────
    //  Metadata
    // ──────────────────────────────────────
    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("read_email_detail");
        });

        it("should require uid parameter", () => {
            expect(metadata.parameters.required).toContain("uid");
        });
    });

    // ──────────────────────────────────────
    //  Configuration
    // ──────────────────────────────────────
    describe("configuration validation", () => {
        it("should return error when EMAIL_HOST is missing", async () => {
            delete process.env.EMAIL_HOST;
            const result = await execute({ uid: 12345 });
            expect(result).toContain("Configuration Error");
        });

        it("should return error for missing UID", async () => {
            const result = await execute({ uid: undefined as any });
            expect(result).toContain("Error");
            expect(result).toContain("UID");
        });
    });

    // ──────────────────────────────────────
    //  Successful Fetch
    // ──────────────────────────────────────
    describe("full email fetch", () => {
        it("should return full email content with all headers", async () => {
            mockClient.fetchOne.mockResolvedValue({
                source: Buffer.from("raw email data"),
            });
            mockSimpleParser.mockResolvedValue({
                from: { text: "boss@company.com" },
                to: { text: "me@mymail.com" },
                cc: { text: "colleague@company.com" },
                subject: "Quarterly Review Meeting",
                date: new Date("2026-05-10T10:00:00"),
                text: "Hi team,\n\nPlease prepare the Q2 report for tomorrow's meeting.\n\nBest regards,\nBoss",
                html: null,
                attachments: [],
                headers: new Map(),
            });

            const result = await execute({ uid: 12345 });
            expect(result).toContain("EMAIL DETAIL");
            expect(result).toContain("UID: 12345");
            expect(result).toContain("boss@company.com");
            expect(result).toContain("me@mymail.com");
            expect(result).toContain("colleague@company.com");
            expect(result).toContain("Quarterly Review Meeting");
            expect(result).toContain("Q2 report");
            expect(result).toContain("Full Content");
        });

        it("should display attachment info", async () => {
            mockClient.fetchOne.mockResolvedValue({ source: Buffer.from("data") });
            mockSimpleParser.mockResolvedValue({
                from: { text: "hr@company.com" },
                to: { text: "me@mymail.com" },
                subject: "Offer Letter",
                date: new Date(),
                text: "Please find attached.",
                attachments: [
                    { filename: "offer_letter.pdf", size: 245760 },
                    { filename: "benefits.xlsx", size: 102400 },
                ],
                headers: new Map(),
            });

            const result = await execute({ uid: 99999 });
            expect(result).toContain("offer_letter.pdf");
            expect(result).toContain("240.0 KB");
            expect(result).toContain("benefits.xlsx");
        });

        it("should fallback to stripped HTML when text is empty", async () => {
            mockClient.fetchOne.mockResolvedValue({ source: Buffer.from("data") });
            mockSimpleParser.mockResolvedValue({
                from: { text: "newsletter@tech.com" },
                to: { text: "me@test.com" },
                subject: "Weekly Digest",
                date: new Date(),
                text: "", // No plain text
                html: "<html><body><h1>Hello</h1><p>This is <b>important</b> news.</p></body></html>",
                attachments: [],
                headers: new Map(),
            });

            const result = await execute({ uid: 5555 });
            expect(result).toContain("Hello");
            expect(result).toContain("important");
            expect(result).not.toContain("<h1>");
        });
    });

    // ──────────────────────────────────────
    //  Edge Cases
    // ──────────────────────────────────────
    describe("edge cases", () => {
        it("should handle email not found", async () => {
            mockClient.fetchOne.mockResolvedValue(null);

            const result = await execute({ uid: 99999 });
            expect(result).toContain("not found");
        });

        it("should truncate very long emails", async () => {
            const longBody = "A".repeat(5000);
            mockClient.fetchOne.mockResolvedValue({ source: Buffer.from("data") });
            mockSimpleParser.mockResolvedValue({
                from: { text: "a@b.com" },
                to: { text: "c@d.com" },
                subject: "Long email",
                date: new Date(),
                text: longBody,
                attachments: [],
                headers: new Map(),
            });

            const result = await execute({ uid: 111 });
            expect(result).toContain("truncated");
            expect(result.length).toBeLessThan(longBody.length);
        });
    });

    // ──────────────────────────────────────
    //  PII Sanitization
    // ──────────────────────────────────────
    describe("PII sanitization", () => {
        it("should mask URLs and codes in content", async () => {
            mockClient.fetchOne.mockResolvedValue({ source: Buffer.from("pii") });
            mockSimpleParser.mockResolvedValue({
                from: { text: "bank@vietcombank.com" },
                to: { text: "me@test.com" },
                subject: "OTP code 12345678",
                date: new Date(),
                text: "Your verification code is 9876543210. Click https://secret.bank.com/verify to confirm.",
                attachments: [],
                headers: new Map(),
            });

            const result = await execute({ uid: 200 });
            expect(result).toContain("SECURE_LINK");
            expect(result).toContain("REDACTED_CODE");
        });
    });

    // ──────────────────────────────────────
    //  Error Handling
    // ──────────────────────────────────────
    describe("error handling", () => {
        it("should return IMAP error on connection failure", async () => {
            mockClient.connect.mockRejectedValueOnce(new Error("ETIMEDOUT"));
            const result = await execute({ uid: 12345 });
            expect(result).toContain("IMAP Error");
            expect(result).toContain("ETIMEDOUT");
        });
    });
});
