/**
 * MemoryTelemetry.test.ts — v4.0 Observability Tests
 */
import { describe, it, expect, beforeEach } from "vitest";
import { memTelemetry } from "../../src/utils/MemoryTelemetry";

describe("MemoryTelemetry", () => {
    beforeEach(() => {
        memTelemetry.reset();
    });

    describe("L0 Cache Metrics", () => {
        it("should track cache hits and misses", () => {
            memTelemetry.recordCacheHit();
            memTelemetry.recordCacheHit();
            memTelemetry.recordCacheMiss();

            const snap = memTelemetry.getSnapshot();
            expect(snap.cacheHits).toBe(2);
            expect(snap.cacheMisses).toBe(1);
        });

        it("should calculate cache hit ratio", () => {
            memTelemetry.recordCacheHit();
            memTelemetry.recordCacheHit();
            memTelemetry.recordCacheHit();
            memTelemetry.recordCacheMiss();

            const snap = memTelemetry.getSnapshot();
            expect(snap.cacheHitRatio).toBe(0.75);
        });

        it("should return 0 ratio when no requests", () => {
            expect(memTelemetry.getSnapshot().cacheHitRatio).toBe(0);
        });
    });

    describe("L2 Search Latency", () => {
        it("should track average latency", () => {
            memTelemetry.recordL2Latency(10);
            memTelemetry.recordL2Latency(20);
            memTelemetry.recordL2Latency(30);

            const snap = memTelemetry.getSnapshot();
            expect(snap.l2AvgLatencyMs).toBe(20);
        });

        it("should track max latency", () => {
            memTelemetry.recordL2Latency(10);
            memTelemetry.recordL2Latency(50);
            memTelemetry.recordL2Latency(30);

            expect(memTelemetry.getSnapshot().l2MaxLatencyMs).toBe(50);
        });

        it("should keep only last 100 samples for moving average", () => {
            for (let i = 0; i < 110; i++) {
                memTelemetry.recordL2Latency(10);
            }
            // Internally should have trimmed to 100
            const snap = memTelemetry.getSnapshot();
            expect(snap.l2AvgLatencyMs).toBe(10);
        });

        it("should track circuit breaker trips", () => {
            memTelemetry.recordCircuitBreakerTrip();
            memTelemetry.recordCircuitBreakerTrip();

            expect(memTelemetry.getSnapshot().l2CircuitBreakerTrips).toBe(2);
        });
    });

    describe("Token FinOps", () => {
        it("should accumulate tokens saved", () => {
            memTelemetry.addTokensSaved(1000);
            memTelemetry.addTokensSaved(500);

            expect(memTelemetry.getSnapshot().tokensSaved).toBe(1500);
        });
    });

    describe("Counters", () => {
        it("should track GDPR purge count", () => {
            memTelemetry.recordPurge();
            expect(memTelemetry.getSnapshot().purgeCount).toBe(1);
        });

        it("should track PKE extractions", () => {
            memTelemetry.recordPKEExtraction(3);
            memTelemetry.recordPKEExtraction();
            expect(memTelemetry.getSnapshot().pkeExtractions).toBe(4);
        });

        it("should track fact reconciliations", () => {
            memTelemetry.recordFactReconciliation();
            memTelemetry.recordFactReconciliation();
            expect(memTelemetry.getSnapshot().factReconciliations).toBe(2);
        });

        it("should set facts count", () => {
            memTelemetry.setFactsCount(42);
            expect(memTelemetry.getSnapshot().factsCount).toBe(42);
        });
    });

    describe("Reset", () => {
        it("should reset all counters to zero", () => {
            memTelemetry.recordCacheHit();
            memTelemetry.recordL2Latency(100);
            memTelemetry.addTokensSaved(5000);
            memTelemetry.recordPurge();

            memTelemetry.reset();

            const snap = memTelemetry.getSnapshot();
            expect(snap.cacheHits).toBe(0);
            expect(snap.l2AvgLatencyMs).toBe(0);
            expect(snap.tokensSaved).toBe(0);
            expect(snap.purgeCount).toBe(0);
        });
    });
});
