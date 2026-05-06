/**
 * MemoryTelemetry — Observability Counters for LIVA Memory System
 * ================================================================
 * Tracks key performance metrics for the 4-tier memory hierarchy.
 * All counters are in-memory (reset on restart). Future: export
 * to Prometheus/StatsD via push gateway.
 *
 * Usage:
 *   import { memTelemetry } from "./utils/MemoryTelemetry";
 *   memTelemetry.recordCacheHit();
 *   memTelemetry.recordL2Latency(42);
 *   memTelemetry.addTokensSaved(1500);
 *   logger.info(memTelemetry.getSnapshot());
 *
 * [v4.0] Phase 1.5 Observability
 */

import { logger } from "./logger";

export interface TelemetrySnapshot {
    /** L0 cache hit count */
    cacheHits: number;
    /** L0 cache miss count */
    cacheMisses: number;
    /** L0 cache hit ratio (0.0 - 1.0) */
    cacheHitRatio: number;
    /** L2 vector search latency (ms) — moving average */
    l2AvgLatencyMs: number;
    /** L2 vector search latency (ms) — max observed */
    l2MaxLatencyMs: number;
    /** L2 circuit breaker trip count */
    l2CircuitBreakerTrips: number;
    /** Estimated LLM tokens saved by caching/deduplication */
    tokensSaved: number;
    /** Total facts stored */
    factsCount: number;
    /** GDPR purge count */
    purgeCount: number;
    /** PKE extractions performed */
    pkeExtractions: number;
    /** Fact reconciliations (soft-deprecations) */
    factReconciliations: number;
}

class MemoryTelemetry {
    private cacheHits = 0;
    private cacheMisses = 0;
    private l2Latencies: number[] = [];
    private l2MaxLatency = 0;
    private l2Trips = 0;
    private _tokensSaved = 0;
    private _purgeCount = 0;
    private _pkeExtractions = 0;
    private _factReconciliations = 0;
    private _factsCount = 0;

    // --- L0 Cache ---

    public recordCacheHit(): void {
        this.cacheHits++;
    }

    public recordCacheMiss(): void {
        this.cacheMisses++;
    }

    // --- L2 Vector Search ---

    public recordL2Latency(ms: number): void {
        this.l2Latencies.push(ms);
        if (ms > this.l2MaxLatency) this.l2MaxLatency = ms;
        // Keep only last 100 samples for moving average
        if (this.l2Latencies.length > 100) {
            this.l2Latencies.shift();
        }
    }

    public recordCircuitBreakerTrip(): void {
        this.l2Trips++;
        logger.warn(`[Telemetry] L2 Circuit Breaker tripped (total: ${this.l2Trips})`);
    }

    // --- Token FinOps ---

    public addTokensSaved(count: number): void {
        this._tokensSaved += count;
    }

    // --- Counters ---

    public recordPurge(): void {
        this._purgeCount++;
    }

    public recordPKEExtraction(count: number = 1): void {
        this._pkeExtractions += count;
    }

    public recordFactReconciliation(): void {
        this._factReconciliations++;
    }

    public setFactsCount(count: number): void {
        this._factsCount = count;
    }

    // --- Snapshot ---

    public getSnapshot(): TelemetrySnapshot {
        const totalRequests = this.cacheHits + this.cacheMisses;
        const avgLatency = this.l2Latencies.length > 0
            ? this.l2Latencies.reduce((a, b) => a + b, 0) / this.l2Latencies.length
            : 0;

        return {
            cacheHits: this.cacheHits,
            cacheMisses: this.cacheMisses,
            cacheHitRatio: totalRequests > 0 ? this.cacheHits / totalRequests : 0,
            l2AvgLatencyMs: Math.round(avgLatency * 100) / 100,
            l2MaxLatencyMs: this.l2MaxLatency,
            l2CircuitBreakerTrips: this.l2Trips,
            tokensSaved: this._tokensSaved,
            factsCount: this._factsCount,
            purgeCount: this._purgeCount,
            pkeExtractions: this._pkeExtractions,
            factReconciliations: this._factReconciliations,
        };
    }

    /** Reset all counters (for testing) */
    public reset(): void {
        this.cacheHits = 0;
        this.cacheMisses = 0;
        this.l2Latencies = [];
        this.l2MaxLatency = 0;
        this.l2Trips = 0;
        this._tokensSaved = 0;
        this._purgeCount = 0;
        this._pkeExtractions = 0;
        this._factReconciliations = 0;
        this._factsCount = 0;
    }
}

/** Singleton telemetry instance */
export const memTelemetry = new MemoryTelemetry();
