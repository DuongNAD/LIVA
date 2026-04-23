import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { StructuredMemory } from "../../src/memory/StructuredMemory";
import * as fs from "fs";
import * as path from "path";

// Use a temporary agent ID to avoid polluting real data
const TEST_AGENT_ID = "__test_structured_memory__";
const TEST_STORE_PATH = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID, "structured_memory.sqlite");
const TEST_STORE_PATH_JSON = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID, "structured_memory.json");

describe("StructuredMemory", () => {
  let memory: StructuredMemory;

  beforeEach(() => {
    // Clean up any previous test data
    try {
      if (fs.existsSync(TEST_STORE_PATH)) fs.unlinkSync(TEST_STORE_PATH);
      if (fs.existsSync(TEST_STORE_PATH_JSON)) fs.unlinkSync(TEST_STORE_PATH_JSON);
      if (fs.existsSync(TEST_STORE_PATH_JSON + ".bak")) fs.unlinkSync(TEST_STORE_PATH_JSON + ".bak");
    } catch {}
    memory = new StructuredMemory(TEST_AGENT_ID);
    // Explicitly delete all rows from facts for good measure because DatabaseSync could cache
    // Actually the DatabaseSync reconnects so it's fine, but let's clear it just in case
    // Wait, the file is unlinked so it will be newly recreated exactly.
    memory["db"].exec("DELETE FROM facts;");
  });

  afterEach(() => {
    // Cleanup
    try {
      if (fs.existsSync(TEST_STORE_PATH)) fs.unlinkSync(TEST_STORE_PATH);
      if (fs.existsSync(TEST_STORE_PATH_JSON)) fs.unlinkSync(TEST_STORE_PATH_JSON);
      if (fs.existsSync(TEST_STORE_PATH_JSON + ".bak")) fs.unlinkSync(TEST_STORE_PATH_JSON + ".bak");
      const dir = path.dirname(TEST_STORE_PATH);
      if (fs.existsSync(dir)) fs.rmSync(dir, { recursive: true, force: true });
    } catch {}
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
    });
  });

  describe("Size Limit (FIFO Eviction)", () => {
    it("should evict oldest fact when exceeding max", () => {
      // Fill to max (50)
      for (let i = 0; i < 50; i++) {
        memory.setFact(`fact_${i}`, `value_${i}`);
      }
      expect(memory.count).toBe(50);

      // Add one more — should evict fact_0
      memory.setFact("fact_50", "value_50");
      expect(memory.count).toBe(50);
      expect(memory.getFact("fact_0")).toBeNull(); // Evicted
      expect(memory.getFact("fact_50")).not.toBeNull(); // Added
    });
  });

  describe("Persistence", () => {
    it("should persist to disk and survive reload", () => {
      memory.setFact("persistent_key", "persistent_value", { category: "Test" });
      
      // Create new instance (simulates restart)
      const reloaded = new StructuredMemory(TEST_AGENT_ID);
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
});
