/**
 * ReadEmails.test.ts — IMAP email reading skill tests
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock imapflow with class-style mock
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

// Mock mailparser
vi.mock("mailparser", () => ({
    simpleParser: vi.fn().mockResolvedValue({
        from: { text: "sender@test.com" },
        subject: "Test Subject",
        date: new Date("2026-01-01"),
        text: "Test email body content here for testing purposes.",
    }),
}));

import { metadata, execute } from "../../src/skills/social/ReadEmails";

describe("ReadEmails", () => {
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

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("read_emails");
        });

        it("should be marked as core skill", () => {
            expect(metadata.isCoreSkill).toBe(true);
        });

        it("should define limit and unreadOnly parameters", () => {
            expect(metadata.parameters.properties.limit).toBeDefined();
            expect(metadata.parameters.properties.unreadOnly).toBeDefined();
        });
    });

    describe("configuration validation", () => {
        it("should return error when EMAIL_HOST is missing", async () => {
            delete process.env.EMAIL_HOST;
            const result = await execute({});
            expect(result).toContain("Lỗi cấu hình");
            expect(result).toContain("EMAIL_HOST");
        });

        it("should return error when EMAIL_USER is missing", async () => {
            delete process.env.EMAIL_USER;
            const result = await execute({});
            expect(result).toContain("Lỗi cấu hình");
        });

        it("should return error when EMAIL_PASS is missing", async () => {
            delete process.env.EMAIL_PASS;
            const result = await execute({});
            expect(result).toContain("Lỗi cấu hình");
        });
    });

    describe("empty inbox", () => {
        it("should return no-email message when search returns empty", async () => {
            mockClient.search.mockResolvedValueOnce([]);
            const result = await execute({});
            expect(result).toContain("Không tìm thấy email");
        });

        it("should mention 'chưa đọc' when unreadOnly is true", async () => {
            mockClient.search.mockResolvedValueOnce([]);
            const result = await execute({ unreadOnly: true });
            expect(result).toContain("chưa đọc");
        });
    });

    describe("successful email fetch", () => {
        it("should fetch and format emails correctly", async () => {
            mockClient.search.mockResolvedValueOnce([101, 102]);
            mockClient.fetchOne.mockResolvedValue({
                source: Buffer.from("fake email source"),
            });

            const result = await execute({ limit: 2 });
            expect(result).toContain("Đã lấy thành công");
            expect(result).toContain("Email 1");
            expect(mockClient.connect).toHaveBeenCalled();
            expect(mockClient.logout).toHaveBeenCalled();
        });

        it("should cap limit to 20", async () => {
            mockClient.search.mockResolvedValueOnce([1]);
            mockClient.fetchOne.mockResolvedValue({
                source: Buffer.from("test"),
            });

            await execute({ limit: 100 });
            // Should not crash — limit internally capped to 20
        });
    });

    describe("spam filtering", () => {
        it("should skip emails from spam senders", async () => {
            const { simpleParser } = await import("mailparser");
            (simpleParser as any).mockResolvedValueOnce({
                from: { text: "noreply@shopee.vn" },
                subject: "Khuyến mãi sốc!",
                date: new Date(),
                text: "Sale sale sale",
            });

            mockClient.search.mockResolvedValueOnce([200]);
            mockClient.fetchOne.mockResolvedValue({
                source: Buffer.from("spam email"),
            });

            const result = await execute({ limit: 5 });
            // Spam email should be filtered, resulting in no valid emails
            expect(result).toContain("không lấy được");
        });
    });

    describe("IMAP connection error", () => {
        it("should return error message on connection failure", async () => {
            mockClient.connect.mockRejectedValueOnce(new Error("ECONNREFUSED"));
            const result = await execute({});
            expect(result).toContain("IMAP Error");
            expect(result).toContain("ECONNREFUSED");
        });
    });

    describe("PII sanitization in output", () => {
        it("should mask URLs and long numbers in report", async () => {
            const { simpleParser } = await import("mailparser");
            (simpleParser as any).mockResolvedValue({
                from: { text: "real@company.com" },
                subject: "Your code 12345678 is ready",
                date: new Date(),
                text: "Visit https://secret.com/token and use code 9876543210",
            });

            mockClient.search.mockResolvedValueOnce([300]);
            mockClient.fetchOne.mockResolvedValue({
                source: Buffer.from("email with pii"),
            });

            const result = await execute({ limit: 1 });
            expect(result).toContain("LINK_BẢO_MẬT");
            expect(result).toContain("MÃ_BẢO_MẬT_ĐÃ_ẨN");
        });
    });
});
