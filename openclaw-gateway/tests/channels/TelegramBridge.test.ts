/**
 * TelegramBridge.test.ts — Telegram Bot Integration Tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock safeFetch
const mockSafeFetch = vi.fn();
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: (...args: any[]) => mockSafeFetch(...args),
}));

import { TelegramBridge } from "../../src/channels/TelegramBridge";

describe("TelegramBridge", () => {
    let bridge: TelegramBridge;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        process.env.TELEGRAM_BOT_TOKEN = "123456:ABC-DEF";
        process.env.TELEGRAM_ALLOWED_IDS = "111222,333444";
        bridge = new TelegramBridge();
    });

    afterEach(() => {
        bridge.stop();
        delete process.env.TELEGRAM_BOT_TOKEN;
        delete process.env.TELEGRAM_ALLOWED_IDS;
        vi.useRealTimers();
    });

    describe("Initialization", () => {
        it("should initialize with bot token and allowed IDs", () => {
            expect(bridge.channelName).toBe("telegram");
        });

        it("should warn when no bot token configured", () => {
            delete process.env.TELEGRAM_BOT_TOKEN;
            const b = new TelegramBridge();
            b.stop();
            // Should not crash
        });
    });

    describe("sendText", () => {
        it("should call Telegram sendMessage API", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ ok: true }),
            });

            await bridge.sendText("111222", "Hello from LIVA!");

            expect(mockSafeFetch).toHaveBeenCalledTimes(1);
            const [url, opts] = mockSafeFetch.mock.calls[0];
            expect(url).toContain("/sendMessage");
            const body = JSON.parse(opts.body);
            expect(body.chat_id).toBe("111222");
            expect(body.text).toBe("Hello from LIVA!");
        });

        it("should truncate text to 4096 chars", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ ok: true }),
            });

            const longText = "A".repeat(5000);
            await bridge.sendText("111222", longText);

            const body = JSON.parse(mockSafeFetch.mock.calls[0][1].body);
            expect(body.text.length).toBeLessThanOrEqual(4096);
        });

        it("should throw if API call fails", async () => {
            mockSafeFetch.mockRejectedValueOnce(new Error("Network Error"));
            await expect(bridge.sendText("111222", "test")).rejects.toThrow("Network Error");
        });
    });

    describe("sendApprovalCard", () => {
        it("should send message with inline keyboard buttons", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ ok: true }),
            });

            await bridge.sendApprovalCard("111222", "Test Title", "Test Body", "approval-123");

            const body = JSON.parse(mockSafeFetch.mock.calls[0][1].body);
            expect(body.text).toContain("Test Title");
            expect(body.text).toContain("Test Body");

            const keyboard = JSON.parse(body.reply_markup);
            expect(keyboard.inline_keyboard).toHaveLength(1);
            expect(keyboard.inline_keyboard[0]).toHaveLength(2);
            expect(keyboard.inline_keyboard[0][0].callback_data).toBe("approve:approval-123");
            expect(keyboard.inline_keyboard[0][1].callback_data).toBe("reject:approval-123");
        });
    });

    describe("editMessage", () => {
        it("should edit existing message", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ ok: true }),
            });

            await bridge.editMessage("111222", 42, "Updated text");

            expect(mockSafeFetch).toHaveBeenCalledTimes(1);
            const [url, opts] = mockSafeFetch.mock.calls[0];
            expect(url).toContain("/editMessageText");
            const body = JSON.parse(opts.body);
            expect(body.chat_id).toBe("111222");
            expect(body.message_id).toBe(42);
            expect(body.text).toBe("Updated text");
        });
    });

    describe("sendScreenshot", () => {
        it("should send photo via multipart form data", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ ok: true }),
            });

            const buffer = Buffer.from("fake-png-data");
            await bridge.sendScreenshot("111222", buffer);

            expect(mockSafeFetch).toHaveBeenCalledTimes(1);
            const [url, opts] = mockSafeFetch.mock.calls[0];
            expect(url).toContain("/sendPhoto");
            expect(opts.headers["Content-Type"]).toContain("multipart/form-data");
            
            // Should contain the buffer data in body
            const body = opts.body as Buffer;
            expect(body.includes(buffer)).toBe(true);
            expect(body.toString()).toContain('name="chat_id"');
            expect(body.toString()).toContain('111222');
        });
    });

    describe("Polling & Message Handling", () => {
        it("should not start polling if no token", async () => {
            delete process.env.TELEGRAM_BOT_TOKEN;
            const b = new TelegramBridge();
            await b.startPolling();
            expect(mockSafeFetch).not.toHaveBeenCalled();
            b.stop();
        });

        it("should start polling loop and fetch updates", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ 
                    ok: true, 
                    result: [
                        {
                            update_id: 1,
                            message: {
                                message_id: 100,
                                from: { id: 111222, first_name: "Dương" },
                                chat: { id: 111222, type: "private" },
                                text: "Mở terminal",
                                date: 1600000000,
                            }
                        }
                    ] 
                }),
            });

            // Prevent infinite loop by stopping it immediately after first poll completes
            const messageHandler = vi.fn();
            bridge.on("message", messageHandler);

            // Start polling (does not await the internal loop)
            bridge.startPolling();
            
            // Let the microtask queue process the first poll
            await Promise.resolve(); // wait for fetch
            await Promise.resolve(); // wait for handleUpdate

            expect(mockSafeFetch).toHaveBeenCalled();
            expect(messageHandler).toHaveBeenCalledTimes(1);
            
            const msg = messageHandler.mock.calls[0][0];
            expect(msg.channel).toBe("telegram");
            expect(msg.text).toBe("Mở terminal");
            expect(msg.senderId).toBe("111222");
        });

        it("should emit callback_query event when inline button clicked", async () => {
            mockSafeFetch
                .mockResolvedValueOnce({
                    json: () => Promise.resolve({ 
                        ok: true, 
                        result: [
                            {
                                update_id: 2,
                                callback_query: {
                                    id: "cb-123",
                                    from: { id: 111222, first_name: "Dương" },
                                    data: "approve:task1",
                                    message: { chat: { id: 111222 }, message_id: 42 }
                                }
                            }
                        ] 
                    }),
                })
                .mockResolvedValueOnce({ // answerCallbackQuery
                    json: () => Promise.resolve({ ok: true })
                });

            const cbHandler = vi.fn();
            bridge.on("callback_query", cbHandler);

            bridge.startPolling();
            
            await Promise.resolve(); 
            await Promise.resolve(); 

            expect(cbHandler).toHaveBeenCalledTimes(1);
            const cbData = cbHandler.mock.calls[0][0];
            expect(cbData.queryId).toBe("cb-123");
            expect(cbData.data).toBe("approve:task1");
            expect(cbData.messageId).toBe(42);
        });

        it("should handle image attachments", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ 
                    ok: true, 
                    result: [
                        {
                            update_id: 3,
                            message: {
                                message_id: 101,
                                from: { id: 111222, first_name: "Dương" },
                                chat: { id: 111222, type: "private" },
                                text: "Xem ảnh",
                                photo: [
                                    { file_id: "small", width: 100, height: 100 },
                                    { file_id: "large-file-id", width: 800, height: 800 }
                                ],
                                date: 1600000000,
                            }
                        }
                    ] 
                }),
            });

            const messageHandler = vi.fn();
            bridge.on("message", messageHandler);

            bridge.startPolling();
            await Promise.resolve(); 
            await Promise.resolve();

            const msg = messageHandler.mock.calls[0][0];
            expect(msg.mediaType).toBe("image");
            expect(msg.mediaUrl).toBe("large-file-id"); // Should pick the last (largest) photo
        });

        it("should block unauthorized sender IDs in text messages", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ 
                    ok: true, 
                    result: [
                        {
                            update_id: 4,
                            message: {
                                message_id: 102,
                                from: { id: 999999, first_name: "Hacker" }, // Not in whitelist
                                chat: { id: 999999, type: "private" },
                                text: "Hack",
                                date: 1600000000,
                            }
                        }
                    ] 
                }),
            });

            const messageHandler = vi.fn();
            bridge.on("message", messageHandler);

            bridge.startPolling();
            await Promise.resolve(); 
            await Promise.resolve();

            expect(messageHandler).not.toHaveBeenCalled();
        });

        it("should block unauthorized sender IDs in callback queries", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ 
                    ok: true, 
                    result: [
                        {
                            update_id: 5,
                            callback_query: {
                                id: "cb-123",
                                from: { id: 999999, first_name: "Hacker" }, // Not in whitelist
                                data: "approve:task1",
                            }
                        }
                    ] 
                }),
            });

            const cbHandler = vi.fn();
            bridge.on("callback_query", cbHandler);

            bridge.startPolling();
            await Promise.resolve(); 
            await Promise.resolve();

            expect(cbHandler).not.toHaveBeenCalled();
        });

        it("should apply exponential backoff on fetch errors", async () => {
            const abortError = new Error("Abort");
            abortError.name = "AbortError"; // Simulated timeout

            // 1st call fails
            mockSafeFetch.mockRejectedValueOnce(abortError);
            // 2nd call succeeds
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ ok: true, result: [] }),
            });

            bridge.startPolling();
            await Promise.resolve(); // Let 1st poll fail
            
            // Should schedule retry with backoff
            expect(mockSafeFetch).toHaveBeenCalledTimes(1);
            
            // Advance timer to trigger 2nd poll
            vi.advanceTimersByTime(2500); 
            await Promise.resolve();
            
            expect(mockSafeFetch).toHaveBeenCalledTimes(2);
        });
    });

    describe("Lifecycle", () => {
        it("should stop cleanly and clear timers", () => {
            bridge.startPolling();
            bridge.stop();
            // No error should be thrown
        });

        it("should be safe to stop multiple times", () => {
            bridge.stop();
            bridge.stop();
            // No error
        });
    
            });
});