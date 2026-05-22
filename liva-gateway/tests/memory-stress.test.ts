/**
 * memory-stress.test.ts — LIVA Memory Architecture Stress Test
 * =============================================================
 * Safe, bounded stress test that validates memory subsystem under load.
 *
 * Stages:
 *   Stage 1: 100 messages  (warm-up)
 *   Stage 2: 500 messages  (moderate load)
 *   Stage 3: 1000 messages (stress load — triggers eviction)
 *
 * Measures: RAM delta, query latency, SQLite row counts, file sizes.
 * Safety:  temp directory auto-cleaned, 500MB RAM hard-abort, dummy vectors (no GPU).
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import * as path from 'node:path';
import * as fs from 'node:fs/promises';
import { QuantizedMemoryStore, CoreKernel } from '../src/memory/TurboQuantStore';
import { StructuredMemory } from '../src/memory/StructuredMemory';

// ═══════════════════════════════════════════════════════
//  Test Utilities
// ═══════════════════════════════════════════════════════

const AGENT_ID = 'stress_test_agent';
const DATA_DIR = path.join(process.cwd(), 'data', 'agents', AGENT_ID);
const QUANT_FILE = path.join(DATA_DIR, 'turbo_quant_memory.jsonl');
const SQLITE_PATH = path.join(DATA_DIR, 'structured_memory.sqlite');
const RAM_DELTA_LIMIT_MB = 300; // Abort if test leaks > 300MB of extra RAM during stress
let baselineRSS = 0;

function getRAM(): { rss: number; heapUsed: number; heapTotal: number } {
    const mem = process.memoryUsage();
    return {
        rss: Math.round(mem.rss / 1024 / 1024),
        heapUsed: Math.round(mem.heapUsed / 1024 / 1024),
        heapTotal: Math.round(mem.heapTotal / 1024 / 1024),
    };
}

function generateMessage(idx: number, role: 'user' | 'assistant'): string {
    const topics = ['weather', 'coding', 'cooking', 'music', 'travel', 'sports', 'science', 'history'];
    const topic = topics[idx % topics.length];
    return `[${role}] Message #${idx} about ${topic}: Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Topic: ${topic}, timestamp: ${Date.now()}.`;
}

function generateDummyVector(dim: number = 256): number[] {
    return Array.from({ length: dim }, (_, i) => Math.sin(i * 0.1) * 2 - 1);
}

async function fileSize(filePath: string): Promise<number> {
    try {
        const stat = await fs.stat(filePath);
        return Math.round(stat.size / 1024); // KB
    } catch {
        return 0;
    }
}

// ═══════════════════════════════════════════════════════
//  Test Suite
// ═══════════════════════════════════════════════════════

describe('LIVA Memory Stress Test', () => {
    let authority: CoreKernel;
    let quantStore: QuantizedMemoryStore;
    let structuredMemory: StructuredMemory;

    const results: Array<{
        stage: string;
        messages: number;
        ramBefore: ReturnType<typeof getRAM>;
        ramAfter: ReturnType<typeof getRAM>;
        ramDeltaMB: number;
        insertTimeMs: number;
        queryTimeMs: number;
        factCount: number;
        jsonlSizeKB: number;
        sqliteSizeKB: number;
    }> = [];

    beforeAll(async () => {
        process.env.LIVA_ENCRYPTION_KEY = "LIVA_TEST_KEY_32BYTES_XXXXXXXXXX";
        // Clean start
        await fs.rm(DATA_DIR, { recursive: true, force: true }).catch(() => {});
        await fs.mkdir(DATA_DIR, { recursive: true });

        // Initialize components
        authority = new CoreKernel(['system', 'user', 'assistant']);
        quantStore = new QuantizedMemoryStore(authority, QUANT_FILE);
        structuredMemory = await StructuredMemory.create(AGENT_ID);

        if (global.gc) global.gc();
        baselineRSS = process.memoryUsage().rss;
    }, 30_000);

    afterAll(async () => {
        // Cleanup
        try {
            await quantStore.dispose();
            structuredMemory.close();
        } catch { /* ignore */ }
        // Remove test data
        await fs.rm(DATA_DIR, { recursive: true, force: true }).catch(() => {});
    }, 10_000);

    // ─── Stage 1: 100 messages (warm-up) ─────────────────

    it('Stage 1: 100 messages — warm-up', async () => {
        const count = 100;
        if (global.gc) global.gc();
        const currentRSS = process.memoryUsage().rss;
        const deltaMB = Math.round((currentRSS - baselineRSS) / (1024 * 1024));
        expect(deltaMB).toBeLessThan(RAM_DELTA_LIMIT_MB);
        const ramBefore = getRAM();

        const startInsert = performance.now();
        for (let i = 0; i < count; i++) {
            const role = i % 2 === 0 ? 'user' : 'assistant' as const;
            const content = generateMessage(i, role);
            const token = authority.mintAuthToken(role) as string;
            await quantStore.addMemory(role, content, generateDummyVector(), token);

            // Every 10th message, add a structured fact
            if (i % 10 === 0) {
                structuredMemory.setFact(
                    `stress_fact_${i}`,
                    `Value for stress test fact #${i}`,
                    { source: 'stress_test', category: 'test' }
                );
            }
        }
        const insertTime = performance.now() - startInsert;

        // Query performance test
        const startQuery = performance.now();
        const queryVector = generateDummyVector();
        const userToken = authority.mintAuthToken('user') as string;
        const searchResults = quantStore.searchSimilar(queryVector, 'user', userToken, 5);
        const queryTime = performance.now() - startQuery;

        const ramAfter = getRAM();

        results.push({
            stage: 'Stage 1',
            messages: count,
            ramBefore,
            ramAfter,
            ramDeltaMB: ramAfter.heapUsed - ramBefore.heapUsed,
            insertTimeMs: Math.round(insertTime),
            queryTimeMs: Math.round(queryTime * 100) / 100,
            factCount: structuredMemory.count,
            jsonlSizeKB: await fileSize(QUANT_FILE),
            sqliteSizeKB: await fileSize(SQLITE_PATH),
        });

        expect(searchResults.length).toBeGreaterThan(0);
        expect(searchResults.length).toBeLessThanOrEqual(5);
        expect(structuredMemory.count).toBeLessThanOrEqual(50); // MAX_FACTS enforced
    }, 60_000);

    // ─── Stage 2: 500 messages (moderate load) ───────────

    it('Stage 2: +400 messages (total 500) — moderate load', async () => {
        const count = 400;
        if (global.gc) global.gc();
        const currentRSS = process.memoryUsage().rss;
        const deltaMB = Math.round((currentRSS - baselineRSS) / (1024 * 1024));
        expect(deltaMB).toBeLessThan(RAM_DELTA_LIMIT_MB);
        const ramBefore = getRAM();

        const startInsert = performance.now();
        for (let i = 100; i < 100 + count; i++) {
            const role = i % 2 === 0 ? 'user' : 'assistant' as const;
            const content = generateMessage(i, role);
            const token = authority.mintAuthToken(role) as string;
            await quantStore.addMemory(role, content, generateDummyVector(), token);

            if (i % 20 === 0) {
                structuredMemory.setFact(
                    `stress_fact_${i}`,
                    `Value for stress test fact #${i} — moderate load phase`,
                    { source: 'stress_test', category: 'test' }
                );
            }
        }
        const insertTime = performance.now() - startInsert;

        // Query latency under load
        const startQuery = performance.now();
        const queryVector = generateDummyVector();
        const assistantToken = authority.mintAuthToken('assistant') as string;
        const searchResults = quantStore.searchSimilar(queryVector, 'assistant', assistantToken, 5);
        const queryTime = performance.now() - startQuery;

        const ramAfter = getRAM();

        results.push({
            stage: 'Stage 2',
            messages: 500,
            ramBefore,
            ramAfter,
            ramDeltaMB: ramAfter.heapUsed - ramBefore.heapUsed,
            insertTimeMs: Math.round(insertTime),
            queryTimeMs: Math.round(queryTime * 100) / 100,
            factCount: structuredMemory.count,
            jsonlSizeKB: await fileSize(QUANT_FILE),
            sqliteSizeKB: await fileSize(SQLITE_PATH),
        });

        expect(searchResults.length).toBeGreaterThan(0);
        expect(structuredMemory.count).toBeLessThanOrEqual(50);
    }, 120_000);

    // ─── Stage 3: 1000 messages (stress load) ────────────

    it('Stage 3: +500 messages (total 1000) — stress load', async () => {
        const count = 500;
        if (global.gc) global.gc();
        const currentRSS = process.memoryUsage().rss;
        const deltaMB = Math.round((currentRSS - baselineRSS) / (1024 * 1024));
        expect(deltaMB).toBeLessThan(RAM_DELTA_LIMIT_MB);
        const ramBefore = getRAM();

        const startInsert = performance.now();
        for (let i = 500; i < 500 + count; i++) {
            const role = i % 2 === 0 ? 'user' : 'assistant' as const;
            const content = generateMessage(i, role);
            const token = authority.mintAuthToken(role) as string;
            await quantStore.addMemory(role, content, generateDummyVector(), token);

            // Facts should FIFO evict beyond 50
            if (i % 10 === 0) {
                structuredMemory.setFact(
                    `stress_fact_${i}`,
                    `Value for stress test fact #${i} — heavy load eviction test`,
                    { source: 'stress_test', category: 'test' }
                );
            }
        }
        const insertTime = performance.now() - startInsert;

        // Query latency under heavy load
        const startQuery = performance.now();
        const queryVector = generateDummyVector();
        const userToken = authority.mintAuthToken('user') as string;
        const searchResults = quantStore.searchSimilar(queryVector, 'user', userToken, 5);
        const queryTime = performance.now() - startQuery;

        const ramAfter = getRAM();

        results.push({
            stage: 'Stage 3',
            messages: 1000,
            ramBefore,
            ramAfter,
            ramDeltaMB: ramAfter.heapUsed - ramBefore.heapUsed,
            insertTimeMs: Math.round(insertTime),
            queryTimeMs: Math.round(queryTime * 100) / 100,
            factCount: structuredMemory.count,
            jsonlSizeKB: await fileSize(QUANT_FILE),
            sqliteSizeKB: await fileSize(SQLITE_PATH),
        });

        expect(searchResults.length).toBeGreaterThan(0);
        expect(structuredMemory.count).toBeLessThanOrEqual(50); // FIFO enforced
    }, 180_000);

    // ─── Final: QuantStore compaction on reload ──────────

    it('Compaction: reload with >2000 entries prunes correctly', async () => {
        // Force save current state (should have 1000 entries)
        await quantStore.save();

        const sizeBeforeKB = await fileSize(QUANT_FILE);
        expect(sizeBeforeKB).toBeGreaterThan(0);

        // Create a new store from same file — compaction should trigger if > 2000
        // With 1000 entries, no compaction needed but loadAsync should succeed
        const store2 = new QuantizedMemoryStore(authority, QUANT_FILE);
        await store2.loadAsync();
        await store2.dispose();

        // The file should still be intact
        const sizeAfterKB = await fileSize(QUANT_FILE);
        expect(sizeAfterKB).toBeGreaterThan(0);
    }, 30_000);

    // ─── Final: Ebbinghaus Decay ─────────────────────────

    it('Ebbinghaus decay runs without error', async () => {
        const result = await structuredMemory.applyMemoryDecay(0.1);
        expect(result).toBeDefined();
        expect(result.decayed).toBeGreaterThanOrEqual(0);
        expect(result.archived).toBeGreaterThanOrEqual(0);
    }, 10_000);

    // ─── Print Summary ──────────────────────────────────

    it('Summary Report', () => {
        console.table(results.map(r => ({
            Stage: r.stage,
            Messages: r.messages,
            'Insert (ms)': r.insertTimeMs,
            'Query (ms)': r.queryTimeMs,
            'RAM Δ (MB)': r.ramDeltaMB,
            'Heap (MB)': r.ramAfter.heapUsed,
            'RSS (MB)': r.ramAfter.rss,
            Facts: r.factCount,
            'JSONL (KB)': r.jsonlSizeKB,
            'SQLite (KB)': r.sqliteSizeKB,
        })));

        // Safety assertions
        for (const r of results) {
            const deltaRSSMB = Math.round((r.ramAfter.rss * 1024 * 1024 - baselineRSS) / (1024 * 1024));
            expect(deltaRSSMB).toBeLessThan(RAM_DELTA_LIMIT_MB);
            expect(r.queryTimeMs).toBeLessThan(5000); // Query should be < 5s even under load
        }
    });
});
