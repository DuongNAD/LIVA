import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock safeFetch from HttpClient since TelegramManager uses it instead of raw fetch
const mockSafeFetch = vi.fn();
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: (...args: any[]) => mockSafeFetch(...args),
}));

import { TelegramManager } from "../../src/services/TelegramManager";

describe("TelegramManager", () => {
    let manager: TelegramManager;

    beforeEach(() => {
        vi.stubEnv("TELEGRAM_BOT_TOKEN", "fake_token");
        vi.stubEnv("TELEGRAM_CHAT_ID", "fake_chat_id");
        manager = new TelegramManager();
        vi.useFakeTimers();
        mockSafeFetch.mockReset();
    });

    afterEach(() => {
        vi.unstubAllEnvs();
        vi.restoreAllMocks();
        vi.useRealTimers();
    });

    it("should send message successfully", async () => {
        mockSafeFetch.mockResolvedValueOnce({
            json: async () => ({ ok: true, result: { message_id: 42 } }),
        });

        const result = await manager.sendMessage("hello");
        expect(result).toBe(42);
        expect(mockSafeFetch).toHaveBeenCalledTimes(1);
    });

    it("should return null when bot token is missing", async () => {
        vi.stubEnv("TELEGRAM_BOT_TOKEN", "");
        manager = new TelegramManager();

        const result = await manager.sendMessage("test");
        expect(result).toBeNull();
    });

    it("should retry on HTTP 429 with retry_after", async () => {
        // First call throws HTTP 429 error with retry_after in message
        mockSafeFetch
            .mockRejectedValueOnce(
                new Error('HTTP 429: {"parameters":{"retry_after":5}}')
            )
            .mockResolvedValueOnce({
                json: async () => ({ ok: true, result: { message_id: 99 } }),
            });

        const sendPromise = manager.sendMessage("test message");

        // Let the code reach the setTimeout delay
        await vi.advanceTimersByTimeAsync(5000);

        const result = await sendPromise;
        expect(result).toBe(99);
        expect(mockSafeFetch).toHaveBeenCalledTimes(2);
    });

    it("should throw on non-429 error", async () => {
        mockSafeFetch.mockRejectedValueOnce(new Error("HTTP 500: Internal Server Error"));

        await expect(manager.sendMessage("test")).rejects.toThrow(/TelegramManager Error/);
    });

    it("should edit message successfully", async () => {
        mockSafeFetch.mockResolvedValueOnce({});
        await expect(manager.editMessage(1, "updated")).resolves.not.toThrow();
    });

    it("should skip editMessage when bot token is missing", async () => {
        vi.stubEnv("TELEGRAM_BOT_TOKEN", "");
        manager = new TelegramManager();
        await expect(manager.editMessage(1, "test")).resolves.not.toThrow();
        expect(mockSafeFetch).not.toHaveBeenCalled();
    });

    it("should silently ignore 'message is not modified' error on edit", async () => {
        mockSafeFetch.mockRejectedValueOnce(new Error("message is not modified"));
        await expect(manager.editMessage(1, "test")).resolves.not.toThrow();
    });

    it("should handle other edit errors gracefully", async () => {
        mockSafeFetch.mockRejectedValueOnce(new Error("Network error"));
        await expect(manager.editMessage(1, "test")).resolves.not.toThrow();
    });
});
