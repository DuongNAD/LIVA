import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { StructuredMemory } from "../../src/memory/StructuredMemory";
import * as fs from "node:fs";
import * as path from "node:path";

let TEST_AGENT_ID = "__test_concurrency_agent__";
let TEST_BASE_DIR = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID);
let TEST_STORE_PATH = path.join(TEST_BASE_DIR, "concurrency_memory.sqlite");

describe("StructuredMemory Concurrency & Database Locking Stress Tests", () => {
  let memory: StructuredMemory;

  beforeEach(async () => {
    const randId = Math.random().toString(36).substring(7) + "_" + process.pid + "_" + Date.now();
    TEST_AGENT_ID = `__test_concurrency_${randId}__`;
    TEST_BASE_DIR = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID);
    TEST_STORE_PATH = path.join(TEST_BASE_DIR, "concurrency_memory.sqlite");

    try {
      if (fs.existsSync(TEST_STORE_PATH)) fs.unlinkSync(TEST_STORE_PATH);
      if (fs.existsSync(TEST_STORE_PATH + "-wal")) fs.unlinkSync(TEST_STORE_PATH + "-wal");
      if (fs.existsSync(TEST_STORE_PATH + "-shm")) fs.unlinkSync(TEST_STORE_PATH + "-shm");
    } catch {}

    memory = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
    await memory.initVecDimension(3);
  });

  afterEach(async () => {
    await memory.close();
    try {
      if (fs.existsSync(TEST_STORE_PATH)) fs.rmSync(TEST_STORE_PATH, { force: true });
      if (fs.existsSync(TEST_STORE_PATH + "-wal")) fs.rmSync(TEST_STORE_PATH + "-wal", { force: true });
      if (fs.existsSync(TEST_STORE_PATH + "-shm")) fs.rmSync(TEST_STORE_PATH + "-shm", { force: true });
      const dir = path.dirname(TEST_STORE_PATH);
      if (fs.existsSync(dir)) fs.rmSync(dir, { recursive: true, force: true });
    } catch {}
  });

  it("should handle high concurrency of setFact and getFact operations without locking", async () => {
    const promises: Promise<void>[] = [];
    
    // Mix of setFact (async worker) and getFact (sync main-thread)
    for (let i = 0; i < 100; i++) {
      promises.push(
        memory.setFact(`key_${i}`, `value_${i}`, { source: "user", category: "stress" })
      );
      
      // Attempt synchronous reads on main thread during active worker writes
      if (i % 2 === 0) {
        memory.getFact(`key_${i - 2}`); // Read previously set keys (or null)
      }
    }

    // Wait for all writes to finish
    await Promise.all(promises);

    // Verify all facts exist and count is correctly capped to MAX_FACTS (50)
    expect(memory.count).toBe(50);
  });

  it("should handle parallel vector upserts and concurrent search queries", async () => {
    const promises: Promise<void>[] = [];
    const dummyVector = [0.5, 0.5, 0.5];

    for (let i = 0; i < 50; i++) {
      promises.push(
        (async () => {
          memory.upsertVector({
            vecId: `vec_${i}`,
            type: "AXIOM",
            content: `axiom content ${i}`,
            vector: dummyVector,
            domain: "StressTest",
            category: "Math"
          });
        })()
      );
    }

    await Promise.all(promises);

    // Force flush the vector queue
    await memory.flushVectorQueue();

    // Query concurrent search similar vectors
    const searchPromises: Promise<any>[] = [];
    for (let i = 0; i < 20; i++) {
      searchPromises.push(memory.searchSimilarVectors(dummyVector, 5));
    }

    const results = await Promise.all(searchPromises);
    expect(results).toHaveLength(20);
    for (const res of results) {
      expect(res.length).toBeGreaterThan(0);
    }
  });

  it("should handle interleaved events insertion and transaction operations", async () => {
    // Start transaction, do inserts, and commit
    await memory.beginTransaction();
    
    for (let i = 0; i < 10; i++) {
      await memory.insertEvent({
        eventId: `evt_tx_${i}`,
        timestamp: Date.now(),
        phi: { facts: [`tx_fact_${i}`], entities: [] },
        psi: { sentiment: "neutral", intent: "stress", relational: "" },
        rawUserMsg: `msg_${i}`,
        rawAiReply: `reply_${i}`
      });
    }

    await memory.commitTransaction();

    const count = await memory.getUnconsolidatedCount();
    expect(count).toBe(10);
  });
});
