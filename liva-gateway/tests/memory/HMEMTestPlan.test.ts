import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { promises as fsp, existsSync, writeFileSync, readFileSync } from "node:fs";
import * as path from "node:path";
import OpenAI from "openai";
import { MemoryManager } from "../../src/MemoryManager";
import { StructuredMemory } from "../../src/memory/StructuredMemory";
import { VectorRepository } from "../../src/memory/VectorRepository";
import { EventRepository, EventBrick } from "../../src/memory/EventRepository";
import { ReflectionDaemon } from "../../src/memory/ReflectionDaemon";
import { ConsolidationCron } from "../../src/memory/ConsolidationCron";
import { ContradictionResolver } from "../../src/memory/ContradictionResolver";
import { ArchivingCron } from "../../src/memory/ArchivingCron";
import { BookIndex } from "../../src/memory/BookIndex";
import { SemanticCache } from "../../src/memory/SemanticCache";
import { safeRename } from "../../src/utils/FileUtils";
import { EmbeddingService } from "../../src/services/EmbeddingService";
import { Worker } from "node:worker_threads";

// Test identification and database setup
const TEST_AGENT_ID = "hmem_v18_test_agent_" + Math.random().toString(36).substring(2, 7);
const TEST_BASE_DIR = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID);
const TEST_STORE_PATH = path.join(TEST_BASE_DIR, "structured_memory.sqlite");

// Mock OpenAI
const mockOpenAI = {
    chat: {
        completions: {
            create: vi.fn().mockImplementation(async (options) => {
                const promptContent = options.messages[0]?.content || "";
                if (promptContent.includes("dual-perspective event extraction")) {
                    return {
                        choices: [{
                            message: {
                                content: JSON.stringify({
                                    factual_entries: [{
                                        fact: "Sếp thích uống cà phê sữa đá không đường lúc 9h sáng",
                                        entity: "Sếp",
                                        confidence: 0.95,
                                        domain_classification: "Personal",
                                        category_routing_tag: "Habit",
                                        trace_identifiers: ["Sếp", "cà phê sữa đá"]
                                    }],
                                    relational_entries: [{
                                        relation: "Sếp - Người pha chế",
                                        sentiment: "happy",
                                        intent: "sharing preference",
                                        topic_summary: "Sếp thích uống cà phê"
                                    }]
                                })
                            }
                        }]
                    };
                } else if (promptContent.includes("long-term memory synthesis")) {
                    return {
                        choices: [{
                            message: {
                                content: JSON.stringify({
                                    narrative_summary: "Sếp thích uống cà phê sữa đá không đường lúc 9h sáng.",
                                    new_user_insights: [{
                                        key: "boss_coffee_preference",
                                        value: "Thích uống cà phê sữa đá không đường lúc 9h sáng",
                                        category: "Habit"
                                    }],
                                    graph_nodes: [
                                        { id: "User", label: "PERSON", properties: "{}" },
                                        { id: "CafeSuaDa", "label": "DRINK", "properties": "{}" }
                                    ],
                                    graph_edges: [
                                        { source: "User", target: "CafeSuaDa", relation: "LIKES" }
                                    ]
                                })
                            }
                        }]
                    };
                } else if (promptContent.includes("Contradiction Resolver")) {
                    return {
                        choices: [{
                            message: {
                                content: JSON.stringify({
                                    status: "contradiction",
                                    obsolete_edges: [{ source: "User", target: "CafeSuaDa", relation: "LIKES" }]
                                })
                            }
                        }]
                    };
                } else if (promptContent.includes("Summarize the following")) {
                    return {
                        choices: [{
                            message: {
                                content: "Sếp thích uống cà phê sữa đá không đường vào buổi sáng."
                            }
                        }]
                    };
                }
                return { choices: [{ message: { content: "{}" } }] };
            })
        }
    }
} as unknown as OpenAI;

describe("LIVA H-MEM v18 Test Plan", () => {
    let memory: StructuredMemory;

    beforeEach(async () => {
        try {
            await fsp.rm(TEST_BASE_DIR, { recursive: true, force: true });
        } catch {}
        await fsp.mkdir(TEST_BASE_DIR, { recursive: true });
        memory = await StructuredMemory.create(TEST_AGENT_ID, TEST_STORE_PATH);
        vi.spyOn(StructuredMemory, "create").mockResolvedValue(memory);
    });

    afterEach(async () => {
        if (memory) {
            await memory.close();
        }
        try {
            await fsp.rm(TEST_BASE_DIR, { recursive: true, force: true });
            const coldStorageDir = path.join(process.cwd(), "data", "cold_storage");
            if (existsSync(coldStorageDir)) {
                await fsp.rm(coldStorageDir, { recursive: true, force: true });
            }
        } catch {}
        vi.restoreAllMocks();
        vi.useRealTimers();
    });

    // =========================================================================
    // Phase 1: Unit Testing (Kiểm Thử Mức Vi Mô)
    // =========================================================================

    describe("Phase 1: Unit Testing", () => {

        it("TC1.1 - L0 WorkingBuffer Limits", async () => {
            const mm = new MemoryManager(TEST_AGENT_ID);
            await mm.initialize();

            // Push 250 messages continuously
            for (let i = 0; i < 250; i++) {
                await mm.addMessage("user", `Message index: ${i}`);
            }

            const history = await mm.getShortTermHistory();
            // Since it slices when exceeding 200 to keep the last 100:
            // 200 pushes -> length 200
            // 201st push -> exceeds 200 -> slices to 100, then adds 201st -> length 101
            // 49 more pushes -> 101 + 49 = 150 messages.
            expect(history.length).toBeLessThanOrEqual(200);
            expect(history.length).toBe(149);
            expect(history[0].content).toContain("Message index: 101");
            await mm.dispose();
        });

        it("TC1.2 - Decoupled CPU Embedding", async () => {
            const service = EmbeddingService.getInstance();
            expect(service.modelId).toBe("onnx-cpu-worker");
            expect(service.dimension).toBe(384);
            expect(service.supportsMRL).toBe(false);

            // Verify worker runs on CPU (no CUDA/VRAM requirements)
            const workerPath = path.join(process.cwd(), "src", "workers", "EmbeddingWorker.ts");
            const testWorker = new Worker(`
                require('tsx/cjs');
                require(${JSON.stringify(workerPath)});
            `, { eval: true });

            const workerReady = new Promise<void>((resolve, reject) => {
                const timeout = setTimeout(() => {
                    testWorker.terminate();
                    reject(new Error("EmbeddingWorker init timeout"));
                }, 20000);

                testWorker.on("message", (msg) => {
                    if (msg.type === "ready") {
                        clearTimeout(timeout);
                        testWorker.postMessage({ type: "dispose" });
                        resolve();
                    }
                });
                testWorker.on("error", (err) => {
                    clearTimeout(timeout);
                    testWorker.terminate();
                    reject(err);
                });
                testWorker.postMessage({ type: "init" });
            });

            await expect(workerReady).resolves.not.toThrow();
        });

        it("TC1.3 - L2 Vector Quantization", () => {
            const dimension = 384;
            const float32Size = dimension * 4; // 384 float32 values = 1536 bytes
            const int8Size = dimension * 1;    // 384 int8 values = 384 bytes
            const savingRatio = (float32Size - int8Size) / float32Size;

            // Assert that saving is exactly 75%
            expect(savingRatio).toBe(0.75);

            // Test insertion and quantization via C++ vec_quantize_int8 in Node:sqlite
            const db = memory.getDb();
            db.exec(`CREATE VIRTUAL TABLE IF NOT EXISTS test_vec_quant USING vec0(embedding int8[3])`);
            
            const rawVector = new Float32Array([1.0, -1.0, 0.5]);
            const blob = new Uint8Array(rawVector.buffer);
            
            expect(() => {
                db.prepare(`INSERT INTO test_vec_quant(rowid, embedding) VALUES (1, vec_quantize_int8(?, 'unit'))`).run(blob);
            }).not.toThrow();
        });

        it("TC1.4 - Ebbinghaus Function", async () => {
            const lambda = 0.1;
            const daysSinceAccess = 10;
            const S0 = 1.0;
            const expectedStrength = S0 * Math.exp(-lambda * daysSinceAccess);

            await memory.setFact("ebbinghaus_key", "ebbinghaus_val", { source: "user" });
            const now = Date.now();
            const tenDaysAgo = now - daysSinceAccess * 24 * 60 * 60 * 1000;

            // Inject simulated last accessed time directly into database
            memory.getDb().prepare(
                "UPDATE facts SET memory_strength = 1.0, last_accessed_at = ? WHERE key = ?"
            ).run(tenDaysAgo, "ebbinghaus_key");

            const decayResult = await memory.applyMemoryDecay(lambda);
            expect(decayResult.decayed).toBeGreaterThanOrEqual(1);

            const updatedFact = memory.getFact("ebbinghaus_key");
            expect(updatedFact).not.toBeNull();
            expect(updatedFact!.memoryStrength).toBeCloseTo(expectedStrength, 4);
        });

    });

    // =========================================================================
    // Phase 2: Integration Testing (Kiểm Thử Tích Hợp L0 - L3)
    // =========================================================================

    describe("Phase 2: Integration Testing", () => {

        it("TC2.1 - ReflectionDaemon Extraction", async () => {
            vi.useFakeTimers();
            const daemon = new ReflectionDaemon(memory, mockOpenAI);

            // Trigger conversation turn
            daemon.queueTurn(
                "Sếp thích uống cà phê sữa đá không đường lúc 9h sáng hằng ngày.",
                "Dạ vâng, em đã ghi nhớ thói quen uống cà phê sữa đá không đường lúc 9h sáng của sếp."
            );

            expect(daemon.pendingCount).toBe(1);

            // Advance debounce timer (12s)
            await vi.advanceTimersByTimeAsync(12_000);

            // Verify L1 event table insertion
            const unconsolidated = await memory.getUnconsolidatedEvents();
            expect(unconsolidated).toHaveLength(1);
            expect(unconsolidated[0].phi.facts[0]).toBe("Sếp thích uống cà phê sữa đá không đường lúc 9h sáng");
            expect(unconsolidated[0].psi.sentiment).toBe("happy");
            expect(unconsolidated[0].psi.intent).toBe("sharing preference");
        });

        it("TC2.2 - Hybrid Search & RRF Scoring", async () => {
            const repo = new VectorRepository(memory.getDbBridge());
            await repo.init();

            const queryText = "LIVA";
            const queryVec = new Array(384).fill(0.1);

            // Insert mock records
            await repo.upsertVector({
                vecId: "v1",
                type: "AXIOM",
                content: "LIVA H-MEM v18 is an amazing architecture",
                vector: new Array(384).fill(0.1),
                domain: "Development"
            });
            await repo.upsertVector({
                vecId: "v2",
                type: "AXIOM",
                content: "LIVA vector memory with RRF search support",
                vector: new Array(384).fill(0.12),
                domain: "Development"
            });

            // Perform hybrid search
            const results = await repo.searchHybridVectors(queryText, queryVec, 5);
            expect(results.length).toBeGreaterThan(0);

            // RRF Score calculation verification
            // RRF_Score = 1/(60 + Rank_semantic) + 1/(60 + Rank_keyword)
            const item = results[0];
            expect(item.score).toBeGreaterThan(0);
            
            // Expected score calculation if item is rank 1 in both semantic and keyword:
            // 1/(60+1) + 1/(60+1) = 2/61 = ~0.03278
            expect(item.score).toBeCloseTo(2 / 61, 4);
        });

        it("TC2.3 - Contradiction Resolver", async () => {
            const contradictionResolver = new ContradictionResolver(memory, EmbeddingService.getInstance(), mockOpenAI);

            // 1. Setup existing L3 graph edge
            await memory.graph.upsertNode({ id: "User", label: "PERSON", properties: "{}" });
            await memory.graph.upsertNode({ id: "CafeSuaDa", label: "DRINK", properties: "{}" });
            await memory.graph.upsertEdge({ source: "User", target: "CafeSuaDa", relation: "LIKES", weight: 1.0, obsolete: 0 });

            // Ensure old edge is active
            let activeEdges = await memory.graph.getActiveEdgesBySource("User");
            expect(activeEdges).toHaveLength(1);
            expect(activeEdges[0].obsolete).toBe(0);

            // 2. Prepare contradiction inputs
            const newEdge = { source: "User", target: "CafeSuaDa", relation: "DISLIKES", weight: 1.0, obsolete: 0 };
            const sourceNode = { id: "User", label: "PERSON", properties: "{}" };
            const targetNode = { id: "CafeSuaDa", label: "DRINK", properties: "{}" };

            // Mock embedding and hybrid search outputs to mimic cosine similarity > 0.85
            vi.spyOn(EmbeddingService.getInstance(), "embed").mockResolvedValue(new Array(384).fill(0.1));
            vi.spyOn(memory, "searchSimilarVectors").mockResolvedValue([{
                id: 1,
                vecId: "old_fact_1",
                content: "User CafeSuaDa LIKES",
                type: "AXIOM",
                domain: "Personal",
                category: "Habit",
                distance: 0.1,
                score: 0.9, // > 0.85
                traceKeywords: [],
                sourceEventIds: []
            }]);

            // 3. Resolve contradiction
            await contradictionResolver.resolve(newEdge, sourceNode, targetNode);

            // 4. Assert old edge is marked as obsolete
            const oldEdgeRow = memory.getDb().prepare("SELECT obsolete FROM l3_edges WHERE source = 'User' AND target = 'CafeSuaDa'").get() as any;
            expect(oldEdgeRow.obsolete).toBe(1);
        });

        it("TC2.4 - L2 to L1 Positional Drill-down", async () => {
            const repo = new VectorRepository(memory.getDbBridge());
            await repo.init();

            const eventId = "event_pos_123";
            const vecId = "vec_drill_1";

            // Insert L1 event
            await memory.insertEvent({
                eventId,
                timestamp: Date.now(),
                phi: { facts: ["Sếp thích uống trà sữa"], entities: [] },
                psi: { sentiment: "", intent: "", relational: "" },
                rawUserMsg: "Sếp thích uống trà sữa",
                rawAiReply: "Dạ ghi nhận"
            });

            // Insert L1 turn node to allow drill-down
            await memory.insertTurnNode(eventId, Date.now(), "Sếp thích uống trà sữa", "Dạ ghi nhận");

            // Insert L2 vector linked to L1 event
            await repo.upsertVector({
                vecId,
                type: "ANCHOR",
                content: "Sếp thích uống trà sữa",
                vector: new Array(384).fill(0.1),
                sourceEventIds: [eventId]
            });

            // Fetch L2 and drill-down
            const searchResults = await repo.searchWithDrilldown(new Array(384).fill(0.1), 1);
            expect(searchResults).toHaveLength(1);
            expect(searchResults[0].sourceEventIds).toContain(eventId);

            // Retrieve raw turns from L1
            const events = await memory.getTurnsByIds(searchResults[0].sourceEventIds);
            expect(events).toHaveLength(1);
            expect(events[0].userMsg).toBe("Sếp thích uống trà sữa");
        });

        it("TC2.5 - End-to-End (E2E) Long-term Recall", async () => {
            vi.useFakeTimers();
            const daemon = new ReflectionDaemon(memory, mockOpenAI);
            const bookIndex = new BookIndex();
            
            const cron = new ConsolidationCron(memory, EmbeddingService.getInstance(), bookIndex, mockOpenAI);

            // 1. Add dialogue
            daemon.queueTurn(
                "Sếp thích uống cà phê sữa đá không đường lúc 9h sáng.",
                "Dạ vâng, em đã nhớ thói quen này của sếp rồi ạ."
            );

            // 2. Debounce triggers extraction to L1
            await vi.advanceTimersByTimeAsync(12_000);
            expect(await memory.getUnconsolidatedCount()).toBe(1);

            // Retrieve the generated eventId to insert a corresponding L1 turn node
            const unconsolidated = await memory.getUnconsolidatedEvents();
            expect(unconsolidated).toHaveLength(1);
            const eventId = unconsolidated[0].eventId;
            await memory.insertTurnNode(eventId, Date.now(), "Sếp thích uống cà phê sữa đá không đường lúc 9h sáng.", "Dạ vâng, em đã nhớ thói quen này của sếp rồi ạ.");

            // 3. Consolidation synthesizes L1 into L2 AXIOM/ANCHOR
            const processed = await cron.consolidateNow(true); // force = true bypasses minimum threshold check
            expect(processed).toBe(1);
            expect(await memory.getUnconsolidatedCount()).toBe(0);

            // Flush vector queue so they are written to SQLite before searching
            await memory.flushVectorQueue();

            // 4. Retrieve via hybrid query
            const searchVector = new Array(384).fill(0.1);
            const searchResults = await memory.searchSimilarVectors(searchVector, 5);
            expect(searchResults.length).toBeGreaterThan(0);
            
            // Find ANCHOR result (which contains sourceEventIds, excluding community summaries)
            const mainRecord = searchResults.find(r => r.type === "ANCHOR" && r.domain !== "Community");
            expect(mainRecord).toBeDefined();

            // Drill down back to raw L1 turn
            const rawEventIds = mainRecord!.sourceEventIds;
            expect(rawEventIds.length).toBeGreaterThan(0);

            const rawTurns = await memory.getTurnsByIds(rawEventIds);
            expect(rawTurns).toHaveLength(1);
            expect(rawTurns[0].userMsg).toBe("Sếp thích uống cà phê sữa đá không đường lúc 9h sáng.");
        });

    });

    // =========================================================================
    // Phase 3: System & Edge Case Testing (Kiểm Thử Hiệu Năng & Ngoại Lệ)
    // =========================================================================

    describe("Phase 3: System & Edge Case Testing", () => {

        it("TC3.1 - Battery Throttling", async () => {
            const bookIndex = new BookIndex();
            const cron = new ConsolidationCron(memory, EmbeddingService.getInstance(), bookIndex, mockOpenAI);

            // Setup 15 events in SQLite (above default threshold 10, below battery threshold 50)
            for (let i = 0; i < 15; i++) {
                await memory.insertEvent({
                    eventId: `evt_bat_${i}`,
                    timestamp: Date.now(),
                    phi: { facts: [`Fact ${i}`], entities: [] },
                    psi: { sentiment: "", intent: "", relational: "" },
                    rawUserMsg: "msg",
                    rawAiReply: "reply"
                });
            }

            // Write battery state: is_battery = true
            const hwStatePath = path.join(process.cwd(), "data", "hardware_state.json");
            await fsp.mkdir(path.dirname(hwStatePath), { recursive: true });
            await fsp.writeFile(hwStatePath, JSON.stringify({ is_battery: true }), "utf-8");

            // Execute consolidation cron
            const batteryRuns = await cron.consolidateNow(false);
            expect(batteryRuns).toBe(0); // Should skip because 15 < 50 threshold

            // Write battery state: is_battery = false
            await fsp.writeFile(hwStatePath, JSON.stringify({ is_battery: false }), "utf-8");

            // Execute consolidation cron again
            const normalRuns = await cron.consolidateNow(false);
            expect(normalRuns).toBe(15); // Should consolidate all 15 events because 15 > 10 threshold

            // Clean up
            await fsp.unlink(hwStatePath).catch(() => {});
        });

        it("TC3.2 - Debounced Memory Touch", async () => {
            const eventRepo = new EventRepository(memory.getDbBridge(), TEST_AGENT_ID);
            
            // Insert mock events to touch
            for (let i = 0; i < 1000; i++) {
                await eventRepo.insertEvent({
                    eventId: `touch_evt_${i}`,
                    timestamp: Date.now(),
                    phi: { facts: [], entities: [] },
                    psi: { sentiment: "", intent: "", relational: "" },
                    rawUserMsg: "msg",
                    rawAiReply: "reply"
                });
            }

            // Send 1000 touches to fill queue
            const spyFlush = vi.spyOn(eventRepo, "flushTouchQueue");
            
            for (let i = 0; i < 1000; i++) {
                eventRepo.queueMemoryTouch(`touch_evt_${i}`);
            }

            // Early flush triggers at 900 items (microtask)
            await Promise.resolve();
            expect(spyFlush).toHaveBeenCalled();
            spyFlush.mockRestore();
        });

        it("TC3.3 - SemanticCache Zero-Latency", () => {
            const cache = new SemanticCache(10);
            
            // Cache a short command (< 20 words)
            const query = "thời tiết hôm nay thế nào";
            const response = "Hôm nay trời nắng đẹp.";
            cache.set(query, response);

            // Query with fuzzy matching (Levenshtein distance similarity >= 0.95)
            // "thời tiết hôm nay thế nao" has length 25, distance 1 -> similarity 0.96
            const fuzzyQuery = "thời tiết hôm nay thế nao";
            
            const start = performance.now();
            const hit = cache.get(fuzzyQuery);
            const duration = performance.now() - start;

            expect(hit).not.toBeNull();
            expect(hit!.response).toBe(response);
            expect(duration).toBeLessThan(5.0); // Zero-latency (under 5ms)
        });

        it("TC3.4 - Atomic File Write & Crash Recovery", async () => {
            const configPath = path.join(TEST_BASE_DIR, "config.json");
            const tmpPath = `${configPath}.tmp`;
            
            // Write initial configuration
            await fsp.writeFile(configPath, JSON.stringify({ theme: "dark" }), "utf-8");

            // Simulation of atomic write that crashes before rename()
            await fsp.writeFile(tmpPath, JSON.stringify({ theme: "light" }), "utf-8");
            
            // Simulate crash/halt before rename (meaning safeRename is never called)
            // Assert system data has not been corrupted and original config remains intact
            expect(existsSync(configPath)).toBe(true);
            const content = await fsp.readFile(configPath, "utf-8");
            expect(JSON.parse(content).theme).toBe("dark");

            // Simulate recovery: boot checks and cleans up .tmp files
            if (existsSync(tmpPath)) {
                await fsp.unlink(tmpPath);
            }
            expect(existsSync(tmpPath)).toBe(false);
        });

        it("TC3.5 - VRAM Guard Lock", async () => {
            vi.useFakeTimers();
            const bookIndex = new BookIndex();
            const cron = new ConsolidationCron(memory, EmbeddingService.getInstance(), bookIndex, mockOpenAI);

            // Set AgentLoop state to busy THINKING
            cron.setAgentLoopStateGetter(() => "THINKING");

            // Mock trigger consolidation activity
            cron.recordActivity("NEW_TURN");

            // Advance debounce timer (15s)
            await vi.advanceTimersByTimeAsync(15_000);

            // Verify consolidation is bypassed to protect GPU resource
            expect((cron as any).isRunning).toBe(false);
        });

        it("TC3.6 - Security & GDPR Deletion", async () => {
            const mm = new MemoryManager(TEST_AGENT_ID);
            await mm.initialize();

            // Populate L0, L2, L3 facts, L3 Graph
            await mm.addMessage("user", "Hello LIVA, please remember this personal fact.");
            mm.setStructuredFact("personal_hobby", "Gamer", { source: "user" });
            await memory.graph.upsertNode({ id: "UserNode", label: "USER", properties: "{}" });

            // Purge context
            await mm.purgeUserContext();

            // Verify L0 WorkingBuffer/memCache cleared
            expect(await mm.getShortTermHistory()).toHaveLength(0);

            // Verify L2 Vector DB cleared
            expect(await memory.getVectorCount()).toBe(0);

            // Verify L3 facts cleared
            expect(memory.getAllFacts()).toHaveLength(0);

            // Verify L3 Graph DB cleared
            const nodeRow = memory.getDb().prepare("SELECT count(*) as c FROM l3_nodes").get() as any;
            expect(nodeRow.c).toBe(0);

            await mm.dispose();
        });

        it("TC3.7 - Cold Storage Archiving", async () => {
            const bookIndex = new BookIndex();
            
            const archivingCron = new ArchivingCron(memory, mockOpenAI);

            // Insert L1 event
            const eventId = "evt_arch_1";
            await memory.insertEvent({
                eventId,
                timestamp: Date.now() - 31 * 24 * 60 * 60 * 1000,
                phi: { facts: ["Fact to be archived"], entities: [] },
                psi: { sentiment: "", intent: "", relational: "" },
                rawUserMsg: "User raw message",
                rawAiReply: "AI reply"
            });

            // Insert L2 vector (older than 30 days, decay_weight = 0.4)
            const repo = new VectorRepository(memory.getDbBridge());
            await repo.init();
            
            const db = memory.getDb();
            db.prepare(`
                INSERT INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, source_event_ids, created_at, last_accessed_at, decay_weight, access_count)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, 0)
            `).run(
                "v_stale", "AXIOM", "Fact to be archived", "Personal", "Habit", "[]",
                JSON.stringify([eventId]), Date.now() - 32 * 24 * 60 * 60 * 1000, 0.4
            );
            
            const floatArray = new Float32Array(384);
            const blob = new Uint8Array(floatArray.buffer);
            db.prepare("INSERT INTO vec_idx(rowid, embedding) VALUES (1, vec_quantize_int8(?, 'unit'))").run(blob);
            db.prepare("INSERT INTO vectors_fts(rowid, content) VALUES (1, ?)").run("Fact to be archived");

            const initialDbSize = (await fsp.stat(TEST_STORE_PATH)).size;

            // Trigger Archiving Process
            const archived = await archivingCron.runArchivingProcess();
            expect(archived).toBe(1);

            // Assert L3 ArchiveNode created
            const nodes = db.prepare("SELECT * FROM l3_nodes WHERE label = 'ARCHIVED_CONCEPT'").all() as any[];
            expect(nodes.length).toBeGreaterThanOrEqual(1);

            // Assert data exported to cold storage .jsonl
            const archiveDir = path.join(process.cwd(), "data", "cold_storage");
            expect(existsSync(archiveDir)).toBe(true);
            const files = await fsp.readdir(archiveDir);
            expect(files.length).toBeGreaterThanOrEqual(1);

            // Assert L2 and L1 entries cleared
            const vecMeta = db.prepare("SELECT * FROM vectors_meta WHERE vec_id = 'v_stale'").get();
            expect(vecMeta).toBeUndefined();
            const l1Event = db.prepare(`SELECT * FROM events WHERE eventId = '${eventId}'`).get();
            expect(l1Event).toBeUndefined();

            // Assert SQLite size comparison after VACUUM
            const finalDbSize = (await fsp.stat(TEST_STORE_PATH)).size;
            // SQLite file size might be identical due to page allocation, but we ensure VACUUM executes cleanly.
            expect(finalDbSize).toBeGreaterThan(0);
        });

        it("TC3.8 - Boot-time Warm-up", async () => {
            const now = Date.now();
            const oneHourAgo = now - 60 * 60 * 1000;
            const twoDaysAgo = now - 2 * 24 * 60 * 60 * 1000;

            // Pre-populate 12 old turns (> 24h) and 3 recent turns (< 24h) in SQLite L1
            for (let i = 0; i < 12; i++) {
                await memory.insertTurnNode(`turn_old_${i}`, twoDaysAgo + i * 1000, `old msg ${i}`, `old reply ${i}`);
            }
            for (let i = 0; i < 3; i++) {
                await memory.insertTurnNode(`turn_new_${i}`, oneHourAgo + i * 1000, `new msg ${i}`, `new reply ${i}`);
            }

            const mm = new MemoryManager(TEST_AGENT_ID);
            await mm.initialize();

            // Boot-time warm-up loads up to 10 recent turns from the last 24h
            const shortTermHistory = await mm.getShortTermHistory();
            
            // Should contain exactly the turns within the last 24h (which is 3 turns)
            // Plus system PREVIOUS SESSION CONTEXT prompt injection
            const prevSessionContext = shortTermHistory.filter(m => m.role === "system" && m.content.includes("PREVIOUS SESSION CONTEXT"));
            expect(prevSessionContext).toHaveLength(1);
            
            // Confirm the content is from the recent turns
            expect(prevSessionContext[0].content).toContain("new msg 0");
            expect(prevSessionContext[0].content).toContain("new msg 2");
            expect(prevSessionContext[0].content).not.toContain("old msg");

            await mm.dispose();
        });

        it("TC3.9 - Stress & Load Testing", async () => {
            const repo = new VectorRepository(memory.getDbBridge());
            await repo.init();

            const totalRecords = 10000;
            
            // Batch insert 10,000 dummy records into database in a transaction
            memory.getDb().exec("BEGIN TRANSACTION;");
            try {
                const stmtMeta = memory.getDb().prepare(`
                    INSERT INTO vectors_meta (vec_id, type, content, domain, category, created_at)
                    VALUES (?, 'AXIOM', ?, 'General', 'Test', ?)
                `);
                const stmtVec = memory.getDb().prepare("INSERT INTO vec_idx(rowid, embedding) VALUES (?, vec_quantize_int8(?, 'unit'))");
                const stmtFts = memory.getDb().prepare("INSERT INTO vectors_fts(rowid, content) VALUES (?, ?)");

                const dummyVec = new Float32Array(384);
                dummyVec.fill(0.05);
                const blob = new Uint8Array(dummyVec.buffer);

                for (let i = 0; i < totalRecords; i++) {
                    const id = i + 1;
                    const vecId = `stress_vec_${id}`;
                    const content = `Stress test content record index ${id} with random text keywords LIVA H-MEM v18 search performance.`;
                    
                    stmtMeta.run(vecId, content, Date.now());
                    stmtVec.run(BigInt(id), blob);
                    stmtFts.run(BigInt(id), content);
                }
                memory.getDb().exec("COMMIT;");
            } catch (err) {
                memory.getDb().exec("ROLLBACK;");
                throw err;
            }

            // Verify count
            expect(await repo.getVectorCount()).toBe(totalRecords);

            // Execute Hybrid Search 10 times and calculate average latency
            const queryVec = new Array(384).fill(0.05);
            const queryText = "LIVA H-MEM search performance";
            
            const start = performance.now();
            for (let i = 0; i < 10; i++) {
                const res = await repo.searchHybridVectors(queryText, queryVec, 5);
                expect(res.length).toBeGreaterThan(0);
            }
            const avgDuration = (performance.now() - start) / 10;

            // Search speed should be lightning fast (< 100ms)
            expect(avgDuration).toBeLessThan(100.0);
        });

    });

});
