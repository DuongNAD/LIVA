import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================================
// Mock safeFetch — prevents real network calls
// ============================================================
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
}));
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { safeFetch } from "../../src/utils/HttpClient";
import { notifyZalo } from "../../src/utils/ZaloNotifier";

const mockFetch = vi.mocked(safeFetch);

// ============================================================
// Tests
// ============================================================
describe("ZaloNotifier", () => {
    let originalToken: string | undefined;
    let originalUserId: string | undefined;

    beforeEach(() => {
        vi.resetAllMocks();
        // Save original env vars
        originalToken = process.env.ZALO_OA_ACCESS_TOKEN;
        originalUserId = process.env.ZALO_USER_ID;
    });

    afterEach(() => {
        // Restore env vars to prevent state leakage
        if (originalToken !== undefined) process.env.ZALO_OA_ACCESS_TOKEN = originalToken;
        else delete process.env.ZALO_OA_ACCESS_TOKEN;
        if (originalUserId !== undefined) process.env.ZALO_USER_ID = originalUserId;
        else delete process.env.ZALO_USER_ID;
    });

    describe("Environment Variable Handling", () => {
        it("should early-return without calling fetch when token is missing", async () => {
            delete process.env.ZALO_OA_ACCESS_TOKEN;
            process.env.ZALO_USER_ID = "12345";

            await notifyZalo("test message");
            expect(mockFetch).not.toHaveBeenCalled();
        });

        it("should early-return without calling fetch when userId is missing", async () => {
            process.env.ZALO_OA_ACCESS_TOKEN = "test-token";
            delete process.env.ZALO_USER_ID;

            await notifyZalo("test message");
            expect(mockFetch).not.toHaveBeenCalled();
        });

        it("should early-return when both env vars are missing", async () => {
            delete process.env.ZALO_OA_ACCESS_TOKEN;
            delete process.env.ZALO_USER_ID;

            await notifyZalo("test message");
            expect(mockFetch).not.toHaveBeenCalled();
        });
    });

    describe("Bot Creator Token Path", () => {
        it("should call Bot Creator API when token contains ':'", async () => {
            process.env.ZALO_OA_ACCESS_TOKEN = "botid:secretkey";
            process.env.ZALO_USER_ID = "user123";
            mockFetch.mockResolvedValueOnce({} as any);

            await notifyZalo("Hello from bot");

            expect(mockFetch).toHaveBeenCalledOnce();
            const [url, options] = mockFetch.mock.calls[0];
            expect(url).toContain("bot-api.zaloplatforms.com");
            expect(url).toContain("botid:secretkey");

            const body = JSON.parse(options!.body as string);
            expect(body.chat_id).toBe("user123");
            expect(body.text).toBe("Hello from bot\n\n#Liva");
        });
    });

    describe("OA API Token Path", () => {
        it("should call OA API when token does NOT contain ':'", async () => {
            process.env.ZALO_OA_ACCESS_TOKEN = "plain-oa-token";
            process.env.ZALO_USER_ID = "user456";
            mockFetch.mockResolvedValueOnce({} as any);

            await notifyZalo("Hello from OA");

            expect(mockFetch).toHaveBeenCalledOnce();
            const [url, options] = mockFetch.mock.calls[0];
            expect(url).toContain("openapi.zalo.me");

            const headers = options!.headers as Record<string, string>;
            expect(headers.access_token).toBe("plain-oa-token");

            const body = JSON.parse(options!.body as string);
            expect(body.recipient.user_id).toBe("user456");
            expect(body.message.text).toBe("Hello from OA\n\n#Liva");
        });
    });

    describe("Error Handling", () => {
        it("should NOT throw when safeFetch fails (fire-and-forget)", async () => {
            process.env.ZALO_OA_ACCESS_TOKEN = "test:token";
            process.env.ZALO_USER_ID = "user789";
            mockFetch.mockRejectedValueOnce(new Error("HTTP 500: Internal Server Error"));

            // Should NOT throw — fire-and-forget pattern
            await expect(notifyZalo("test")).resolves.toBeUndefined();
        });

        it("should NOT throw on network timeout", async () => {
            process.env.ZALO_OA_ACCESS_TOKEN = "token:key";
            process.env.ZALO_USER_ID = "uid";
            mockFetch.mockRejectedValueOnce(new Error("AbortError: timeout"));

            await expect(notifyZalo("msg")).resolves.toBeUndefined();
        });
    });
});
