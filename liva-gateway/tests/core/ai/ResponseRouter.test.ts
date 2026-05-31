import { describe, expect, it, vi } from "vitest";
import { ResponseRouter } from "../../../src/core/ai/ResponseRouter";
import type { EventCatalog } from "../../../src/core/events/EventCatalog";
import { TypedEventBus } from "../../../src/core/events/TypedEventBus";
import { notifyZalo } from "../../../src/utils/ZaloNotifier";

vi.mock("../../../src/memory/SensoryManager", () => ({
    SensoryManager: {
        getInstance: vi.fn().mockReturnValue({
            flush: vi.fn(),
        }),
    },
}));

vi.mock("../../../src/utils/ZaloNotifier", () => ({
    notifyZalo: vi.fn().mockResolvedValue(undefined),
}));

describe("ResponseRouter", () => {
    it("forwards stream events and disconnects callbacks without leaking listeners", () => {
        const eventBus = new TypedEventBus<EventCatalog>();
        const router = new ResponseRouter({
            eventBus,
            registry: { executeSkill: vi.fn() },
        });
        const onStreamStart = vi.fn();
        const onStreamChunk = vi.fn();

        const disconnect = router.connectStreamCallbacks({ onStreamStart, onStreamChunk });

        eventBus.emit("ai:stream_start", { id: "stream-1" });
        eventBus.emit("ai:stream_chunk", { id: "stream-1", text: "hello" });
        expect(onStreamStart).toHaveBeenCalledOnce();
        expect(onStreamChunk).toHaveBeenCalledWith("hello");

        disconnect();
        expect(eventBus.listenerCount("ai:stream_start")).toBe(0);
        expect(eventBus.listenerCount("ai:stream_chunk")).toBe(0);

        eventBus.emit("ai:stream_chunk", { id: "stream-1", text: "ignored" });
        expect(onStreamChunk).toHaveBeenCalledTimes(1);
    });

    it("classifies queueable Zalo AI errors without notifying immediately", async () => {
        const router = new ResponseRouter({
            eventBus: new TypedEventBus<EventCatalog>(),
            registry: { executeSkill: vi.fn() },
        });

        const result = await router.routeError(
            "[Tin nhắn từ Zalo điện thoại] hello",
            new Error("fetch failed"),
            {},
        );

        expect(result.action).toBe("queue_zalo");
        expect(notifyZalo).not.toHaveBeenCalled();
    });

    it("routes non-Zalo errors to spoken response", async () => {
        const router = new ResponseRouter({
            eventBus: new TypedEventBus<EventCatalog>(),
            registry: { executeSkill: vi.fn() },
        });
        const onSpokenResponse = vi.fn();

        const result = await router.routeError("hello", new Error("boom"), { onSpokenResponse });

        expect(result.action).toBe("spoken_error");
        expect(onSpokenResponse).toHaveBeenCalledWith("Vang Native AI: boom");
    });
});
