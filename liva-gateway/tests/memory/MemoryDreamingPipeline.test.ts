import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryDreamingPipeline, type MemoryNode } from "../../src/memory/MemoryDreamingPipeline";
import * as fs from "node:fs/promises";
import * as path from "node:path";

describe("MemoryDreamingPipeline", () => {
    const testAgentId = "test-agent-dreaming";
    const testStoreDir = path.join(process.cwd(), "data", "agents", testAgentId, "memory_store");
    let pipeline: MemoryDreamingPipeline;

    beforeEach(async () => {
        // Clean up store directory before test
        try {
            await fs.rm(path.join(process.cwd(), "data", "agents", testAgentId), { recursive: true, force: true });
        } catch {
            // ignore
        }
        pipeline = new MemoryDreamingPipeline(testAgentId);
    });

    afterEach(async () => {
        pipeline.dispose();
        // Clean up files after test
        try {
            await fs.rm(path.join(process.cwd(), "data", "agents", testAgentId), { recursive: true, force: true });
        } catch {
            // ignore
        }
    });

    it("should bootstrap correctly and create files", async () => {
        await pipeline.bootstrap();
        
        const dirStat = await fs.stat(testStoreDir);
        expect(dirStat.isDirectory()).toBe(true);

        const indexFile = path.join(testStoreDir, "index_summary.json");
        const fileContent = await fs.readFile(indexFile, "utf-8");
        expect(fileContent).toBe("[]");
    });

    it("should prevent directory traversal attacks", () => {
        expect(() => {
            new MemoryDreamingPipeline(testAgentId, "../../../outside");
        }).toThrow("Directory traversal detected");
    });

    it("should append session logs correctly", async () => {
        await pipeline.appendSessionLog("user", "Hello first");
        await pipeline.appendSessionLog("assistant", "Hi there");

        const logFile = path.join(testStoreDir, "session_logs.jsonl");
        const logsContent = await fs.readFile(logFile, "utf-8");
        const lines = logsContent.trim().split("\n");
        expect(lines.length).toBe(2);

        const parsed1 = JSON.parse(lines[0]!);
        expect(parsed1.role).toBe("user");
        expect(parsed1.content).toBe("Hello first");
        expect(parsed1.timestamp).toBeTypeOf("number");

        const parsed2 = JSON.parse(lines[1]!);
        expect(parsed2.role).toBe("assistant");
        expect(parsed2.content).toBe("Hi there");
    });

    it("should handle empty state in executeDreamingSequence", async () => {
        const result = await pipeline.executeDreamingSequence();
        expect(result).toBeNull();
    });

    it("should execute dreaming sequence: dedup, weight, sort, compression ratio", async () => {
        // Log duplicate messages
        await pipeline.appendSessionLog("user", "Hello task");
        await pipeline.appendSessionLog("assistant", "Doing task");
        await pipeline.appendSessionLog("user", "Hello task"); // Duplicate
        await pipeline.appendSessionLog("user", "Unique node");

        const result = await pipeline.executeDreamingSequence();
        expect(result).not.toBeNull();
        if (!result) return;

        expect(result.originalSizeBytes).toBeGreaterThan(0);
        expect(result.optimizedSizeBytes).toBeGreaterThan(0);
        expect(result.compressionRatio).toBeLessThan(1);
        expect(result.proposedIndex.length).toBe(3); // Hello task, Doing task, Unique node

        // Check deduplication and weights
        const helloNode = result.proposedIndex.find(n => n.content === "Hello task");
        expect(helloNode).toBeDefined();
        expect(helloNode!.weight).toBe(2);

        const uniqueNode = result.proposedIndex.find(n => n.content === "Unique node");
        expect(uniqueNode).toBeDefined();
        expect(uniqueNode!.weight).toBe(1);

        // Check sorting: Hello task should be first because it has weight 2, and others have weight 1
        expect(result.proposedIndex[0]!.content).toBe("Hello task");

        // Verify diff payload has added items
        expect(result.diffPayload).toContain("+ [ADDED]");
        expect(result.diffPayload).toContain("Hello task");
        expect(result.diffPayload).toContain("Unique node");
    });

    it("should prevent concurrent dreaming execution", async () => {
        await pipeline.appendSessionLog("user", "Some log");
        
        // Mock fs.readFile to take a bit longer or execute simultaneously
        const promise1 = pipeline.executeDreamingSequence();
        
        // Call it again immediately
        await expect(pipeline.executeDreamingSequence()).rejects.toThrow("Dreaming sequence already in progress");
        
        const result = await promise1;
        expect(result).not.toBeNull();
    });

    it("should generate proper diff for modified and removed items", async () => {
        // First commit: initial memory with real SHA-256 hashes
        const { createHash } = await import("node:crypto");
        const hash1 = createHash("sha256").update("Memory 1").digest("hex");
        const hash2 = createHash("sha256").update("Memory 2").digest("hex");

        const node1: MemoryNode = { id: "1", hash: hash1, content: "Memory 1", weight: 1, lastAccessed: Date.now() };
        const node2: MemoryNode = { id: "2", hash: hash2, content: "Memory 2", weight: 1, lastAccessed: Date.now() };
        await pipeline.commitApprovedMemory([node1, node2]);

        // Add logs:
        // We log "Memory 1" again (modifying weight to 2)
        // We log a new "Memory 3" (adding it)
        // We don't log "Memory 2" (weight stays 1)
        await pipeline.appendSessionLog("user", "Memory 1");
        await pipeline.appendSessionLog("user", "Memory 3");

        const result = await pipeline.executeDreamingSequence();
        expect(result).not.toBeNull();
        if (!result) return;

        expect(result.proposedIndex.length).toBe(3); // Memory 1 (w=2), Memory 2 (w=1), Memory 3 (w=1)
        
        // Check modified weight
        const m1 = result.proposedIndex.find(n => n.content === "Memory 1");
        expect(m1!.weight).toBe(2);

        // Check diff
        expect(result.diffPayload).toContain("~ [MODIFIED] ID: 1");
        expect(result.diffPayload).toContain("- Weight: 1");
        expect(result.diffPayload).toContain("+ Weight: 2");
        expect(result.diffPayload).toContain("+ [ADDED]");
        expect(result.diffPayload).toContain("Memory 3");

        // Now test removal by generating direct diff
        const oldIndex = [node1, node2];
        const newIndex = [node1]; // node2 removed
        const diff = pipeline.generateDiffPayload(oldIndex, newIndex);
        expect(diff).toContain("- [REMOVED] ID: 2");
        expect(diff).toContain("- Memory 2");
    });

    it("should commit approved memory and purge logs", async () => {
        await pipeline.appendSessionLog("user", "Test commit");
        const result = await pipeline.executeDreamingSequence();
        expect(result).not.toBeNull();
        if (!result) return;

        await pipeline.commitApprovedMemory(result.proposedIndex);

        // Verify index is written
        const indexList = await pipeline.loadIndex();
        expect(indexList.length).toBe(1);
        expect(indexList[0]!.content).toBe("Test commit");

        // Verify logs are purged/truncated
        const logFile = path.join(testStoreDir, "session_logs.jsonl");
        const logStat = await fs.stat(logFile);
        expect(logStat.size).toBe(0);
    });

    it("should fail validation and keep index untouched if invalid index is committed", async () => {
        const invalidIndex = [{ wrong: "schema" }];
        await expect(pipeline.commitApprovedMemory(invalidIndex as any)).rejects.toThrow();
    });

    it("should clear cache on dispose", async () => {
        await pipeline.bootstrap();
        const indexList = await pipeline.loadIndex();
        expect(indexList).toEqual([]);

        // Cache should be set
        pipeline.dispose();
        // Index is cleared, next load will read disk again
        const indexList2 = await pipeline.loadIndex();
        expect(indexList2).toEqual([]);
    });
});
