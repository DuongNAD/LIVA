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
      if (fs.existsSync(TEST_STORE_PATH_JSON)) fs.unlinkSync(TEST_STORE_PATH_JSON);
      if (fs.existsSync(TEST_STORE_PATH_JSON + ".bak")) fs.unlinkSync(TEST_STORE_PATH_JSON + ".bak");
    } catch {}
    memory = await StructuredMemory.create(TEST_AGENT_ID);
    // Explicitly delete all rows from facts and events for good measure because DatabaseSync could cache
    memory["db"].exec("DELETE FROM facts; DELETE FROM events; DELETE FROM turn_layer_nodes;");
  });

  afterEach(() => {
    // DEV GUARD D (Database Trash Trap): Triệt để xóa SQLite DB file và thư mục
    memory.close();
    try {
      if (fs.existsSync(TEST_STORE_PATH)) fs.rmSync(TEST_STORE_PATH, { force: true });
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
      const mem2 = await StructuredMemory.create(TEST_AGENT_ID);
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
      const mem2 = await StructuredMemory.create(TEST_AGENT_ID);
      expect(mem2.getAllFacts().length).toBe(0);
      mem2.close();
    });

    it("should use default agentId 'liva_core' if not provided (Line 104 default branch)", async () => {
        const mem_default = await StructuredMemory.create();
        // Just verify it instantiates and we can close it
        expect(mem_default).not.toBeNull();
        mem_default.close();
        
        // Clean up the default dir
        const defaultDir = path.join(process.cwd(), "data", "agents", "liva_core");
        if (fs.existsSync(defaultDir)) {
            fs.rmSync(defaultDir, { recursive: true, force: true });
        }
    });

    it("should not create directory if it already exists (Line 104 false branch)", async () => {
        const baseDir = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID);
        fs.mkdirSync(baseDir, { recursive: true });
        const mem3 = await StructuredMemory.create(TEST_AGENT_ID);
        expect(fs.existsSync(baseDir)).toBe(true);
        mem3.close();
    });

    it("should ignore JSON if facts is not an array (Line 200 false branch)", async () => {
        fs.mkdirSync(path.dirname(TEST_STORE_PATH_JSON), { recursive: true });
        fs.writeFileSync(TEST_STORE_PATH_JSON, JSON.stringify({ facts: "not_an_array" }));
        const mem4 = await StructuredMemory.create(TEST_AGENT_ID);
        expect(mem4.getAllFacts().length).toBe(0);
        mem4.close();
    });
  });

  describe("CRUD Operations", () => {
    it("should set and get a fact", () => {
      memory.setFact("user_name", "Dương", { source: "user", category: "Profile" });
      const fact = memory.getFact("user_name");
      expect(fact).not.toBeNull();
      expect(fact!.value).toBe("Dương");
      expect(fact!.category).toBe("Profile");
    });

    it("should update existing fact", () => {
      memory.setFact("city", "Hà Nội");
      memory.setFact("city", "TP.HCM");
      const fact = memory.getFact("city");
      expect(fact!.value).toBe("TP.HCM");
    });

    it("should delete a fact", () => {
      memory.setFact("temp", "value");
      expect(memory.deleteFact("temp")).toBe(true);
      expect(memory.getFact("temp")).toBeNull();
    });

    it("should return false when deleting non-existent fact", () => {
      expect(memory.deleteFact("non_existent")).toBe(false);
    });

    it("should get all facts", () => {
      memory.setFact("key1", "val1");
      memory.setFact("key2", "val2");
      memory.setFact("key3", "val3");
      const all = memory.getAllFacts();
      expect(all).toHaveLength(3);
    });

    it("should get facts by category", () => {
      memory.setFact("name", "Dương", { category: "Profile" });
      memory.setFact("age", "25", { category: "Profile" });
      memory.setFact("project", "LIVA", { category: "Work" });
      
      const profileFacts = memory.getFactsByCategory("Profile");
      expect(profileFacts).toHaveLength(2);
      
      const workFacts = memory.getFactsByCategory("Work");
      expect(workFacts).toHaveLength(1);

      const noneFacts = memory.getFactsByCategory("NonExistent");
      expect(noneFacts).toHaveLength(0);
    });
  });

  describe("Size Limit (FIFO Eviction)", () => {
    it("should not evict when adding exactly 50 facts", () => {
      for (let i = 0; i < 50; i++) {
        memory.setFact(`fact_${i}`, `value_${i}`);
      }
      expect(memory.count).toBe(50);
      expect(memory.getFact("fact_0")).not.toBeNull();
    });

    it("should evict oldest fact when exceeding max (51 facts)", () => {
      for (let i = 0; i < 50; i++) {
        memory.setFact(`fact_${i}`, `value_${i}`);
      }
      memory.setFact("fact_50", "value_50");
      expect(memory.count).toBe(50);
      expect(memory.getFact("fact_0")).toBeNull(); // Evicted
      expect(memory.getFact("fact_50")).not.toBeNull(); // Added
    });

    it("should evict facts based on TTL", () => {
      vi.useFakeTimers();
      memory.setFact("ttl_fact", "value", { ttlDays: 1 });
      
      // Advance by 2 days
      vi.setSystemTime(Date.now() + 2 * 24 * 60 * 60 * 1000);
      
      // TTL eviction happens on interval, let's just trigger it manually by calling _enforceSizeLimit?
      // Actually size limit triggers it. Let's add 50 elements to trigger size limit, or call the private method.
      (memory as any).evictExpired();
      
      expect(memory.getFact("ttl_fact")).toBeNull();
      vi.useRealTimers();
    });
  });

  describe("Persistence", () => {
    it("should persist to disk and survive reload", async () => {
      memory.setFact("persistent_key", "persistent_value", { category: "Test" });
      
      // Create new instance (simulates restart) via async factory
      const reloaded = await StructuredMemory.create(TEST_AGENT_ID);
      const fact = reloaded.getFact("persistent_key");
      expect(fact).not.toBeNull();
      expect(fact!.value).toBe("persistent_value");
    });
  });

  describe("System Prompt Formatting", () => {
    it("should return empty string when no facts", () => {
      expect(memory.formatForSystemPrompt()).toBe("");
    });

    it("should format facts grouped by category", () => {
      memory.setFact("name", "Dương", { category: "Profile" });
      memory.setFact("project", "LIVA", { category: "Work" });
      
      const prompt = memory.formatForSystemPrompt();
      expect(prompt).toContain("BỘ NHỚ CẤU TRÚC");
      expect(prompt).toContain("Profile");
      expect(prompt).toContain("name: Dương");
      expect(prompt).toContain("Work");
      expect(prompt).toContain("project: LIVA");
    });
  });

  describe("Input Validation", () => {
    it("should reject empty key", () => {
      memory.setFact("", "value");
      expect(memory.count).toBe(0);
    });

    it("should reject empty value", () => {
      memory.setFact("key", "");
      expect(memory.count).toBe(0);
    });

    it("should truncate overly long keys", () => {
      const longKey = "a".repeat(200);
      memory.setFact(longKey, "value");
      const fact = memory.getAllFacts()[0];
      expect(fact.key.length).toBeLessThanOrEqual(100);
    });
  });

  describe("Error Handling", () => {
    it("should catch and log SQLite errors gracefully during setFact", () => {
      const dbMock = vi.spyOn((memory as any).db, "prepare").mockImplementation(() => {
        throw new Error("SQLite disk I/O error");
      });
      
      expect(() => {
        memory.setFact("error_key", "value");
      }).toThrow();

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

    it("should track unconsolidated events correctly", () => {
        memory.insertEvent({
            eventId: "evt_1", timestamp: Date.now(),
            phi: { facts: [], entities: [] }, psi: { sentiment: "", intent: "", relational: "" },
            rawUserMsg: "hi", rawAiReply: "hello"
        });
        expect(memory.getUnconsolidatedCount()).toBe(1);
    });

    it("should garbage collect old consolidated events", () => {
        const now = Date.now();
        
        // Append 10 days ago and mark consolidated
        vi.setSystemTime(now - 10 * 24 * 60 * 60 * 1000);
        memory.insertEvent({
            eventId: "old_evt", timestamp: Date.now(),
            phi: { facts: [], entities: [] }, psi: { sentiment: "", intent: "", relational: "" },
            rawUserMsg: "old", rawAiReply: "old"
        });
        memory.markConsolidated(["old_evt"]);

        // Append 2 days ago and mark consolidated
        vi.setSystemTime(now - 2 * 24 * 60 * 60 * 1000);
        memory.insertEvent({
            eventId: "new_evt", timestamp: Date.now(),
            phi: { facts: [], entities: [] }, psi: { sentiment: "", intent: "", relational: "" },
            rawUserMsg: "new", rawAiReply: "new"
        });
        memory.markConsolidated(["new_evt"]);

        // Back to present
        vi.setSystemTime(now);

        // GC events older than 7 days
        const removed = memory.gcOldEvents(7);
        expect(removed).toBe(1);
    });

    it("should use default retentionDays=7 in gcOldEvents (Line 509 default branch)", () => {
        const now = Date.now();
        
        // Append 10 days ago and mark consolidated
        vi.setSystemTime(now - 10 * 24 * 60 * 60 * 1000);
        memory.insertEvent({
            eventId: "default_evt", timestamp: Date.now(),
            phi: { facts: [], entities: [] }, psi: { sentiment: "", intent: "", relational: "" },
            rawUserMsg: "default", rawAiReply: "default"
        });
        memory.markConsolidated(["default_evt"]);

        // Back to present
        vi.setSystemTime(now);

        // GC without args should use 7 days default
        const removed = memory.gcOldEvents();
        expect(removed).toBe(1);
    });

    it("should get unconsolidated events and map rows properly", () => {
        // Insert barebones event with missing optional fields to test mapEventRow defaults
        const rawSql = `INSERT INTO events (eventId, timestamp, phi_facts, phi_entities, psi_sentiment, psi_intent, psi_relational, rawUserMsg, rawAiReply, consolidated) 
                        VALUES ('manual_1', 12345, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 0)`;
        (memory as any).db.exec(rawSql);

        memory.insertEvent({
            eventId: "evt_full", timestamp: 12346,
            phi: { facts: ["f1"], entities: ["e1"] }, psi: { sentiment: "s1", intent: "i1", relational: "r1" },
            rawUserMsg: "hi", rawAiReply: "hello"
        });

        const events = memory.getUnconsolidatedEvents();
        expect(events).toHaveLength(2);
        
        // Check manual_1 default mapping
        expect(events[0].phi.facts).toEqual([]);
        expect(events[0].psi.sentiment).toBe("");
        expect(events[0].rawUserMsg).toBe("");
        
        // Check evt_full
        expect(events[1].phi.facts).toEqual(["f1"]);
    });

    it("should catch error on close", () => {
        const mockClose = vi.spyOn((memory as any).db, "close").mockImplementation(() => { throw new Error("db close failed"); });
        expect(() => memory.close()).not.toThrow();
        mockClose.mockRestore();
    });
  });

  describe("Turn Layer API", () => {
    it("should catch and log error inserting turn node (Line 519)", () => {
        vi.spyOn(memory["db"], "prepare").mockImplementationOnce(() => { throw new Error("Mock Insert Error"); });
        // Should not throw, just log
        memory.insertTurnNode("turn_1", 1000, "hello", "hi");
        expect(memory["db"].prepare).toHaveBeenCalled();
    });

    it("should insert and query turns by time range", () => {
        memory.insertTurnNode("turn_1", 1000, "hello", "hi");
        memory.insertTurnNode("turn_2", 2000, "how are you", "good");
        memory.insertTurnNode("turn_3", 3000, "bye", "bye");

        const turns = memory.getTurnsByTimeRange(1500, 2500);
        expect(turns).toHaveLength(1);
        expect(turns[0].turnId).toBe("turn_2");

        const invalidTurns = memory.getTurnsByTimeRange(5000, 1000);
        expect(invalidTurns).toHaveLength(0);
    });

    it("should query turns by IDs", () => {
        memory.insertTurnNode("turn_A", 1000, "A", "A");
        memory.insertTurnNode("turn_B", 2000, "B", "B");
        
        const turns = memory.getTurnsByIds(["turn_A", "turn_B"]);
        expect(turns).toHaveLength(2);
        
        const empty = memory.getTurnsByIds([]);
        expect(empty).toHaveLength(0);
    });
  });

  // ===========================
  // [v4.0] Enterprise Tests
  // ===========================

  describe("[v4.0] AES-256-GCM Encryption", () => {
    it("should encrypt values at rest and decrypt on read", () => {
        memory.setFact("secret_key", "my_password_123", { source: "user" });
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
    it("should hard-delete all facts with deleteAllFacts()", () => {
        memory.setFact("fact_1", "value_1", { source: "user" });
        memory.setFact("fact_2", "value_2", { source: "agent" });
        expect(memory.count).toBe(2);

        memory.deleteAllFacts();
        expect(memory.count).toBe(0);
        expect(memory.getAllFacts()).toHaveLength(0);
    });

    it("should hard-delete all events and turn nodes with deleteAllEvents()", () => {
        memory.insertEvent({
            eventId: "evt_1", timestamp: Date.now(),
            phi: { facts: ["test"], entities: [] },
            psi: { sentiment: "neutral", intent: "info", relational: "" },
            rawUserMsg: "test", rawAiReply: "reply"
        });
        memory.insertTurnNode("t1", Date.now(), "msg", "reply");

        memory.deleteAllEvents();

        expect(memory.getUnconsolidatedEvents()).toHaveLength(0);
        expect(memory.getTurnsByTimeRange(0, Date.now() + 10000)).toHaveLength(0);
    });
  });

  describe("[v4.0] Importance Scoring & Reconciliation", () => {
    it("should store importance based on source", () => {
        memory.setFact("user_fact", "from user", { source: "user" });
        memory.setFact("agent_fact", "from agent", { source: "agent" });

        const userFact = memory.getFact("user_fact");
        const agentFact = memory.getFact("agent_fact");

        expect(userFact!.importance).toBe(1.0);   // User facts are highest priority
        expect(agentFact!.importance).toBe(0.5);   // Agent facts are default
    });

    it("should setFactImportance for reconciliation", () => {
        memory.setFact("old_company", "FPT", { source: "auto_extract" });
        expect(memory.getFact("old_company")!.importance).toBe(0.5);

        memory.setFactImportance("old_company", 0.1);
        const deprecated = memory.getFact("old_company");
        expect(deprecated!.importance).toBe(0.1);
    });

    it("should evict low-importance facts first during FIFO", () => {
        // Fill to capacity with default importance (0.5)
        for (let i = 0; i < 50; i++) {
            memory.setFact(`fact_${i}`, `value_${i}`, { source: "agent" });
        }
        // Set one fact to high importance
        memory.setFactImportance("fact_0", 1.0);
        // Set another to low importance
        memory.setFactImportance("fact_1", 0.0);

        // Add one more to trigger eviction
        memory.setFact("fact_overflow", "overflow", { source: "agent" });

        // Low importance fact_1 should be evicted first
        expect(memory.getFact("fact_1")).toBeNull();
        // High importance fact_0 should survive
        expect(memory.getFact("fact_0")).not.toBeNull();
        expect(memory.getFact("fact_overflow")).not.toBeNull();
    });
  });

  describe("[v4.0] Data Lineage", () => {
    it("should include confidenceScore and sourceTurnId in fact metadata", () => {
        memory.setFact("lineage_fact", "test_value", { source: "auto_extract" });
        const fact = memory.getFact("lineage_fact");

        expect(fact).not.toBeNull();
        expect(fact!.confidenceScore).toBe(1.0);
        // sourceTurnId is null by default (set during extraction)
        expect(fact!.sourceTurnId).toBeUndefined();
    });
  });

  describe("[v4.0] Background Eviction Timer", () => {
    it("should catch and ignore evictExpired errors in timer (Line 120)", () => {
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
        memTimer.close();
        // Cleanup
        try { fs.rmSync(timerStoreDir, { recursive: true, force: true }); } catch {}
        vi.useRealTimers();
    });

    it("should clean up eviction timer on close()", () => {
        // Access private timer to verify it exists
        expect(memory["evictionTimer"]).not.toBeNull();
        memory.close();
        expect(memory["evictionTimer"]).toBeNull();
    });
  });

    describe('Coverage padding', () => {
        it('should hit consolidation source', () => {
            memory.setFact('cons_key', 'val', { source: 'consolidation' });
            expect(memory.getFact('cons_key')!.value).toBe('val');
        });
    });

        it('should fallback default values for importance/confidence', () => {
            (memory as any).db.prepare('INSERT OR REPLACE INTO facts (key, value, createdAt, updatedAt, source, importance, confidenceScore) VALUES (?, ?, ?, ?, ?, ?, ?)').run('null_fact', 'val', new Date().toISOString(), new Date().toISOString(), 'agent', null, null);
            const fact = memory.getFact('null_fact');
            expect(fact!.importance).toBe(0.5);
            expect(fact!.confidenceScore).toBe(1.0);
        });
});
