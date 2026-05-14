import { describe, it, expect } from "vitest";
import { TraceContext } from "../../src/utils/TraceContext";

describe("TraceContext", () => {
    describe("run", () => {
        it("should provide a trace ID within the run context", () => {
            let capturedId = "";
            TraceContext.run(() => {
                capturedId = TraceContext.getTraceId();
            });
            expect(capturedId).not.toBe("no-trace");
            expect(capturedId.length).toBe(8); // UUID substring(0,8)
        });

        it("should return 'no-trace' outside a run context", () => {
            expect(TraceContext.getTraceId()).toBe("no-trace");
        });

        it("should allow override trace ID", () => {
            TraceContext.run(() => {
                expect(TraceContext.getTraceId()).toBe("custom01");
            }, "custom01");
        });

        it("should return the function's return value", () => {
            const result = TraceContext.run(() => 42);
            expect(result).toBe(42);
        });
    });

    describe("runWithContext", () => {
        it("should store userId and channel in the trace store", () => {
            TraceContext.runWithContext(() => {
                const store = TraceContext.getStore();
                expect(store?.userId).toBe("user123");
                expect(store?.channel).toBe("zalo");
                expect(store?.traceId).toBeTruthy();
            }, { userId: "user123", channel: "zalo" });
        });
    });

    describe("pinoMixin", () => {
        it("should return empty object outside context", () => {
            expect(TraceContext.pinoMixin()).toEqual({});
        });

        it("should return traceId inside context", () => {
            TraceContext.run(() => {
                const mixin = TraceContext.pinoMixin();
                expect(mixin).toHaveProperty("traceId");
                expect(mixin.traceId.length).toBe(8);
            });
        });

        it("should include userId and channel when set", () => {
            TraceContext.runWithContext(() => {
                const mixin = TraceContext.pinoMixin();
                expect(mixin.traceId).toBeTruthy();
                expect(mixin.userId).toBe("u1");
                expect(mixin.channel).toBe("telegram");
            }, { userId: "u1", channel: "telegram" });
        });
    });

    describe("getStore", () => {
        it("should return undefined outside context", () => {
            expect(TraceContext.getStore()).toBeUndefined();
        });
    });
});
