/**
 * SecurityGateway.test.ts — Zero-Trust Security Layer Tests
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { SecurityGateway } from "../../src/security/SecurityGateway";

describe("SecurityGateway", () => {
    let gateway: SecurityGateway;

    beforeEach(() => {
        gateway = new SecurityGateway(5, 1000); // 5 requests per 1s for testing
    });

    afterEach(() => {
        delete process.env.REMOTE_CONTROL_ENABLED;
        delete process.env.TELEGRAM_ALLOWED_IDS;
        delete process.env.ZALO_ALLOWED_IDS;
    });

    describe("Kill Switch", () => {
        it("should be disabled by default", () => {
            expect(gateway.isRemoteControlEnabled()).toBe(false);
        });

        it("should be enabled when env var is 'true'", () => {
            process.env.REMOTE_CONTROL_ENABLED = "true";
            expect(gateway.isRemoteControlEnabled()).toBe(true);
        });

        it("should be disabled for any other value", () => {
            process.env.REMOTE_CONTROL_ENABLED = "yes";
            expect(gateway.isRemoteControlEnabled()).toBe(false);
        });
    });

    describe("Sender Whitelist", () => {
        it("should allow whitelisted sender", () => {
            process.env.TELEGRAM_ALLOWED_IDS = "111,222,333";
            expect(gateway.isAllowedSender("telegram", "222")).toBe(true);
        });

        it("should block non-whitelisted sender", () => {
            process.env.TELEGRAM_ALLOWED_IDS = "111,222";
            expect(gateway.isAllowedSender("telegram", "999")).toBe(false);
        });

        it("should block all when no whitelist configured (Zero-Trust)", () => {
            expect(gateway.isAllowedSender("telegram", "111")).toBe(false);
        });

        it("should use channel-specific env var", () => {
            process.env.ZALO_ALLOWED_IDS = "aaa,bbb";
            expect(gateway.isAllowedSender("zalo", "aaa")).toBe(true);
            expect(gateway.isAllowedSender("telegram", "aaa")).toBe(false);
        });
    });

    describe("Webhook Signature Verification", () => {
        it("should verify valid HMAC-SHA256 signature", () => {
            const { createHmac } = require("node:crypto");
            const secret = "test_secret_123";
            const payload = '{"event":"message"}';
            const hmac = createHmac("sha256", secret).update(payload).digest("hex");
            const signature = `sha256=${hmac}`;

            expect(gateway.verifyWebhookSignature(payload, signature, secret)).toBe(true);
        });

        it("should reject invalid signature", () => {
            expect(gateway.verifyWebhookSignature("payload", "sha256=invalid", "secret")).toBe(false);
        });

        it("should reject signature with wrong length", () => {
            expect(gateway.verifyWebhookSignature("payload", "sha256=short", "secret")).toBe(false);
        });
    });

    describe("Command Risk Classification", () => {
        it("should classify rm -rf as dangerous", () => {
            expect(gateway.classifyRisk("rm -rf /tmp/data")).toBe("dangerous");
        });

        it("should classify drop database as dangerous", () => {
            expect(gateway.classifyRisk("DROP DATABASE production")).toBe("dangerous");
        });

        it("should classify git push --force as dangerous", () => {
            expect(gateway.classifyRisk("git push --force origin main")).toBe("dangerous");
        });

        it("should classify npm install as moderate", () => {
            expect(gateway.classifyRisk("npm install express")).toBe("moderate");
        });

        it("should classify git push as moderate", () => {
            expect(gateway.classifyRisk("git push origin main")).toBe("moderate");
        });

        it("should classify ls -la as safe", () => {
            expect(gateway.classifyRisk("ls -la")).toBe("safe");
        });

        it("should classify cat file.txt as safe", () => {
            expect(gateway.classifyRisk("cat file.txt")).toBe("safe");
        });
    });

    describe("Rate Limiting", () => {
        it("should allow requests within limit", () => {
            for (let i = 0; i < 5; i++) {
                expect(gateway.checkRateLimit("user1")).toBe(true);
            }
        });

        it("should block requests exceeding limit", () => {
            for (let i = 0; i < 5; i++) {
                gateway.checkRateLimit("user1");
            }
            expect(gateway.checkRateLimit("user1")).toBe(false);
        });

        it("should track different users independently", () => {
            for (let i = 0; i < 5; i++) {
                gateway.checkRateLimit("user1");
            }
            expect(gateway.checkRateLimit("user1")).toBe(false);
            expect(gateway.checkRateLimit("user2")).toBe(true); // Different user
        });
    });

    describe("Full Validation Pipeline", () => {
        it("should block when remote control disabled", () => {
            const result = gateway.validateIncoming("telegram", "111");
            expect(result).toContain("disabled");
        });

        it("should block unauthorized sender", () => {
            process.env.REMOTE_CONTROL_ENABLED = "true";
            process.env.TELEGRAM_ALLOWED_IDS = "222";
            const result = gateway.validateIncoming("telegram", "111");
            expect(result).toContain("whitelist");
        });

        it("should allow valid sender with remote control enabled", () => {
            process.env.REMOTE_CONTROL_ENABLED = "true";
            process.env.TELEGRAM_ALLOWED_IDS = "111";
            const result = gateway.validateIncoming("telegram", "111");
            expect(result).toBeNull();
        });
    });
});
