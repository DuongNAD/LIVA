/**
 * SendZaloBot.test.ts — Unit Tests for Zalo Bot Messaging Skill
 * ===============================================================
 * Tests Bot Creator path, OA path, config validation, and error handling.
 * All network calls mocked via vi.mock (AI_CONTEXT §8).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { HITLGuard } from "../../src/security/HITLGuard";

// ─── Mock safeFetch and HITLGuard (prevent real HTTP and hangs) ───
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
}));
vi.mock("../../src/security/HITLGuard", () => ({
    HITLGuard: {
        requestApproval: async () => true,
        getPendingByChannel: () => null,
        respond: () => {},
    },
}));
vi.mock("@security/HITLGuard", () => ({
    HITLGuard: {
        requestApproval: async () => true,
        getPendingByChannel: () => null,
        respond: () => {},
    },
}));
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { safeFetch } from "../../src/utils/HttpClient";
const mockFetch = vi.mocked(safeFetch);

// ─── Tests ───
describe("SendZaloBot Skill", () => {
    let savedEnv: NodeJS.ProcessEnv;

    beforeEach(() => {
        vi.resetAllMocks();
        savedEnv = { ...process.env };
    });

    afterEach(() => {
        process.env = savedEnv;
    });

    async function loadModule() {
        return await import("../../src/skills/social/SendZaloBot");
    }

    // ──────────────────────────────────────
    //  Metadata
    // ──────────────────────────────────────
    describe("metadata", () => {
        it("should export correct skill name", async () => {
            const { metadata } = await loadModule();
            expect(metadata.name).toBe("send_zalo_bot");
        });

        it("should require 'message' parameter", async () => {
            const { metadata } = await loadModule();
            expect(metadata.parameters.required).toContain("message");
        });
    });

    // ──────────────────────────────────────
    //  Input Validation
    // ──────────────────────────────────────
    describe("Input Validation", () => {
        it("should reject empty message", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "valid_token";
            process.env.ZALO_USER_ID = "user123";

            const result = await execute({ message: "   " });
            expect(result).toContain("rỗng");
        });

        it("should accept message from fallback fields (text, content)", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "valid_oa_token";
            process.env.ZALO_USER_ID = "user123";

            mockFetch.mockResolvedValueOnce({
                json: async () => ({ error: 0 }),
            } as any);

            const result = await execute({ text: "Hello via text field" } as any);
            expect(result).toContain("thành công");
        });
    });

    // ──────────────────────────────────────
    //  Config Validation
    // ──────────────────────────────────────
    describe("Configuration", () => {
        it("should error when ZALO_OA_ACCESS_TOKEN is missing", async () => {
            const { execute } = await loadModule();
            delete process.env.ZALO_OA_ACCESS_TOKEN;

            const result = await execute({ message: "test" });
            expect(result).toContain("ZALO_OA_ACCESS_TOKEN");
        });

        it("should error when token contains placeholder", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "NHẬP_TOKEN_CỦA_BẠN";

            const result = await execute({ message: "test" });
            expect(result).toContain("ZALO_OA_ACCESS_TOKEN");
        });

        it("should error when OA path missing ZALO_USER_ID", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "valid_oa_token_no_colon";
            delete process.env.ZALO_USER_ID;

            const result = await execute({ message: "test" });
            expect(result).toContain("ZALO_USER_ID");
        });
    });

    // ──────────────────────────────────────
    //  Bot Creator Path (token contains ":")
    // ──────────────────────────────────────
    describe("Bot Creator Path", () => {
        it("should send via Bot Creator API when token contains colon", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "bot:creator_token_123";
            process.env.ZALO_USER_ID = "creator_user_456";

            mockFetch.mockResolvedValueOnce({
                json: async () => ({ ok: true }),
            } as any);

            const result = await execute({ message: "Test Bot Creator" });
            expect(result).toContain("Bot Creator");
            expect(mockFetch).toHaveBeenCalledWith(
                expect.stringContaining("zaloplatforms.com"),
                expect.objectContaining({ method: "POST" }),
            );
        });

        it("should auto-detect user ID when not configured (Bot Creator)", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "bot:creator_token_123";
            delete process.env.ZALO_USER_ID;

            // First call: getUpdates to detect user ID
            mockFetch.mockResolvedValueOnce({
                json: async () => ({
                    ok: true,
                    result: { message: { chat: { id: "auto_detected_id" } } },
                }),
            } as any);
            // Second call: sendMessage
            mockFetch.mockResolvedValueOnce({
                json: async () => ({ ok: true }),
            } as any);

            const result = await execute({ message: "Test auto ID" });
            expect(result).toContain("Bot Creator");
        });

        it("should error when auto-detect finds no user", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "bot:creator_token_123";
            delete process.env.ZALO_USER_ID;

            mockFetch.mockResolvedValueOnce({
                json: async () => ({ ok: false, result: null }),
            } as any);

            const result = await execute({ message: "Test" });
            expect(result).toContain("User ID");
        });

        it("should handle Bot Creator API error response", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "bot:creator_token_123";
            process.env.ZALO_USER_ID = "user_123";

            mockFetch.mockResolvedValueOnce({
                json: async () => ({ ok: false, description: "Rate limited" }),
            } as any);

            const result = await execute({ message: "Test" });
            expect(result).toContain("Rate limited");
        });
    });

    // ──────────────────────────────────────
    //  Official Account (OA) Path
    // ──────────────────────────────────────
    describe("OA Path", () => {
        it("should send via OA API when token has no colon", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "oa_token_without_colon";
            process.env.ZALO_USER_ID = "oa_user_789";

            mockFetch.mockResolvedValueOnce({
                json: async () => ({ error: 0 }),
            } as any);

            const result = await execute({ message: "Test OA" });
            expect(result).toContain("Zalo OA");
            expect(mockFetch).toHaveBeenCalledWith(
                expect.stringContaining("openapi.zalo.me"),
                expect.objectContaining({ method: "POST" }),
            );
        });

        it("should handle OA API error response", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "oa_token";
            process.env.ZALO_USER_ID = "oa_user";

            mockFetch.mockResolvedValueOnce({
                json: async () => ({ error: -216, message: "Token expired" }),
            } as any);

            const result = await execute({ message: "Test" });
            expect(result).toContain("Token expired");
        });
    });

    // ──────────────────────────────────────
    //  Network Error Handling
    // ──────────────────────────────────────
    describe("Error Handling", () => {
        it("should catch and return network errors", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "oa_token";
            process.env.ZALO_USER_ID = "oa_user";

            mockFetch.mockRejectedValueOnce(new Error("ECONNREFUSED"));

            const result = await execute({ message: "Test" });
            expect(result).toContain("ECONNREFUSED");
        });

        it("should truncate long messages to 2000 chars (Bot Creator)", async () => {
            const { execute } = await loadModule();
            process.env.ZALO_OA_ACCESS_TOKEN = "bot:token";
            process.env.ZALO_USER_ID = "user";

            mockFetch.mockResolvedValueOnce({
                json: async () => ({ ok: true }),
            } as any);

            const longMsg = "x".repeat(5000);
            await execute({ message: longMsg });

            // Check the body sent to safeFetch
            const callArgs = mockFetch.mock.calls[0];
            const body = JSON.parse(callArgs[1]?.body as string);
            expect(body.text.length).toBeLessThanOrEqual(2000);
        });
    });
});
