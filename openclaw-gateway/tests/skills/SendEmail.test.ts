import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as SendEmail from "../../src/skills/social/SendEmail";
import { HITLGuard } from "../../src/security/HITLGuard";
import * as nodemailer from "nodemailer";

// Mock ESM module
vi.mock("nodemailer", () => ({
    createTransport: vi.fn()
}));

describe("SendEmail Skill", () => {
    beforeEach(() => {
        vi.stubEnv("EMAIL_HOST", "smtp.test.com");
        vi.stubEnv("EMAIL_USER", "user@test.com");
        vi.stubEnv("EMAIL_PASS", "secret");
    });

    afterEach(() => {
        vi.unstubAllEnvs();
        vi.restoreAllMocks();
    });

    it("should send email when approved", async () => {
        const approvalMock = vi.spyOn(HITLGuard, "requestApproval").mockResolvedValue(true);
        const sendMailMock = vi.fn().mockResolvedValue(true);
        
        // Setup mock return value for this specific test
        vi.mocked(nodemailer.createTransport).mockReturnValue({
            sendMail: sendMailMock
        } as any);

        const result = await SendEmail.execute({
            to: "test@example.com",
            cc: "cc@example.com",
            subject: "Test Subject",
            body_text: "Hello World"
        });

        expect(approvalMock).toHaveBeenCalled();
        expect(nodemailer.createTransport).toHaveBeenCalled();
        expect(sendMailMock).toHaveBeenCalledWith({
            from: "user@test.com",
            to: "test@example.com",
            cc: "cc@example.com",
            subject: "Test Subject",
            text: "Hello World"
        });
        expect(result).toBe("Email đã được gửi thành công.");
    });

    it("should return rejection message when not approved", async () => {
        const approvalMock = vi.spyOn(HITLGuard, "requestApproval").mockResolvedValue(false);

        const result = await SendEmail.execute({
            to: "test@example.com",
            subject: "Test Subject",
            body_text: "Hello World"
        });

        expect(approvalMock).toHaveBeenCalled();
        expect(result).toBe("Lỗi: Người dùng đã từ chối gửi email này.");
    });

    it("should throw HITLRejectedError on timeout", async () => {
        const approvalMock = vi.spyOn(HITLGuard, "requestApproval").mockRejectedValue(new Error("REJECTED_BY_TIMEOUT"));

        await expect(SendEmail.execute({
            to: "test@example.com",
            subject: "Test Subject",
            body_text: "Hello World"
        })).rejects.toThrow("HITLRejectedError: REJECTED_BY_TIMEOUT");
    });
});
