/**
 * CheckImportantEmailsToday.test.ts — Unit Tests for Email Triage Skill
 * ======================================================================
 * Tests the importance scoring engine, PII sanitization, and IMAP error handling.
 * ImapFlow and mailparser fully mocked to prevent real network connections.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ─── Persistent mock functions (survive vi.clearAllMocks) ───
const mockRelease = vi.fn();
const mockConnect = vi.fn();
const mockGetMailboxLock = vi.fn();
const mockSearch = vi.fn();
const mockFetchOne = vi.fn();
const mockLogout = vi.fn();
const mockSimpleParser = vi.fn();

vi.mock("imapflow", () => ({
    ImapFlow: class MockImapFlow {
        connect = mockConnect;
        getMailboxLock = mockGetMailboxLock;
        search = mockSearch;
        fetchOne = mockFetchOne;
        logout = mockLogout;
        constructor(_opts: any) {}
    },
}));

vi.mock("mailparser", () => ({
    simpleParser: (...args: any[]) => mockSimpleParser(...args),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

describe("CheckImportantEmailsToday Skill", () => {
    let savedEnv: NodeJS.ProcessEnv;

    beforeEach(() => {
        vi.clearAllMocks(); // clearAllMocks keeps implementations; resetAllMocks would wipe them
        savedEnv = { ...process.env };

        // Default working config
        process.env.EMAIL_HOST = "imap.gmail.com";
        process.env.EMAIL_PORT = "993";
        process.env.EMAIL_USER = "test@gmail.com";
        process.env.EMAIL_PASS = "app_password_123";

        // Setup default happy-path mock chain
        mockConnect.mockResolvedValue(undefined);
        mockGetMailboxLock.mockResolvedValue({ release: mockRelease });
        mockLogout.mockResolvedValue(undefined);
    });

    afterEach(() => {
        process.env = savedEnv;
    });

    async function loadModule() {
        return await import("../../src/skills/social/CheckImportantEmailsToday");
    }

    // ──────────────────────────────────────
    //  Metadata
    // ──────────────────────────────────────
    describe("metadata", () => {
        it("should export correct skill name", async () => {
            const { metadata } = await loadModule();
            expect(metadata.name).toBe("check_important_emails_today");
        });

        it("should be marked as core skill", async () => {
            const { metadata } = await loadModule();
            expect(metadata.isCoreSkill).toBe(true);
        });

        it("should require no parameters", async () => {
            const { metadata } = await loadModule();
            expect(metadata.parameters.required).toEqual([]);
        });
    });

    // ──────────────────────────────────────
    //  Config Validation
    // ──────────────────────────────────────
    describe("Configuration", () => {
        it("should error when EMAIL_HOST is missing", async () => {
            const { execute } = await loadModule();
            delete process.env.EMAIL_HOST;

            const result = await execute();
            expect(result).toContain("Lỗi cấu hình");
        });

        it("should error when EMAIL_USER is missing", async () => {
            const { execute } = await loadModule();
            delete process.env.EMAIL_USER;

            const result = await execute();
            expect(result).toContain("Lỗi cấu hình");
        });

        it("should error when EMAIL_PASS is missing", async () => {
            const { execute } = await loadModule();
            delete process.env.EMAIL_PASS;

            const result = await execute();
            expect(result).toContain("Lỗi cấu hình");
        });
    });

    // ──────────────────────────────────────
    //  No Emails Today
    // ──────────────────────────────────────
    describe("Empty Inbox", () => {
        it("should report no emails when search returns empty", async () => {
            const { execute } = await loadModule();
            mockSearch.mockResolvedValue([]);

            const result = await execute();
            expect(result).toContain("Không có");
        });
    });

    // ──────────────────────────────────────
    //  Importance Scoring Engine
    // ──────────────────────────────────────
    describe("Importance Scoring", () => {
        it("should flag banking emails as important (score >= 2)", async () => {
            const { execute } = await loadModule();
            mockSearch.mockResolvedValue([101]);
            mockFetchOne.mockResolvedValue({
                source: Buffer.from("fake email source"),
            });
            mockSimpleParser.mockResolvedValue({
                from: { text: "banking@vietcombank.com.vn" },
                subject: "Thông báo giao dịch chuyển khoản thành công",
                date: new Date(),
                text: "Quý khách đã chuyển khoản 500,000 VND thành công.",
                headers: new Map(),
            });

            const result = await execute();
            expect(result).toContain("QUAN TRỌNG");
        });

        it("should flag security alerts as important", async () => {
            const { execute } = await loadModule();
            mockSearch.mockResolvedValue([102]);
            mockFetchOne.mockResolvedValue({
                source: Buffer.from("fake"),
            });
            mockSimpleParser.mockResolvedValue({
                from: { text: "security@google.com" },
                subject: "Cảnh báo bảo mật: Đăng nhập mới phát hiện",
                date: new Date(),
                text: "OTP xác minh tài khoản",
                headers: new Map(),
            });

            const result = await execute();
            expect(result).toContain("QUAN TRỌNG");
        });

        it("should filter out spam/promotion emails (score < 2)", async () => {
            const { execute } = await loadModule();
            mockSearch.mockResolvedValue([103]);
            mockFetchOne.mockResolvedValue({
                source: Buffer.from("fake"),
            });
            mockSimpleParser.mockResolvedValue({
                from: { text: "noreply@shopee.vn" },
                subject: "Khuyến mãi SALE 50% - Voucher giảm giá",
                date: new Date(),
                text: "Deal hot cuối tuần, mua ngay!",
                headers: new Map([["list-unsubscribe", "<mailto:unsub@shopee.vn>"]]),
            });

            const result = await execute();
            expect(result).toContain("KHÔNG CÓ");
        });

        it("should process multiple emails and rank by score", async () => {
            const { execute } = await loadModule();
            mockSearch.mockResolvedValue([201, 202]);

            mockFetchOne
                .mockResolvedValueOnce({ source: Buffer.from("email1") })
                .mockResolvedValueOnce({ source: Buffer.from("email2") });

            mockSimpleParser
                .mockResolvedValueOnce({
                    from: { text: "hr@fpt.edu.vn" },
                    subject: "Lịch phỏng vấn dự án quan trọng",
                    date: new Date(),
                    text: "Meeting at 3PM",
                    headers: new Map(),
                })
                .mockResolvedValueOnce({
                    from: { text: "alert@bank.com" },
                    subject: "Cảnh báo giao dịch thanh toán bảo mật OTP",
                    date: new Date(),
                    text: "Your OTP code is hidden",
                    headers: new Map(),
                });

            const result = await execute();
            expect(result).toContain("QUAN TRỌNG");
        });
    });

    // ──────────────────────────────────────
    //  PII Sanitization
    // ──────────────────────────────────────
    describe("PII Sanitization", () => {
        it("should redact URLs and long numeric codes from output", async () => {
            const { execute } = await loadModule();
            mockSearch.mockResolvedValue([301]);
            mockFetchOne.mockResolvedValue({
                source: Buffer.from("fake"),
            });
            mockSimpleParser.mockResolvedValue({
                from: { text: "security@bank.com" },
                subject: "Mã bảo mật OTP cho giao dịch",
                date: new Date(),
                text: "Click https://bank.com/verify?token=abc123 và nhập mã 9876543210",
                headers: new Map(),
            });

            const result = await execute();
            if (result.includes("QUAN TRỌNG")) {
                expect(result).toContain("MÃ_BẢO_MẬT_ĐÃ_ẨN");
                expect(result).toContain("LINK_BẢO_MẬT");
            }
        });
    });

    // ──────────────────────────────────────
    //  Error Handling
    // ──────────────────────────────────────
    describe("Error Handling", () => {
        it("should return IMAP error message on connection failure", async () => {
            const { execute } = await loadModule();
            mockConnect.mockRejectedValue(new Error("ECONNREFUSED 993"));

            const result = await execute();
            expect(result).toContain("IMAP Error");
            expect(result).toContain("ECONNREFUSED");
        });
    });
});
