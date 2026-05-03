import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { TelegramManager } from "../../src/services/TelegramManager";

describe("TelegramManager", () => {
    let manager: TelegramManager;

    beforeEach(() => {
        vi.stubEnv("TELEGRAM_BOT_TOKEN", "fake_token");
        vi.stubEnv("TELEGRAM_CHAT_ID", "fake_chat_id");
        manager = new TelegramManager();
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.unstubAllEnvs();
        vi.restoreAllMocks();
        vi.useRealTimers();
    });

    it("should retry on HTTP 429 with retry_after", async () => {
        const fetchMock = vi.fn()
            .mockResolvedValueOnce({
                ok: false,
                status: 429,
                text: async () => JSON.stringify({ parameters: { retry_after: 5 } })
            })
            .mockResolvedValueOnce({
                ok: true,
                status: 200,
                text: async () => "{}"
            });
        
        vi.stubGlobal("fetch", fetchMock);

        const sendPromise = manager.sendMessage("test message");
        
        // Wait a tiny bit for the code to hit the delay
        await Promise.resolve();
        // Advance timers by the retry delay
        await vi.advanceTimersByTimeAsync(5000);
        
        await sendPromise;

        expect(fetchMock).toHaveBeenCalledTimes(2);
    });

    it("should throw on HTTP 500", async () => {
        const fetchMock = vi.fn().mockResolvedValueOnce({
            ok: false,
            status: 500,
            text: async () => "Internal Server Error"
        });
        vi.stubGlobal("fetch", fetchMock);

        await expect(manager.sendMessage("test")).rejects.toThrow(/TelegramManager Error: HTTP 500/);
    });
});
