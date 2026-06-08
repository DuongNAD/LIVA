import { EventEmitter } from "node:events";
import { describe, expect, expectTypeOf, it, vi, beforeEach, afterEach } from "vitest";
import { TypedEventBus } from "../../../src/core/events/TypedEventBus";

interface TestEvents {
    "ai:stream_chunk": {
        id: string;
        text: string;
        index?: number;
    };
    "kernel:tick": void;
}

describe("TypedEventBus", () => {
    it("emits typed payloads to registered listeners", () => {
        const bus = new TypedEventBus<TestEvents>();
        const received: string[] = [];

        bus.on("ai:stream_chunk", (payload) => {
            expectTypeOf(payload).toEqualTypeOf<TestEvents["ai:stream_chunk"]>();
            received.push(`${payload.id}:${payload.text}:${payload.index ?? 0}`);
        });

        const emitted = bus.emit("ai:stream_chunk", {
            id: "turn-1",
            text: "hello",
            index: 1,
        });

        expect(emitted).toBe(true);
        expect(received).toEqual(["turn-1:hello:1"]);
    });

    it("supports payloadless events", () => {
        const bus = new TypedEventBus<TestEvents>();
        const handler = vi.fn();

        bus.on("kernel:tick", handler);

        expect(bus.emit("kernel:tick")).toBe(true);
        expect(handler).toHaveBeenCalledOnce();
    });

    it("removes one listener with off", () => {
        const bus = new TypedEventBus<TestEvents>();
        const handler = vi.fn();

        bus.on("ai:stream_chunk", handler);
        expect(bus.listenerCount("ai:stream_chunk")).toBe(1);

        bus.off("ai:stream_chunk", handler);
        expect(bus.listenerCount("ai:stream_chunk")).toBe(0);

        bus.emit("ai:stream_chunk", { id: "turn-2", text: "ignored" });
        expect(handler).not.toHaveBeenCalled();
    });

    it("does not leak listeners through repeated subscribe and unsubscribe cycles", () => {
        const bus = new TypedEventBus<TestEvents>();
        const handler = vi.fn();

        for (let index = 0; index < 100; index += 1) {
            const unsubscribe = bus.on("ai:stream_chunk", handler);
            unsubscribe();
        }

        expect(bus.listenerCount("ai:stream_chunk")).toBe(0);
    });

    it("disposes only listeners owned by the bus", () => {
        const emitter = new EventEmitter();
        const outsideListener = vi.fn();
        const busListener = vi.fn();

        emitter.on("ai:stream_chunk", outsideListener);

        const bus = new TypedEventBus<TestEvents>(emitter);
        bus.on("ai:stream_chunk", busListener);
        bus.on("kernel:tick", vi.fn());

        expect(bus.listenerCount("ai:stream_chunk")).toBe(2);
        expect(bus.listenerCount("kernel:tick")).toBe(1);

        bus.dispose();

        expect(bus.isDisposed).toBe(true);
        expect(emitter.listenerCount("ai:stream_chunk")).toBe(1);
        expect(emitter.listenerCount("kernel:tick")).toBe(0);

        emitter.emit("ai:stream_chunk", { id: "turn-3", text: "external" });
        expect(outsideListener).toHaveBeenCalledOnce();
        expect(busListener).not.toHaveBeenCalled();
    });

    it("rejects new listeners and emits after disposal", () => {
        const bus = new TypedEventBus<TestEvents>();
        bus.dispose();

        expect(() => bus.on("kernel:tick", vi.fn())).toThrow("TypedEventBus has been disposed");
        expect(() => bus.emit("kernel:tick")).toThrow("TypedEventBus has been disposed");
    });

    it("keeps compile-time event contracts strict", () => {
        const bus = new TypedEventBus<TestEvents>();

        bus.on("ai:stream_chunk", (payload) => {
            expectTypeOf(payload).toEqualTypeOf<TestEvents["ai:stream_chunk"]>();
        });

        if (false) {
            // @ts-expect-error unknown event names are rejected
            bus.emit("ai:missing", { id: "turn-4", text: "nope" });
            // @ts-expect-error required payload fields are enforced
            bus.emit("ai:stream_chunk", { id: "turn-4" });
            // @ts-expect-error payloadless events reject payload objects
            bus.emit("kernel:tick", { now: Date.now() });
            // @ts-expect-error handlers receive the mapped payload type
            bus.on("ai:stream_chunk", (_payload: number) => undefined);
        }

        expect(bus.listenerCount("ai:stream_chunk")).toBe(1);
        bus.dispose();
    });

    describe("Deduplication & DLQ", () => {
        beforeEach(() => {
            vi.useFakeTimers();
        });

        afterEach(() => {
            vi.useRealTimers();
        });

        it("should deduplicate identical events with payload within CACHE_TTL_MS", () => {
            const bus = new TypedEventBus<TestEvents>();
            const handler = vi.fn();
            bus.on("ai:stream_chunk", handler);

            // First emit should succeed
            expect(bus.emit("ai:stream_chunk", { id: "1", text: "hello" })).toBe(true);
            
            // Second identical emit should be dropped (returns false)
            expect(bus.emit("ai:stream_chunk", { id: "1", text: "hello" })).toBe(false);

            expect(handler).toHaveBeenCalledOnce();

            // Advance timers past TTL (2500ms) - let two cycles run (6000ms)
            vi.advanceTimersByTime(6000);

            // Third emit with same payload should now succeed
            expect(bus.emit("ai:stream_chunk", { id: "1", text: "hello" })).toBe(true);
            expect(handler).toHaveBeenCalledTimes(2);
            bus.dispose();
        });

        it("should NOT deduplicate events with different payloads within TTL", () => {
            const bus = new TypedEventBus<TestEvents>();
            const handler = vi.fn();
            bus.on("ai:stream_chunk", handler);

            expect(bus.emit("ai:stream_chunk", { id: "1", text: "hello" })).toBe(true);
            expect(bus.emit("ai:stream_chunk", { id: "1", text: "world" })).toBe(true); // payload changes
            expect(handler).toHaveBeenCalledTimes(2);
            bus.dispose();
        });

        it("should route synchronous and asynchronous listener errors to DLQ without throwing in emit", async () => {
            const bus = new TypedEventBus<TestEvents>();
            
            bus.on("kernel:tick", () => {
                throw new Error("Sync Error");
            });

            bus.on("kernel:tick", async () => {
                throw new Error("Async Error");
            });

            expect(bus.emit("kernel:tick")).toBe(true);

            // Wait for microtasks (listeners are async wrappers)
            await vi.advanceTimersByTimeAsync(100);

            const letters = bus.getDeadLetters();
            expect(letters.length).toBe(2);
            expect(letters[0].topic).toBe("kernel:tick");
            expect(letters[0].error).toBe("Sync Error");
            expect(letters[1].error).toBe("Async Error");

            // Test clear
            bus.clearDeadLetters();
            expect(bus.getDeadLetters().length).toBe(0);
            bus.dispose();
        });
    });
});
