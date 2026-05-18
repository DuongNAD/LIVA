/**
 * RamCacheManager.test.ts — Sprint 4 Task 4.2 Tests
 * Tests bounded FIFO cache, hydration, warm-up injection, and GDPR purge
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { RamCacheManager } from "../../src/memory/RamCacheManager";
import type { ChatMessage } from "../../src/memory/RamCacheManager";

describe("RamCacheManager", () => {
    let cache: RamCacheManager;

    beforeEach(() => {
        vi.clearAllMocks();
        cache = new RamCacheManager();
    });

    describe("push and getAll", () => {
        it("should push a single message and retrieve it", () => {
            const msg: ChatMessage = { role: "user", content: "Hello", timestamp: Date.now() };
            cache.push(msg);

            const all = cache.getAll();
            expect(all.length).toBe(1);
            expect(all[0].content).toBe("Hello");
        });

        it("should push multiple messages in order", () => {
            cache.push({ role: "user", content: "A", timestamp: 1 });
            cache.push({ role: "assistant", content: "B", timestamp: 2 });
            cache.push({ role: "user", content: "C", timestamp: 3 });

            const all = cache.getAll();
            expect(all.length).toBe(3);
            expect(all[0].content).toBe("A");
            expect(all[2].content).toBe("C");
        });

        it("should report correct length", () => {
            expect(cache.length).toBe(0);
            cache.push({ role: "user", content: "X", timestamp: 1 });
            expect(cache.length).toBe(1);
        });
    });

    describe("bounded eviction (MAX_CACHE_SIZE=200)", () => {
        it("should evict old messages when cache exceeds 200", () => {
            // Fill to 210 messages
            for (let i = 0; i < 210; i++) {
                cache.push({ role: "user", content: `Msg ${i}`, timestamp: i });
            }

            // After 201st push, cache should have been sliced to 100, then 9 more added = 109
            // But the eviction happens on the push that exceeds 200, so:
            // After 201: cache = last 100 = [101..200]
            // Then pushes 202-210 → [101..209] = 109 messages
            expect(cache.length).toBeLessThanOrEqual(200);
            expect(cache.length).toBeGreaterThanOrEqual(100);
        });

        it("should log eviction message", async () => {
            const { logger } = await import("../../src/utils/logger");
            const infoSpy = vi.spyOn(logger, "info").mockImplementation(() => {});

            for (let i = 0; i < 201; i++) {
                cache.push({ role: "user", content: `Msg ${i}`, timestamp: i });
            }

            expect(infoSpy).toHaveBeenCalledWith(
                expect.stringContaining("Đã chặt bỏ 100 tin nhắn cũ khỏi RAM Cache")
            );
        });
    });

    describe("hydrate", () => {
        it("should parse valid JSONL lines", () => {
            const lines = [
                '{"role":"user","content":"Hi","timestamp":1000}',
                '{"role":"assistant","content":"Hello!","timestamp":2000}',
            ];
            cache.hydrate(lines);

            expect(cache.length).toBe(2);
            expect(cache.getAll()[0].content).toBe("Hi");
            expect(cache.getAll()[1].content).toBe("Hello!");
        });

        it("should reset to empty on invalid JSON", () => {
            const lines = ['{"valid":"json"}', "INVALID_LINE"];
            cache.hydrate(lines);

            // Should catch error and reset to []
            expect(cache.length).toBe(0);
        });

        it("should handle empty lines array", () => {
            cache.hydrate([]);
            expect(cache.length).toBe(0);
        });

        it("should assign Date.now() as default timestamp when missing", () => {
            const before = Date.now();
            cache.hydrate(['{"role":"user","content":"no ts"}']);
            const after = Date.now();

            const msg = cache.getAll()[0];
            expect(msg.timestamp).toBeGreaterThanOrEqual(before);
            expect(msg.timestamp).toBeLessThanOrEqual(after);
        });
    });

    describe("injectWarmup", () => {
        it("should inject a system message with warm-up content", () => {
            cache.injectWarmup("[PREVIOUS SESSION CONTEXT]\nUser: Hello");

            expect(cache.length).toBe(1);
            const msg = cache.getAll()[0];
            expect(msg.role).toBe("system");
            expect(msg.content).toContain("PREVIOUS SESSION CONTEXT");
        });
    });

    describe("purge (GDPR)", () => {
        it("should clear all messages", () => {
            cache.push({ role: "user", content: "A", timestamp: 1 });
            cache.push({ role: "assistant", content: "B", timestamp: 2 });
            expect(cache.length).toBe(2);

            cache.purge();
            expect(cache.length).toBe(0);
            expect(cache.getAll()).toEqual([]);
        });
    });
});
