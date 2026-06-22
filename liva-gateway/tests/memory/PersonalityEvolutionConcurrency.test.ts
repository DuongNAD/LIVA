import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { StructuredMemory } from "../../src/memory/StructuredMemory";
import { PersonalityEvolution } from "../../src/memory/PersonalityEvolution";
import * as fs from "node:fs";
import * as path from "node:path";

let TEST_AGENT_ID = "__test_personality_agent__";
let TEST_BASE_DIR = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID);
let TEST_STORE_PATH = path.join(TEST_BASE_DIR, "personality_memory.sqlite");

describe("PersonalityEvolution Concurrency and Lost Update Tests", () => {
  let memory: StructuredMemory;

  beforeEach(async () => {
    const randId = Math.random().toString(36).substring(7) + "_" + process.pid + "_" + Date.now();
    TEST_AGENT_ID = `__test_personality_${randId}__`;
    TEST_BASE_DIR = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID);
    TEST_STORE_PATH = path.join(TEST_BASE_DIR, "personality_memory.sqlite");

    try {
      if (fs.existsSync(TEST_STORE_PATH)) fs.unlinkSync(TEST_STORE_PATH);
      if (fs.existsSync(TEST_STORE_PATH + "-wal")) fs.unlinkSync(TEST_STORE_PATH + "-wal");
      if (fs.existsSync(TEST_STORE_PATH + "-shm")) fs.unlinkSync(TEST_STORE_PATH + "-shm");
    } catch {}

    memory = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
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

  it("should serialize concurrent personality evolutions and prevent lost updates", async () => {
    const db = memory.db;
    const dbBridge = memory.dbBridge;
    
    // Initialize first to get the default state in db
    const initialState = PersonalityEvolution.getPersonalityState(db, TEST_AGENT_ID);
    expect(initialState.valence).toBe(0.5);
    expect(initialState.friendliness).toBe(0.8);
    expect(initialState.arousal).toBe(0.5);

    // Trigger 5 concurrent evolveFromTurn operations in parallel
    const promises: Promise<any>[] = [];
    for (let i = 0; i < 5; i++) {
      promises.push(
        PersonalityEvolution.evolveFromTurn(db, dbBridge, TEST_AGENT_ID, "love you, thank you so much")
      );
    }

    // Wait for all of them to resolve
    await Promise.all(promises);

    // Retrieve state again and verify all updates were applied sequentially without being lost
    const finalState = PersonalityEvolution.getPersonalityState(db, TEST_AGENT_ID);
    
    // Valence changes: 0.5 + (5 * 0.08) = 0.90
    expect(finalState.valence).toBeCloseTo(0.90, 4);

    // Arousal changes: 0.5 - (5 * 0.05) = 0.25
    expect(finalState.arousal).toBeCloseTo(0.25, 4);

    // Friendliness changes: 0.8 + (5 * 0.1) = 1.3 -> capped at 1.0
    expect(finalState.friendliness).toBe(1.0);
  });
});
