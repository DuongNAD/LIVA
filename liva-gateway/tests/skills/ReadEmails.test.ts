/**
 * ReadEmails.test.ts — Unified Email Skill Tests
 * ================================================
 * Tests: config validation, filter modes (all/important/unread),
 * topic search, spam filtering, PII sanitization, time windows.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

const createMockClient = () => ({
    connect: vi.fn().mockResolvedValue(undefined),
    getMailboxLock: vi.fn().mockResolvedValue({ release: vi.fn() }),
    search: vi.fn().mockResolvedValue([]),
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

import { metadata, execute } from "../../src/skills/social/ReadEmails";

describe("ReadEmails (Unified)", () => {
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
            expect(metadata.name).toBe("read_emails");
        });

        it("should be marked as core skill", () => {
            expect(metadata.isCoreSkill).toBe(true);
        });

        it("should define all 4 parameters", () => {
            const props = metadata.parameters.properties;
            expect(props.limit).toBeDefined();
            expect(props.filter).toBeDefined();
            expect(props.topic).toBeDefined();
            expect(props.days).toBeDefined();
        });

        it("should have filter enum values", () => {
            expect(metadata.parameters.properties.filter.enum).toEqual(["all", "important", "unread"]);
        });
    });

    // ──────────────────────────────────────
    //  Configuration
    // ──────────────────────────────────────
    describe("configuration validation", () => {
        it("should return error when EMAIL_HOST is missing", async () => {
            delete process.env.EMAIL_HOST;
            const result = await execute({});
            expect(result).toContain("Configuration Error");
        });

        it("should return error when EMAIL_USER is missing", async () => {
            delete process.env.EMAIL_USER;
            const result = await execute({});
            expect(result).toContain("Configuration Error");
        });

        it("should return error when EMAIL_PASS is missing", async () => {
            delete process.env.EMAIL_PASS;
            const result = await execute({});
            expect(result).toContain("Configuration Error");
        });
    });

    // ──────────────────────────────────────
    //  Empty Inbox
    // ──────────────────────────────────────
    describe("empty inbox", () => {
        it("should return no-email message when search returns empty", async () => {
            mockClient.search.mockResolvedValueOnce([]);
            const result = await execute({});
            expect(result).toContain("No");
            expect(result).toContain("emails found");
        });

        it("should mention 'unread' when filter is unread", async () => {
            mockClient.search.mockResolvedValueOnce([]);
            const result = await execute({ filter: "unread" });
            expect(result).toContain("unread");
        });
    });

    // ──────────────────────────────────────
    //  Filter: all (default)
    // ──────────────────────────────────────
    describe("filter: all (default)", () => {
        it("should fetch and format emails correctly", async () => {
            mockClient.search.mockResolvedValueOnce([101, 102]);
            mockClient.fetchOne.mockResolvedValue({
                source: Buffer.from("fake email source"),
            });
            mockSimpleParser.mockResolvedValue({
                from: { text: "real@company.com" },
                subject: "Project Update",
                date: new Date("2026-05-10"),
                text: "Meeting tomorrow at 3PM.",
                headers: new Map(),
            });

            const result = await execute({ limit: 2 });
            expect(result).toContain("Successfully retrieved");
            expect(result).toContain("Email 1");
            expect(result).toContain("[UID:");
            expect(mockClient.connect).toHaveBeenCalled();
            expect(mockClient.logout).toHaveBeenCalled();
        });

        it("should cap limit to 20", async () => {
            mockClient.search.mockResolvedValueOnce([1]);
            mockClient.fetchOne.mockResolvedValue({ source: Buffer.from("test") });
            mockSimpleParser.mockResolvedValue({
                from: { text: "a@b.com" }, subject: "Hi", date: new Date(),
                text: "Hello", headers: new Map(),
            });

            await execute({ limit: 100 });
            // Should not crash — limit internally capped to 20
        });
    });

    // ──────────────────────────────────────
    //  Filter: important
    // ──────────────────────────────────────
    describe("filter: important", () => {
        it("should flag banking emails as important (score >= 2)", async () => {
            mockClient.search.mockResolvedValueOnce([101]);
            mockClient.fetchOne.mockResolvedValue({
                source: Buffer.from("fake"),
            });
            mockSimpleParser.mockResolvedValue({
                from: { text: "banking@vietcombank.com.vn" },
                subject: "Thông báo giao dịch chuyển khoản thành công",
                date: new Date(),
                text: "Quý khách đã chuyển khoản 500,000 VND.",
                headers: new Map(),
            });

            const result = await execute({ filter: "important" });
            expect(result).toContain("important");
            expect(result).toContain("Score:");
        });

        it("should filter out spam/promotion emails", async () => {
            mockClient.search.mockResolvedValueOnce([103]);
            mockClient.fetchOne.mockResolvedValue({ source: Buffer.from("fake") });
            mockSimpleParser.mockResolvedValue({
                from: { text: "noreply@shopee.vn" },
                subject: "Khuyến mãi SALE 50%",
                date: new Date(),
                text: "Deal hot!",
                headers: new Map([["list-unsubscribe", "<mailto:unsub@shopee.vn>"]]),
            });

            const result = await execute({ filter: "important" });
            expect(result).toContain("No important emails");
        });

        it("should flag security alerts as important", async () => {
            mockClient.search.mockResolvedValueOnce([102]);
            mockClient.fetchOne.mockResolvedValue({ source: Buffer.from("fake") });
            mockSimpleParser.mockResolvedValue({
                from: { text: "security@google.com" },
                subject: "Security alert: New login detected",
                date: new Date(),
                text: "OTP verification code",
                headers: new Map(),
            });

            const result = await execute({ filter: "important" });
            expect(result).toContain("important");
        });
    });

    // ──────────────────────────────────────
    //  Filter: topic keyword
    // ──────────────────────────────────────
    describe("topic search", () => {
        it("should filter by topic keyword in subject", async () => {
            mockClient.search.mockResolvedValueOnce([201, 202]);
            mockClient.fetchOne
                .mockResolvedValueOnce({ source: Buffer.from("e1") })
                .mockResolvedValueOnce({ source: Buffer.from("e2") });

            mockSimpleParser
                .mockResolvedValueOnce({
                    from: { text: "boss@company.com" },
                    subject: "Meeting agenda for tomorrow",
                    date: new Date(), text: "Please review.", headers: new Map(),
                })
                .mockResolvedValueOnce({
                    from: { text: "newsletter@medium.com" },
                    subject: "Top 10 JavaScript tips",
                    date: new Date(), text: "Tips and tricks.", headers: new Map(),
                });

            const result = await execute({ topic: "meeting" });
            expect(result).toContain("meeting");
            expect(result).toContain("Meeting agenda");
            expect(result).not.toContain("JavaScript tips");
        });

        it("should return message when no emails match topic", async () => {
            mockClient.search.mockResolvedValueOnce([301]);
            mockClient.fetchOne.mockResolvedValue({ source: Buffer.from("fake") });
            mockSimpleParser.mockResolvedValue({
                from: { text: "a@b.com" }, subject: "Random stuff",
                date: new Date(), text: "Nothing relevant.", headers: new Map(),
            });

            const result = await execute({ topic: "kubernetes" });
            expect(result).toContain("kubernetes");
            expect(result).toContain("No emails matching");
        });
    });

    // ──────────────────────────────────────
    //  Time window (days)
    // ──────────────────────────────────────
    describe("time window", () => {
        it("should accept days parameter without crashing", async () => {
            mockClient.search.mockResolvedValueOnce([]);
            const result = await execute({ days: 7 });
            expect(result).toContain("7 day");
        });

        it("should clamp days to max 30", async () => {
            mockClient.search.mockResolvedValueOnce([]);
            const result = await execute({ days: 365 });
            // Clamped to 30
            expect(result).toContain("30 day");
        });
    });

    // ──────────────────────────────────────
    //  PII Sanitization
    // ──────────────────────────────────────
    describe("PII sanitization", () => {
        it("should mask URLs and long numbers in report", async () => {
            mockClient.search.mockResolvedValueOnce([300]);
            mockClient.fetchOne.mockResolvedValue({ source: Buffer.from("pii") });
            mockSimpleParser.mockResolvedValue({
                from: { text: "real@company.com" },
                subject: "Your code 12345678 is ready",
                date: new Date(),
                text: "Visit https://secret.com/token and use code 9876543210",
                headers: new Map(),
            });

            const result = await execute({ limit: 1 });
            expect(result).toContain("SECURE_LINK");
            expect(result).toContain("REDACTED_CODE");
        });
    });

    // ──────────────────────────────────────
    //  Error Handling
    // ──────────────────────────────────────
    describe("error handling", () => {
        it("should return IMAP error on connection failure", async () => {
            mockClient.connect.mockRejectedValueOnce(new Error("ECONNREFUSED"));
            const result = await execute({});
            expect(result).toContain("IMAP Error");
            expect(result).toContain("ECONNREFUSED");
        });
    });
});
