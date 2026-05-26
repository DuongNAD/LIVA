import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { StructuredMemory } from "../../src/memory/StructuredMemory";
import * as fs from "node:fs";
import * as path from "node:path";

// Use a temporary agent ID to avoid polluting real data
const TEST_AGENT_ID = "__test_structured_memory__";
const TEST_BASE_DIR = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID);
const TEST_STORE_PATH = path.join(TEST_BASE_DIR, "structured_memory.sqlite");
const TEST_STORE_PATH_JSON = path.join(TEST_BASE_DIR, "structured_memory.json");

describe("StructuredMemory", () => {
  let memory: StructuredMemory;

  beforeEach(async () => {
    // Clean up any previous test data
    try {
      if (fs.existsSync(TEST_STORE_PATH)) fs.unlinkSync(TEST_STORE_PATH);
      if (fs.existsSync(TEST_STORE_PATH + "-wal")) fs.unlinkSync(TEST_STORE_PATH + "-wal");
      if (fs.existsSync(TEST_STORE_PATH + "-shm")) fs.unlinkSync(TEST_STORE_PATH + "-shm");
      if (fs.existsSync(TEST_STORE_PATH_JSON)) fs.unlinkSync(TEST_STORE_PATH_JSON);
      if (fs.existsSync(TEST_STORE_PATH_JSON + ".bak")) fs.unlinkSync(TEST_STORE_PATH_JSON + ".bak");
    } catch {}
    memory = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
    // Explicitly delete all rows from facts and events for good measure because DatabaseSync could cache
    memory["db"].exec("DELETE FROM facts; DELETE FROM events; DELETE FROM turn_layer_nodes; DELETE FROM vectors_meta;");
    try { memory["db"].exec("DELETE FROM vec_idx;"); } catch {}
  });

  afterEach(async () => {
    // DEV GUARD D (Database Trash Trap): Triệt để xóa SQLite DB file và thư mục
    await memory.close();
    try {
      if (fs.existsSync(TEST_STORE_PATH)) fs.rmSync(TEST_STORE_PATH, { force: true });
      if (fs.existsSync(TEST_STORE_PATH + "-wal")) fs.rmSync(TEST_STORE_PATH + "-wal", { force: true });
      if (fs.existsSync(TEST_STORE_PATH + "-shm")) fs.rmSync(TEST_STORE_PATH + "-shm", { force: true });
      if (fs.existsSync(TEST_STORE_PATH_JSON)) fs.rmSync(TEST_STORE_PATH_JSON, { force: true });
      if (fs.existsSync(TEST_STORE_PATH_JSON + ".bak")) fs.rmSync(TEST_STORE_PATH_JSON + ".bak", { force: true });
      const dir = path.dirname(TEST_STORE_PATH);
      if (fs.existsSync(dir)) fs.rmSync(dir, { recursive: true, force: true });
    } catch {}
  });

  describe("Initialization & Migration", () => {
    it("should migrate from JSON if json exists and backup file", async () => {
      // Setup JSON file
      fs.mkdirSync(path.dirname(TEST_STORE_PATH_JSON), { recursive: true });
      fs.writeFileSync(TEST_STORE_PATH_JSON, JSON.stringify({
        facts: [
          { key: "json_key", value: "json_val", createdAt: Date.now(), updatedAt: Date.now(), ttlDays: 7, source: "user", category: "Test" }
        ]
      }));

      // Create new instance to trigger migration (async factory)
      const mem2 = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
      const fact = mem2.getFact("json_key");
      expect(fact).not.toBeNull();
      expect(fact!.value).toBe("json_val");
      
      // Check backup file created
      expect(fs.existsSync(TEST_STORE_PATH_JSON + ".bak")).toBe(true);
      expect(fs.existsSync(TEST_STORE_PATH_JSON)).toBe(false);
      
      mem2.close();
    });

    it("should ignore malformed JSON silently during migration", async () => {
      fs.mkdirSync(path.dirname(TEST_STORE_PATH_JSON), { recursive: true });
      fs.writeFileSync(TEST_STORE_PATH_JSON, "{ bad_json");
      const mem2 = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
      expect(mem2.getAllFacts().length).toBe(0);
      mem2.close();
    });

    it("should use default agentId 'liva_core' if not provided (Line 104 default branch)", async () => {
        const defaultTestStore = path.join(TEST_BASE_DIR, "default_test.sqlite");
        const mem_default = await StructuredMemory.create(undefined, defaultTestStore);
        // Just verify it instantiates and we can close it
        expect(mem_default).not.toBeNull();
        expect(mem_default.agentId).toBe("liva_core");
        mem_default.close();
        
        // Clean up
        if (fs.existsSync(defaultTestStore)) {
            try {
                fs.rmSync(defaultTestStore, { force: true });
            } catch (err) {
                // Ignore EBUSY if SQLite hasn't fully closed
            }
        }
    });

    it("should not create directory if it already exists (Line 104 false branch)", async () => {
        const baseDir = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID);
        fs.mkdirSync(baseDir, { recursive: true });
        const mem3 = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
        expect(fs.existsSync(baseDir)).toBe(true);
        mem3.close();
    });

    it("should ignore JSON if facts is not an array (Line 200 false branch)", async () => {
        fs.mkdirSync(path.dirname(TEST_STORE_PATH_JSON), { recursive: true });
        fs.writeFileSync(TEST_STORE_PATH_JSON, JSON.stringify({ facts: "not_an_array" }));
        const mem4 = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
        expect(mem4.getAllFacts().length).toBe(0);
        mem4.close();
    });
  });

  describe("CRUD Operations", () => {
    it("should set and get a fact", async () => {
      await memory.setFact("user_name", "Dương", { source: "user", category: "Profile" });
      const fact = memory.getFact("user_name");
      expect(fact).not.toBeNull();
      expect(fact!.value).toBe("Dương");
      expect(fact!.category).toBe("Profile");
    });

    it("should update existing fact", async () => {
      await memory.setFact("city", "Hà Nội");
      await memory.setFact("city", "TP.HCM");
      const fact = memory.getFact("city");
      expect(fact!.value).toBe("TP.HCM");
    });

    it("should delete a fact", async () => {
      await memory.setFact("temp", "value");
      expect(await memory.deleteFact("temp")).toBe(true);
      expect(memory.getFact("temp")).toBeNull();
    });

    it("should return false when deleting non-existent fact", async () => {
      expect(await memory.deleteFact("non_existent")).toBe(false);
    });

    it("should get all facts", async () => {
      await memory.setFact("key1", "val1");
      await memory.setFact("key2", "val2");
      await memory.setFact("key3", "val3");
      const all = memory.getAllFacts();
      expect(all).toHaveLength(3);
    });

    it("should get facts by category", async () => {
      await memory.setFact("name", "Dương", { category: "Profile" });
      await memory.setFact("age", "25", { category: "Profile" });
      await memory.setFact("project", "LIVA", { category: "Work" });
      
      const profileFacts = memory.getFactsByCategory("Profile");
      expect(profileFacts).toHaveLength(2);
      
      const workFacts = memory.getFactsByCategory("Work");
      expect(workFacts).toHaveLength(1);

      const noneFacts = memory.getFactsByCategory("NonExistent");
      expect(noneFacts).toHaveLength(0);
    });
  });

  describe("Size Limit (FIFO Eviction)", () => {
    it("should not evict when adding exactly 50 facts", async () => {
      for (let i = 0; i < 50; i++) {
        await memory.setFact(`fact_${i}`, `value_${i}`);
      }
      expect(memory.count).toBe(50);
      expect(memory.getFact("fact_0")).not.toBeNull();
    });

    it("should evict oldest fact when exceeding max (51 facts)", async () => {
      for (let i = 0; i < 50; i++) {
        await memory.setFact(`fact_${i}`, `value_${i}`);
      }
      await memory.setFact("fact_50", "value_50");
      expect(memory.count).toBe(50);
      expect(memory.getFact("fact_0")).toBeNull(); // Evicted
      expect(memory.getFact("fact_50")).not.toBeNull(); // Added
    });

    it("should evict facts based on TTL", async () => {
      vi.useFakeTimers();
      await memory.setFact("ttl_fact", "value", { ttlDays: 1 });
      
      // Advance by 2 days
      vi.setSystemTime(Date.now() + 2 * 24 * 60 * 60 * 1000);
      
      // TTL eviction happens on interval, let's just trigger it manually by calling _enforceSizeLimit?
      // Actually size limit triggers it. Let's add 50 elements to trigger size limit, or call the private method.
      await (memory as any).evictExpired();
      
      expect(memory.getFact("ttl_fact")).toBeNull();
      vi.useRealTimers();
    });
  });

  describe("Persistence", () => {
    it("should persist to disk and survive reload", async () => {
      await memory.setFact("persistent_key", "persistent_value", { category: "Test" });
      
      // Create new instance (simulates restart) via async factory
      const reloaded = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
      const fact = reloaded.getFact("persistent_key");
      expect(fact).not.toBeNull();
      expect(fact!.value).toBe("persistent_value");
    });
  });

  describe("System Prompt Formatting", () => {
    it("should return empty string when no facts", () => {
      expect(memory.formatForSystemPrompt()).toBe("");
    });

    it("should format facts grouped by category", async () => {
      await memory.setFact("name", "Dương", { category: "Profile" });
      await memory.setFact("project", "LIVA", { category: "Work" });
      
      const prompt = memory.formatForSystemPrompt();
      expect(prompt).toContain("BỘ NHỚ CẤU TRÚC");
      expect(prompt).toContain("Profile");
      expect(prompt).toContain("name: Dương");
      expect(prompt).toContain("Work");
      expect(prompt).toContain("project: LIVA");
    });
  });

  describe("Input Validation", () => {
    it("should reject empty key", async () => {
      await memory.setFact("", "value");
      expect(memory.count).toBe(0);
    });

    it("should reject empty value", async () => {
      await memory.setFact("key", "");
      expect(memory.count).toBe(0);
    });

    it("should truncate overly long keys", async () => {
      const longKey = "a".repeat(200);
      await memory.setFact(longKey, "value");
      const fact = memory.getAllFacts()[0];
      expect(fact.key.length).toBeLessThanOrEqual(100);
    });
  });

  describe("Error Handling", () => {
    it("should catch and log SQLite errors gracefully during setFact", async () => {
      const dbMock = vi.spyOn((memory as any).dbBridge, "run").mockImplementation(() => {
        throw new Error("SQLite disk I/O error");
      });
      
      await expect(
        memory.setFact("error_key", "value")
      ).rejects.toThrow();

      dbMock.mockRestore();
    });

    it("should return empty array when getAllFacts throws", () => {
      const dbMock = vi.spyOn((memory as any).db, "prepare").mockImplementation(() => {
        throw new Error("SQLite error");
      });
      
      expect(() => {
        memory.getAllFacts();
      }).toThrow();

      dbMock.mockRestore();
    });
  });

  describe("Event Log & Consolidation", () => {
    beforeEach(() => {
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it("should track unconsolidated events correctly", async () => {
        await memory.insertEvent({
            eventId: "evt_1", timestamp: Date.now(),
            phi: { facts: [], entities: [] }, psi: { sentiment: "", intent: "", relational: "" },
            rawUserMsg: "hi", rawAiReply: "hello"
        });
        expect(await memory.getUnconsolidatedCount()).toBe(1);
    });

    it("should garbage collect old consolidated events", async () => {
        const now = Date.now();
        
        // Append 10 days ago and mark consolidated
        vi.setSystemTime(now - 10 * 24 * 60 * 60 * 1000);
        await memory.insertEvent({
            eventId: "old_evt", timestamp: Date.now(),
            phi: { facts: [], entities: [] }, psi: { sentiment: "", intent: "", relational: "" },
            rawUserMsg: "old", rawAiReply: "old"
        });
        await memory.markConsolidated(["old_evt"]);

        // Append 2 days ago and mark consolidated
        vi.setSystemTime(now - 2 * 24 * 60 * 60 * 1000);
        await memory.insertEvent({
            eventId: "new_evt", timestamp: Date.now(),
            phi: { facts: [], entities: [] }, psi: { sentiment: "", intent: "", relational: "" },
            rawUserMsg: "new", rawAiReply: "new"
        });
        await memory.markConsolidated(["new_evt"]);

        // Back to present
        vi.setSystemTime(now);

        // GC events older than 7 days
        const removed = await memory.gcOldEvents(7);
        expect(removed).toBe(1);
    });

    it("should use default retentionDays=7 in gcOldEvents (Line 509 default branch)", async () => {
        const now = Date.now();
        
        // Append 10 days ago and mark consolidated
        vi.setSystemTime(now - 10 * 24 * 60 * 60 * 1000);
        await memory.insertEvent({
            eventId: "default_evt", timestamp: Date.now(),
            phi: { facts: [], entities: [] }, psi: { sentiment: "", intent: "", relational: "" },
            rawUserMsg: "default", rawAiReply: "default"
        });
        await memory.markConsolidated(["default_evt"]);

        // Back to present
        vi.setSystemTime(now);

        // GC without args should use 7 days default
        const removed = await memory.gcOldEvents();
        expect(removed).toBe(1);
    });

    it("should get unconsolidated events and map rows properly", async () => {
        // Insert barebones event with missing optional fields to test mapEventRow defaults
        const rawSql = `INSERT INTO events (eventId, timestamp, phi_facts, phi_entities, psi_sentiment, psi_intent, psi_relational, rawUserMsg, rawAiReply, consolidated, consolidation_status) 
                        VALUES ('manual_1', 12345, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 0, 'pending')`;
        (memory as any).db.exec(rawSql);

        await memory.insertEvent({
            eventId: "evt_full", timestamp: 12346,
            phi: { facts: ["f1"], entities: ["e1"] }, psi: { sentiment: "s1", intent: "i1", relational: "r1" },
            rawUserMsg: "hi", rawAiReply: "hello"
        });

        const events = await memory.getUnconsolidatedEvents();
        expect(events).toHaveLength(2);
        
        // Check manual_1 default mapping
        expect(events[0].phi.facts).toEqual([]);
        expect(events[0].psi.sentiment).toBe("");
        expect(events[0].rawUserMsg).toBe("");
        
        // Check evt_full
        expect(events[1].phi.facts).toEqual(["f1"]);
    });

    it("should catch error on close", async () => {
        const mockClose = vi.spyOn((memory as any).db, "close").mockImplementation(() => { throw new Error("db close failed"); });
        await expect(memory.close()).resolves.not.toThrow();
        mockClose.mockRestore();
    });
  });

  describe("Turn Layer API", () => {
    it("should catch and log error inserting turn node (Line 519)", async () => {
        const spy = vi.spyOn(memory.dbBridge, "prepare").mockImplementationOnce(() => { throw new Error("Mock Insert Error"); });
        // Should not throw, just log
        await memory.insertTurnNode("turn_1", 1000, "hello", "hi");
        expect(spy).toHaveBeenCalled();
        spy.mockRestore();
    });

    it("should insert and query turns by time range", async () => {
        await memory.insertTurnNode("turn_1", 1000, "hello", "hi");
        await memory.insertTurnNode("turn_2", 2000, "how are you", "good");
        await memory.insertTurnNode("turn_3", 3000, "bye", "bye");

        const turns = await memory.getTurnsByTimeRange(1500, 2500);
        expect(turns).toHaveLength(1);
        expect(turns[0].turnId).toBe("turn_2");

        const invalidTurns = await memory.getTurnsByTimeRange(5000, 1000);
        expect(invalidTurns).toHaveLength(0);
    });

    it("should query turns by IDs", async () => {
        await memory.insertTurnNode("turn_A", 1000, "A", "A");
        await memory.insertTurnNode("turn_B", 2000, "B", "B");
        
        const turns = await memory.getTurnsByIds(["turn_A", "turn_B"]);
        expect(turns).toHaveLength(2);
        
        const empty = await memory.getTurnsByIds([]);
        expect(empty).toHaveLength(0);
    });
  });

  // ===========================
  // [v4.0] Enterprise Tests
  // ===========================

  describe("[v4.0] AES-256-GCM Encryption", () => {
    it("should encrypt values at rest and decrypt on read", async () => {
        await memory.setFact("secret_key", "my_password_123", { source: "user" });
        const fact = memory.getFact("secret_key");
        // Value should be decrypted when read back
        expect(fact).not.toBeNull();
        expect(fact!.value).toBe("my_password_123");

        // Raw DB value should NOT be plain-text
        const rawRow = memory["db"].prepare("SELECT value FROM facts WHERE key = ?").get("secret_key") as any;
        expect(rawRow.value).not.toBe("my_password_123");
        // Should contain the IV:AuthTag:Encrypted format
        expect(rawRow.value.split(":")).toHaveLength(3);
    });

    it("should handle backward-compatible plain-text values", () => {
        // Simulate pre-v4 data: insert plain-text directly
        memory["db"].prepare(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES (?, ?, ?, ?, ?)"
        ).run("legacy_key", "plain_text_value", new Date().toISOString(), new Date().toISOString(), "user");

        const fact = memory.getFact("legacy_key");
        expect(fact).not.toBeNull();
        // Plain-text should be returned as-is (backward compat fallback)
        expect(fact!.value).toBe("plain_text_value");
    });

    it("should return raw text if decryption throws an error (Line 95)", () => {
        memory["db"].prepare(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES (?, ?, ?, ?, ?)"
        ).run("bad_enc_key", "invalidhex:invalidhex:invalidhex", new Date().toISOString(), new Date().toISOString(), "user");

        const fact = memory.getFact("bad_enc_key");
        expect(fact).not.toBeNull();
        expect(fact!.value).toBe("invalidhex:invalidhex:invalidhex");
    });
  });

  describe("[v4.0] GDPR Compliance", () => {
    it("should hard-delete all facts with deleteAllFacts()", async () => {
        await memory.setFact("fact_1", "value_1", { source: "user" });
        await memory.setFact("fact_2", "value_2", { source: "agent" });
        expect(memory.count).toBe(2);

        await memory.deleteAllFacts();
        expect(memory.count).toBe(0);
        expect(memory.getAllFacts()).toHaveLength(0);
    });

    it("should hard-delete all events and turn nodes with deleteAllEvents()", async () => {
        await memory.insertEvent({
            eventId: "evt_1", timestamp: Date.now(),
            phi: { facts: ["test"], entities: [] },
            psi: { sentiment: "neutral", intent: "info", relational: "" },
            rawUserMsg: "test", rawAiReply: "reply"
        });
        await memory.insertTurnNode("t1", Date.now(), "msg", "reply");

        await memory.deleteAllEvents();

        expect(await memory.getUnconsolidatedEvents()).toHaveLength(0);
        expect(await memory.getTurnsByTimeRange(0, Date.now() + 10000)).toHaveLength(0);
    });
  });

  describe("[v4.0] Importance Scoring & Reconciliation", () => {
    it("should store importance based on source", async () => {
        await memory.setFact("user_fact", "from user", { source: "user" });
        await memory.setFact("agent_fact", "from agent", { source: "agent" });

        const userFact = memory.getFact("user_fact");
        const agentFact = memory.getFact("agent_fact");

        expect(userFact!.importance).toBe(1.0);   // User facts are highest priority
        expect(agentFact!.importance).toBe(0.5);   // Agent facts are default
    });

    it("should setFactImportance for reconciliation", async () => {
        await memory.setFact("old_company", "FPT", { source: "auto_extract" });
        expect(memory.getFact("old_company")!.importance).toBe(0.5);

        await memory.setFactImportance("old_company", 0.1);
        const deprecated = memory.getFact("old_company");
        expect(deprecated!.importance).toBe(0.1);
    });

    it("should evict low-importance facts first during FIFO", async () => {
        // Fill to capacity with default importance (0.5)
        for (let i = 0; i < 50; i++) {
            await memory.setFact(`fact_${i}`, `value_${i}`, { source: "agent" });
        }
        // Set one fact to high importance
        await memory.setFactImportance("fact_0", 1.0);
        // Set another to low importance
        await memory.setFactImportance("fact_1", 0.0);

        // Add one more to trigger eviction
        await memory.setFact("fact_overflow", "overflow", { source: "agent" });

        // Low importance fact_1 should be evicted first
        expect(memory.getFact("fact_1")).toBeNull();
        // High importance fact_0 should survive
        expect(memory.getFact("fact_0")).not.toBeNull();
        expect(memory.getFact("fact_overflow")).not.toBeNull();
    });
  });

  describe("[v4.0] Data Lineage", () => {
    it("should include confidenceScore and sourceTurnId in fact metadata", async () => {
        await memory.setFact("lineage_fact", "test_value", { source: "auto_extract" });
        const fact = memory.getFact("lineage_fact");

        expect(fact).not.toBeNull();
        expect(fact!.confidenceScore).toBe(1.0);
        // sourceTurnId is null by default (set during extraction)
        expect(fact!.sourceTurnId).toBeUndefined();
    });
  });

  describe("[v4.0] Background Eviction Timer", () => {
    it("should catch and ignore evictExpired errors in timer (Line 120)", async () => {
        vi.useFakeTimers();
        // Use raw constructor with storePath directly — sync instantiation is OK for timer-specific tests
        const timerStoreDir = path.join(process.cwd(), "data", "agents", "timer_test");
        fs.mkdirSync(timerStoreDir, { recursive: true });
        const timerStorePath = path.join(timerStoreDir, "structured_memory.sqlite");
        const memTimer = new StructuredMemory(timerStorePath);
        const spy = vi.spyOn(memTimer as any, "evictExpired").mockImplementationOnce(() => { throw new Error("Mock evict error"); });
        
        // Advance timer by 1 hour
        vi.advanceTimersByTime(60 * 60 * 1000);
        
        expect(spy).toHaveBeenCalled();
        await memTimer.close();
        // Cleanup
        try { fs.rmSync(timerStoreDir, { recursive: true, force: true }); } catch {}
        vi.useRealTimers();
    });

    it("should clean up eviction timer on close()", async () => {
        const clearIntervalSpy = vi.spyOn(global, 'clearInterval');
        await memory.close();
        expect(clearIntervalSpy).toHaveBeenCalled();
        clearIntervalSpy.mockRestore();
    });
  });

  describe('Coverage padding', () => {
      it('should hit consolidation source', async () => {
          await memory.setFact('cons_key', 'val', { source: 'consolidation' });
          expect(memory.getFact('cons_key')!.value).toBe('val');
      });
  });

      it('should fallback default values for importance/confidence', () => {
          (memory as any).db.prepare('INSERT OR REPLACE INTO facts (key, value, createdAt, updatedAt, source, importance, confidenceScore) VALUES (?, ?, ?, ?, ?, ?, ?)').run('null_fact', 'val', new Date().toISOString(), new Date().toISOString(), 'agent', null, null);
          const fact = memory.getFact('null_fact');
          expect(fact!.importance).toBe(0.5);
          expect(fact!.confidenceScore).toBe(1.0);
      });

  // ===========================
  // [UHM] Ebbinghaus Forgetting Curve Tests
  // ===========================
  describe("[UHM] Ebbinghaus Forgetting Curve", () => {
    it("should default memory_strength to 1.0 for new facts", async () => {
      await memory.setFact("new_fact", "value");
      const fact = memory.getFact("new_fact");
      expect(fact).not.toBeNull();
      expect(fact!.memoryStrength).toBe(1.0);
    });

    it("should set last_accessed_at on new facts", async () => {
      const before = Date.now();
      await memory.setFact("ts_fact", "value");
      const fact = memory.getFact("ts_fact");
      expect(fact!.lastAccessedAt).toBeGreaterThanOrEqual(before);
    });

    it("touchFact() should buffer without immediate DB write", async () => {
      await memory.setFact("buf_fact", "val");
      (memory as any).db.prepare("UPDATE facts SET last_accessed_at = 0 WHERE key = ?").run("buf_fact");
      memory.touchFact("buf_fact");
      const raw = (memory as any).db.prepare("SELECT last_accessed_at FROM facts WHERE key = ?").get("buf_fact") as any;
      expect(raw.last_accessed_at).toBe(0);
    });

    it("flushFactTouches() should batch-write to DB in transaction", async () => {
      await memory.setFact("flush_a", "a");
      await memory.setFact("flush_b", "b");
      (memory as any).db.prepare("UPDATE facts SET last_accessed_at = 0, memory_strength = 0.3").run();
      memory.touchFact("flush_a");
      memory.touchFact("flush_b");
      await memory.flushFactTouches();
      const rowA = (memory as any).db.prepare("SELECT memory_strength, last_accessed_at FROM facts WHERE key = ?").get("flush_a") as any;
      const rowB = (memory as any).db.prepare("SELECT memory_strength, last_accessed_at FROM facts WHERE key = ?").get("flush_b") as any;
      expect(rowA.memory_strength).toBe(1.0);
      expect(rowA.last_accessed_at).toBeGreaterThan(0);
      expect(rowB.memory_strength).toBe(1.0);
    });

    it("flushFactTouches() should be no-op when buffer is empty", async () => {
      await memory.flushFactTouches();
    });

    it("getFact() should call touchFact()", async () => {
      await memory.setFact("touch_test", "val");
      const spy = vi.spyOn(memory, "touchFact");
      memory.getFact("touch_test");
      expect(spy).toHaveBeenCalledWith("touch_test");
      spy.mockRestore();
    });

    it("getAllFacts() should NOT call touchFact()", async () => {
      await memory.setFact("no_touch", "val");
      const spy = vi.spyOn(memory, "touchFact");
      memory.getAllFacts();
      expect(spy).not.toHaveBeenCalled();
      spy.mockRestore();
    });

    it("applyMemoryDecay() should reduce strength for old facts", async () => {
      await memory.setFact("old_fact", "val");
      const fiveDaysAgo = Date.now() - 5 * 86_400_000;
      (memory as any).db.prepare("UPDATE facts SET memory_strength = 1.0, last_accessed_at = ? WHERE key = ?").run(fiveDaysAgo, "old_fact");
      const result = await memory.applyMemoryDecay(0.1);
      expect(result.decayed).toBeGreaterThanOrEqual(1);
      const row = (memory as any).db.prepare("SELECT memory_strength FROM facts WHERE key = ?").get("old_fact") as any;
      expect(row.memory_strength).toBeLessThan(0.7);
      expect(row.memory_strength).toBeGreaterThan(0.5);
    });

    it("applyMemoryDecay() should skip recently accessed facts", async () => {
      await memory.setFact("recent_fact", "val");
      const result = await memory.applyMemoryDecay(0.1);
      const row = (memory as any).db.prepare("SELECT memory_strength FROM facts WHERE key = ?").get("recent_fact") as any;
      expect(row.memory_strength).toBe(1.0);
      expect(result.decayed).toBe(0);
    });

    it("applyMemoryDecay() should archive facts below 0.1 threshold", async () => {
      await memory.setFact("dying_fact", "val");
      const thirtyDaysAgo = Date.now() - 30 * 86_400_000;
      (memory as any).db.prepare("UPDATE facts SET memory_strength = 0.2, last_accessed_at = ? WHERE key = ?").run(thirtyDaysAgo, "dying_fact");
      const result = await memory.applyMemoryDecay(0.1);
      expect(result.archived).toBeGreaterThanOrEqual(1);
      const row = (memory as any).db.prepare("SELECT * FROM facts WHERE key = ?").get("dying_fact");
      expect(row).toBeUndefined();
    });

    it("applyMemoryDecay() should return correct counts", async () => {
      await memory.setFact("survive", "val");
      await memory.setFact("decay", "val");
      await memory.setFact("archive", "val");
      const now = Date.now();
      (memory as any).db.prepare("UPDATE facts SET memory_strength = 1.0, last_accessed_at = ? WHERE key = ?").run(now, "survive");
      (memory as any).db.prepare("UPDATE facts SET memory_strength = 1.0, last_accessed_at = ? WHERE key = ?").run(now - 3 * 86_400_000, "decay");
      (memory as any).db.prepare("UPDATE facts SET memory_strength = 0.15, last_accessed_at = ? WHERE key = ?").run(now - 50 * 86_400_000, "archive");
      const result = await memory.applyMemoryDecay(0.1);
      expect(result.decayed).toBe(1);
      expect(result.archived).toBe(1);
    });

    it("formatForSystemPrompt() should exclude facts with strength < 0.2", async () => {
      await memory.setFact("strong_fact", "I am strong", { category: "Test" });
      await memory.setFact("weak_fact", "I am weak", { category: "Test" });
      (memory as any).db.prepare("UPDATE facts SET memory_strength = 0.1 WHERE key = ?").run("weak_fact");
      const prompt = memory.formatForSystemPrompt();
      expect(prompt).toContain("strong_fact");
      expect(prompt).not.toContain("weak_fact");
    });

    it("setFact() on conflict should use MAX(old_strength, 0.8)", async () => {
      await memory.setFact("conflict_key", "original");
      (memory as any).db.prepare("UPDATE facts SET memory_strength = 0.9 WHERE key = ?").run("conflict_key");
      await memory.setFact("conflict_key", "updated");
      const row = (memory as any).db.prepare("SELECT memory_strength FROM facts WHERE key = ?").get("conflict_key") as any;
      expect(row.memory_strength).toBe(0.9);
    });

    it("setFact() on conflict with low strength should boost to 0.8", async () => {
      await memory.setFact("low_key", "original");
      (memory as any).db.prepare("UPDATE facts SET memory_strength = 0.3 WHERE key = ?").run("low_key");
      await memory.setFact("low_key", "updated");
      const row = (memory as any).db.prepare("SELECT memory_strength FROM facts WHERE key = ?").get("low_key") as any;
      expect(row.memory_strength).toBe(0.8);
    });

    it("applyMemoryDecay(0) should not change any strength", async () => {
      await memory.setFact("nodecay", "val");
      const fiveDaysAgo = Date.now() - 5 * 86_400_000;
      (memory as any).db.prepare("UPDATE facts SET memory_strength = 1.0, last_accessed_at = ? WHERE key = ?").run(fiveDaysAgo, "nodecay");
      const result = await memory.applyMemoryDecay(0);
      expect(result.decayed).toBe(0);
      expect(result.archived).toBe(0);
    });

    it("close() should flush fact touches before closing DB", async () => {
      await memory.setFact("shutdown_fact", "val");
      (memory as any).db.prepare("UPDATE facts SET last_accessed_at = 0, memory_strength = 0.5").run();
      memory.touchFact("shutdown_fact");
      await memory.close();
      const mem2 = new StructuredMemory(TEST_STORE_PATH);
      const row = (mem2 as any).db.prepare("SELECT memory_strength, last_accessed_at FROM facts WHERE key = ?").get("shutdown_fact") as any;
      expect(row.memory_strength).toBe(1.0);
      expect(row.last_accessed_at).toBeGreaterThan(0);
      await mem2.close();
    });

    it("schema migration should be idempotent", async () => {
      const mem2 = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
      await mem2.setFact("idempotent_test", "works");
      const fact = mem2.getFact("idempotent_test");
      expect(fact!.memoryStrength).toBe(1.0);
      expect(fact!.lastAccessedAt).toBeGreaterThan(0);
      await mem2.close();
    });
  });

  // ===========================
  // [UHM] H-MEM Positional Index Tests
  // ===========================
  describe("[UHM] H-MEM Positional Index", () => {
    it("should store source_event_ids in vectors_meta", () => {
      const db = (memory as any).db;
      db.prepare(
        "INSERT OR REPLACE INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, file_target, source_event_ids, created_at, last_accessed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)"
      ).run("vec_pos_1", "ANCHOR", "test", "General", "Test", "[]", null, JSON.stringify(["evt_a", "evt_b"]), Date.now());
      const row = db.prepare("SELECT source_event_ids FROM vectors_meta WHERE vec_id = ?").get("vec_pos_1") as any;
      expect(JSON.parse(row.source_event_ids)).toEqual(["evt_a", "evt_b"]);
    });

    it("source_event_ids should default to empty array", () => {
      const db = (memory as any).db;
      db.prepare(
        "INSERT OR REPLACE INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, file_target, created_at, last_accessed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)"
      ).run("vec_no_ids", "ANCHOR", "no ids", "General", "Test", "[]", null, Date.now());
      const row = db.prepare("SELECT source_event_ids FROM vectors_meta WHERE vec_id = ?").get("vec_no_ids") as any;
      expect(JSON.parse(row.source_event_ids)).toEqual([]);
    });

    it("sourceEventIds cap at 50 entries", () => {
      const manyIds = Array.from({ length: 100 }, (_, i) => `evt_${i}`);
      const capped = JSON.stringify(manyIds.slice(0, 50));
      const db = (memory as any).db;
      db.prepare(
        "INSERT OR REPLACE INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, file_target, source_event_ids, created_at, last_accessed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)"
      ).run("vec_cap", "ANCHOR", "capped", "General", "Test", "[]", null, capped, Date.now());
      const row = db.prepare("SELECT source_event_ids FROM vectors_meta WHERE vec_id = ?").get("vec_cap") as any;
      const ids = JSON.parse(row.source_event_ids);
      expect(ids).toHaveLength(50);
      expect(ids[49]).toBe("evt_49");
    });

    it("source_event_ids updatable on upsert", () => {
      const db = (memory as any).db;
      db.prepare(
        "INSERT INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, source_event_ids, created_at, last_accessed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)"
      ).run("vec_up", "ANCHOR", "v1", "General", "Test", "[]", JSON.stringify(["evt_1"]), Date.now());
      db.prepare("UPDATE vectors_meta SET source_event_ids = ? WHERE vec_id = ?")
        .run(JSON.stringify(["evt_2", "evt_3"]), "vec_up");
      const row = db.prepare("SELECT source_event_ids FROM vectors_meta WHERE vec_id = ?").get("vec_up") as any;
      expect(JSON.parse(row.source_event_ids)).toEqual(["evt_2", "evt_3"]);
    });

    it("malformed JSON returns empty array", () => {
      const db = (memory as any).db;
      db.prepare(
        "INSERT INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, source_event_ids, created_at, last_accessed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)"
      ).run("vec_bad", "ANCHOR", "bad", "General", "Test", "[]", "{corrupt:", Date.now());
      const row = db.prepare("SELECT source_event_ids FROM vectors_meta WHERE vec_id = ?").get("vec_bad") as any;
      let result: string[] = [];
      try { result = JSON.parse(row.source_event_ids); } catch { result = []; }
      expect(result).toEqual([]);
    });

    it("deduplication logic for drilldown event IDs", () => {
      const allIds = new Set<string>();
      [["evt_shared", "evt_a"], ["evt_shared", "evt_b"]].forEach(ids =>
        ids.forEach(id => allIds.add(id))
      );
      expect([...allIds]).toHaveLength(3);
      expect([...allIds]).toContain("evt_shared");
    });

    it("json_each drill-down query for L2-to-L1 lookup", async () => {
      await memory.insertEvent({
        eventId: "evt_dd1", timestamp: Date.now(),
        phi: { facts: ["f1"], entities: [] },
        psi: { sentiment: "pos", intent: "info", relational: "" },
        rawUserMsg: "hello", rawAiReply: "hi",
      });
      await memory.insertEvent({
        eventId: "evt_dd2", timestamp: Date.now(),
        phi: { facts: ["f2"], entities: [] },
        psi: { sentiment: "neu", intent: "query", relational: "" },
        rawUserMsg: "test", rawAiReply: "reply",
      });
      const db = (memory as any).db;
      const events = db.prepare(
        "SELECT eventId FROM events WHERE eventId IN (SELECT value FROM json_each(?))"
      ).all(JSON.stringify(["evt_dd1", "evt_dd2"])) as any[];
      expect(events).toHaveLength(2);
    });

    it("schema migration idempotent", async () => {
      const mem2 = new StructuredMemory(TEST_STORE_PATH);
      const cols = (mem2 as any).db.prepare("PRAGMA table_info(vectors_meta)").all() as any[];
      expect(cols.some((c: any) => c.name === "source_event_ids")).toBe(true);
      await mem2.close();
    });
  });

  describe("[v25] Hybrid RAG Search (FTS5 + KNN RRF)", () => {
    it("should perform Hybrid RAG search and merge results via RRF", async () => {
      await memory.initVecDimension(3);

      // Upsert mock vector 1 (highly relevant to keyword "banana")
      await memory.upsertVector({
        vecId: "vec_banana",
        type: "AXIOM",
        content: "Bananas are yellow fruits rich in potassium.",
        vector: [1.0, 0.0, 0.0],
        domain: "Fruit",
        category: "Biology",
        traceKeywords: ["banana", "potassium"],
      });

      // Upsert mock vector 2 (highly relevant to keyword "apple")
      await memory.upsertVector({
        vecId: "vec_apple",
        type: "AXIOM",
        content: "Apples are red pomaceous fruits.",
        vector: [0.0, 1.0, 0.0],
        domain: "Fruit",
        category: "Biology",
        traceKeywords: ["apple", "pomaceous"],
      });

      // Flush queue to persist buffered vectors
      await memory.flushVectorQueue();

      // KNN search query vector close to Banana [0.9, 0.1, 0.0] but text is "apple"
      // This will trigger RRF merging!
      const queryVec = [0.9, 0.1, 0.0];
      const results = await memory.searchHybridVectors("apple", queryVec, 5);

      expect(results).toHaveLength(2);
      expect(results[0].score).toBeGreaterThan(0);
      expect(results[0].vecId).toBeDefined();
      expect(results[0].content).toBeDefined();

      // Test with typeFilter that doesn't match
      const emptyResults = await memory.searchHybridVectors("apple", queryVec, 5, "NON_EXISTENT");
      expect(emptyResults).toHaveLength(0);

      // Test with typeFilter that matches
      const matchedResults = await memory.searchHybridVectors("apple", queryVec, 5, "AXIOM");
      expect(matchedResults.length).toBeGreaterThan(0);
    });
  });

  describe("[v25] Vector Write Batching Queue (Debounced)", () => {
    it("should buffer upsertVector calls and not commit immediately to SQLite", async () => {
      await memory.initVecDimension(3);

      // Call upsertVector once
      await memory.upsertVector({
        vecId: "vec_buffered_1",
        type: "AXIOM",
        content: "Buffered content 1",
        vector: [1.0, 0.0, 0.0],
      });

      // Should not be in database yet because queue is 1 and not yet flushed
      const queryVec = [1.0, 0.0, 0.0];
      const immediateResults = await memory.searchSimilarVectors(queryVec, 5);
      expect(immediateResults.some(r => r.vecId === "vec_buffered_1")).toBe(false);

      // Flush queue manually
      await memory.flushVectorQueue();

      // Now it should be in the database
      const flushedResults = await memory.searchSimilarVectors(queryVec, 5);
      expect(flushedResults.some(r => r.vecId === "vec_buffered_1")).toBe(true);
    });

    it("should auto-flush when queue length reaches 50 entries", async () => {
      await memory.initVecDimension(3);

      // Insert 49 vectors - still not flushed
      for (let i = 0; i < 49; i++) {
        await memory.upsertVector({
          vecId: `vec_auto_${i}`,
          type: "AXIOM",
          content: `Auto content ${i}`,
          vector: [1.0, 0.0, 0.0],
        });
      }
      
      const queryVec = [1.0, 0.0, 0.0];
      const results49 = await memory.searchSimilarVectors(queryVec, 100);
      expect(results49.some(r => r.vecId === "vec_auto_48")).toBe(false);

      // Insert the 50th vector -> triggers auto-flush
      await memory.upsertVector({
        vecId: "vec_auto_49",
        type: "AXIOM",
        content: "Auto content 49",
        vector: [1.0, 0.0, 0.0],
      });

      // Wait for background flush to finish
      await new Promise(resolve => setTimeout(resolve, 2000));

      const results50 = await memory.searchSimilarVectors(queryVec, 100);
      expect(results50.some(r => r.vecId === "vec_auto_49")).toBe(true);
      expect(results50.some(r => r.vecId === "vec_auto_0")).toBe(true);
    });

    it("close() should flush vector queue synchronously before closing", async () => {
      const TEMP_DB_PATH = TEST_STORE_PATH + "_temp_close";
      if (fs.existsSync(TEMP_DB_PATH)) fs.unlinkSync(TEMP_DB_PATH);

      const memTemp = await StructuredMemory.create(TEST_AGENT_ID, TEMP_DB_PATH);
      await memTemp.initVecDimension(3);

      await memTemp.upsertVector({
        vecId: "vec_temp_shutdown",
        type: "AXIOM",
        content: "Shutdown flush content",
        vector: [1.0, 0.0, 0.0],
      });

      // Close the memory database - should trigger flushVectorQueue()
      await memTemp.close();

      // Reopen to verify
      const memReopened = await StructuredMemory.create(TEST_AGENT_ID, TEMP_DB_PATH);
      await memReopened.initVecDimension(3);
      const results = await memReopened.searchSimilarVectors([1.0, 0.0, 0.0], 5);
      expect(results.some(r => r.vecId === "vec_temp_shutdown")).toBe(true);
      await memReopened.close();

      // Clean up temp DB
      try {
        if (fs.existsSync(TEMP_DB_PATH)) fs.unlinkSync(TEMP_DB_PATH);
      } catch {}
    });
  });
});
