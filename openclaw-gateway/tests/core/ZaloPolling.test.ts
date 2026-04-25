/**
 * ZaloPolling.test.ts — Inbound Message Listener Tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
}));

import { ZaloPolling } from "../../src/core/ZaloPolling";
import { safeFetch } from "../../src/utils/HttpClient";

describe("ZaloPolling", () => {
    let originalEnv: string | undefined;

    beforeEach(() => {
        vi.useFakeTimers();
        originalEnv = process.env.ZALO_OA_ACCESS_TOKEN;
    });

    afterEach(() => {
        vi.useRealTimers();
        process.env.ZALO_OA_ACCESS_TOKEN = originalEnv;
    });

    it("should NOT start polling without valid token", () => {
        process.env.ZALO_OA_ACCESS_TOKEN = "";
        const poller = new ZaloPolling();
        expect((poller as any).isPolling).toBe(false);
    });

    it("should NOT start polling with token missing colon", () => {
        process.env.ZALO_OA_ACCESS_TOKEN = "tokenWithoutColon";
        const poller = new ZaloPolling();
        expect((poller as any).isPolling).toBe(false);
    });

    it("should start polling with valid token containing colon", async () => {
        (safeFetch as any).mockResolvedValue({ json: () => Promise.resolve({ ok: false }) });
        process.env.ZALO_OA_ACCESS_TOKEN = "valid:token";
        const poller = new ZaloPolling();
        await (poller as any)._pollingPromise;
        expect((poller as any).isPolling).toBe(true);
        poller.stop();
    });

    it("should emit zalo_incoming on valid message", async () => {
        const mockResponse = {
            json: () => Promise.resolve({
                ok: true,
                result: [{ update_id: 1, message: { text: "Xin chào LIVA" } }],
            }),
        };
        (safeFetch as any).mockResolvedValue(mockResponse);

        process.env.ZALO_OA_ACCESS_TOKEN = "test:token";
        const poller = new ZaloPolling();
        const incomingSpy = vi.fn();
        poller.on("zalo_incoming", incomingSpy);

        // Let the initial poll() run
        await vi.advanceTimersByTimeAsync(100);

        expect(incomingSpy).toHaveBeenCalledWith(expect.stringContaining("Xin chào LIVA"));
        poller.stop();
    });

    it("should update offset after processing messages", async () => {
        (safeFetch as any).mockResolvedValue({
            json: () => Promise.resolve({
                ok: true,
                result: [{ update_id: 42, message: { text: "test" } }],
            }),
        });

        process.env.ZALO_OA_ACCESS_TOKEN = "test:token";
        const poller = new ZaloPolling();
        await vi.advanceTimersByTimeAsync(100);

        expect((poller as any).currentOffset).toBe(43);
        poller.stop();
    });

    it("should stop polling and clear timer on stop()", async () => {
        (safeFetch as any).mockResolvedValue({
            json: () => Promise.resolve({ ok: false }),
        });

        process.env.ZALO_OA_ACCESS_TOKEN = "test:token";
        const poller = new ZaloPolling();
        await vi.advanceTimersByTimeAsync(100);

        poller.stop();
        expect((poller as any).isPolling).toBe(false);
        expect((poller as any).pollTimerRef).toBeNull();
    });

    it("should handle fetch errors gracefully", async () => {
        (safeFetch as any).mockRejectedValue(new Error("Network down"));

        process.env.ZALO_OA_ACCESS_TOKEN = "test:token";
        const poller = new ZaloPolling();

        // Should not crash
        await vi.advanceTimersByTimeAsync(100);
        poller.stop();
    });

    it("should ignore AbortError silently", async () => {
        const abortErr = new Error("Aborted");
        abortErr.name = "AbortError";
        (safeFetch as any).mockRejectedValue(abortErr);

        process.env.ZALO_OA_ACCESS_TOKEN = "test:token";
        const poller = new ZaloPolling();
        await vi.advanceTimersByTimeAsync(100);
        poller.stop();
    });
});
