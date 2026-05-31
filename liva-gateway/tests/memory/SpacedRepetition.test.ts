import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { StructuredMemory } from "../../src/memory/StructuredMemory";
import * as fs from "node:fs";
import * as path from "node:path";

// Ensure LIVA_ENCRYPTION_KEY is set for test isolation.
if (!process.env.LIVA_ENCRYPTION_KEY) {
  process.env.LIVA_ENCRYPTION_KEY = "LIVA_TEST_KEY_32BYTES_XXXXXXXXXX";
}

const TEST_AGENT_ID = "__test_spaced_repetition__";
const TEST_BASE_DIR = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID);
const TEST_STORE_PATH = path.join(TEST_BASE_DIR, "structured_memory.sqlite");

describe("Memory Evolution - Dynamic Spaced Repetition (Ebbinghaus)", () => {
  let memory: StructuredMemory;

  beforeEach(async () => {
    vi.useFakeTimers();
    try {
      const dir = path.dirname(TEST_STORE_PATH);
      if (fs.existsSync(TEST_STORE_PATH)) fs.unlinkSync(TEST_STORE_PATH);
      if (fs.existsSync(dir)) fs.rmSync(dir, { recursive: true, force: true });
    } catch {}
    memory = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
  });

  afterEach(async () => {
    vi.useRealTimers();
    await memory.close();
    try {
      if (fs.existsSync(TEST_STORE_PATH)) fs.unlinkSync(TEST_STORE_PATH);
      const dir = path.dirname(TEST_STORE_PATH);
      if (fs.existsSync(dir)) fs.rmSync(dir, { recursive: true, force: true });
    } catch {}
  });

  it("should calculate memory decay reinforcing with touch counts (Facts)", async () => {
    const now = Date.now();
    vi.setSystemTime(now);

    // Create two facts
    await memory.setFact("fact_low_touch", "Low touch info");
    await memory.setFact("fact_high_touch", "High touch info");

    // Touch the second fact multiple times (5 times) in separate turns/flushes
    for (let i = 0; i < 5; i++) {
      memory.touchFact("fact_high_touch");
      await memory.flushFactTouches();
    }

    const db = (memory as any).db;

    // Verify access_count in DB
    const rowLow = db.prepare("SELECT access_count, memory_strength FROM facts WHERE key = ?").get("fact_low_touch") as any;
    const rowHigh = db.prepare("SELECT access_count, memory_strength FROM facts WHERE key = ?").get("fact_high_touch") as any;
    expect(rowLow.access_count).toBe(0);
    expect(rowHigh.access_count).toBe(5);

    // Manually push back last_accessed_at in DB to simulate 5 days passing
    const fiveDaysMs = 5 * 24 * 60 * 60 * 1000;
    db.prepare("UPDATE facts SET last_accessed_at = ? WHERE key = ?").run(now - fiveDaysMs, "fact_low_touch");
    db.prepare("UPDATE facts SET last_accessed_at = ? WHERE key = ?").run(now - fiveDaysMs, "fact_high_touch");

    // Execute memory decay with lambda_0 = 0.1
    // For n=0 (low touch): lambda = 0.1 / (1 + 0.1*0) = 0.1. Strength = 1.0 * e^(-0.1 * 5) = 0.6065
    // For n=5 (high touch): lambda = 0.1 / (1 + 0.1*5) = 0.0667. Strength = 1.0 * e^(-0.0667 * 5) = 0.7165
    const decayResult = await memory.applyMemoryDecay(0.1);

    const factLowAfter = db.prepare("SELECT memory_strength FROM facts WHERE key = ?").get("fact_low_touch") as any;
    const factHighAfter = db.prepare("SELECT memory_strength FROM facts WHERE key = ?").get("fact_high_touch") as any;

    expect(factLowAfter.memory_strength).toBeLessThan(factHighAfter.memory_strength);
    expect(factLowAfter.memory_strength).toBeCloseTo(Math.exp(-0.1 * 5), 4);
    expect(factHighAfter.memory_strength).toBeCloseTo(Math.exp(-(0.1 / 1.75) * 5), 4);
  });

  it("should calculate memory decay reinforcing with touch counts (Vectors)", async () => {
    const now = Date.now();
    vi.setSystemTime(now);

    // Init 3D vectors
    await memory.initVecDimension(3);

    // Upsert two vectors
    await memory.upsertVector({
      vecId: "vec_low_touch",
      type: "AXIOM",
      content: "Low touch banana fruit",
      vector: [1.0, 0.0, 0.0],
      domain: "Fruit",
      category: "Biology",
    });

    await memory.upsertVector({
      vecId: "vec_high_touch",
      type: "AXIOM",
      content: "High touch apple fruit",
      vector: [0.0, 1.0, 0.0],
      domain: "Fruit",
      category: "Biology",
    });

    // Flush queue so they exist in vectors_meta
    await memory.flushVectorQueue();

    // Query high touch vector multiple times (5 times) to trigger touches on vec_high_touch
    for (let i = 0; i < 5; i++) {
      const results = await memory.searchSimilarVectors([0.0, 1.0, 0.0], 1);
      expect(results).toHaveLength(1);
      expect(results[0].vecId).toBe("vec_high_touch");
      await memory.flushVectorTouches();
    }

    const db = (memory as any).db;

    // Check access count in DB
    const vecLow = db.prepare("SELECT access_count, decay_weight FROM vectors_meta WHERE vec_id = ?").get("vec_low_touch") as any;
    const vecHigh = db.prepare("SELECT access_count, decay_weight FROM vectors_meta WHERE vec_id = ?").get("vec_high_touch") as any;
    
    expect(vecLow.access_count).toBe(0); // 0 search hits
    expect(vecHigh.access_count).toBe(5); // 5 search hits

    // Backdate vectors_meta timestamps to 10 days ago to trigger decay
    const tenDaysMs = 10 * 24 * 60 * 60 * 1000;
    db.prepare("UPDATE vectors_meta SET last_accessed_at = ?, created_at = ? WHERE vec_id = ?")
      .run(now - tenDaysMs, now - tenDaysMs, "vec_low_touch");
    db.prepare("UPDATE vectors_meta SET last_accessed_at = ?, created_at = ? WHERE vec_id = ?")
      .run(now - tenDaysMs, now - tenDaysMs, "vec_high_touch");

    // Apply memory decay
    await memory.applyMemoryDecay(0.1);

    const vecLowAfter = db.prepare("SELECT decay_weight FROM vectors_meta WHERE vec_id = ?").get("vec_low_touch") as any;
    const vecHighAfter = db.prepare("SELECT decay_weight FROM vectors_meta WHERE vec_id = ?").get("vec_high_touch") as any;

    expect(vecLowAfter.decay_weight).toBeLessThan(vecHighAfter.decay_weight);
    expect(vecLowAfter.decay_weight).toBeCloseTo(Math.exp(-(0.1 / (1 + 0.1 * vecLow.access_count)) * 10), 4);
    expect(vecHighAfter.decay_weight).toBeCloseTo(Math.exp(-(0.1 / (1 + 0.1 * vecHigh.access_count)) * 10), 4);
  });

  it("should delete vectors when they decay below ARCHIVE_THRESHOLD (0.15)", async () => {
    const now = Date.now();
    vi.setSystemTime(now);

    await memory.initVecDimension(3);
    await memory.upsertVector({
      vecId: "vec_decay_target",
      type: "AXIOM",
      content: "This vector will be forgotten",
      vector: [1.0, 0.0, 0.0],
    });
    await memory.flushVectorQueue();

    // Set creation/access time to 20 days ago and set decay_weight to 0.20
    const db = (memory as any).db;
    const twentyDaysMs = 20 * 24 * 60 * 60 * 1000;
    db.prepare("UPDATE vectors_meta SET last_accessed_at = 0, created_at = ?, access_count = 0, decay_weight = 0.20 WHERE vec_id = ?")
      .run(now - twentyDaysMs, "vec_decay_target");

    // Apply decay
    const res = await memory.applyMemoryDecay(0.2); // high decay rate to force it to forget

    // Expect the vector to be deleted/archived
    const vecRow = db.prepare("SELECT id FROM vectors_meta WHERE vec_id = ?").get("vec_decay_target");
    expect(vecRow).toBeUndefined();
  });
});
