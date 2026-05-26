import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { MemoryManager } from "../../src/MemoryManager";
import * as fs from "node:fs";
import * as path from "node:path";

const TEST_AGENT_ID = "__brutal_stress_test__";
const TEST_BASE_DIR = path.join(process.cwd(), "data", "agents", TEST_AGENT_ID);

// Mock OpenAI
const mockAiClient = {
    chat: {
        completions: {
            create: vi.fn().mockImplementation(async () => {
                // Simulate Jitter/Delay from 100ms to 300ms
                const delay = Math.random() * 200 + 100;
                await new Promise(r => setTimeout(r, delay));
                
                // 5% chance of LLM Timeout
                if (Math.random() < 0.05) {
                    throw new Error("Simulated LLM Timeout Error (Chaos Testing)");
                }
                
                return {
                    choices: [{
                        message: {
                            content: JSON.stringify({
                                narrative_summary: "A chaotic simulation of stress.",
                                new_user_insights: [],
                                graph_nodes: [],
                                graph_edges: []
                            })
                        }
                    }]
                };
            })
        }
    }
} as any;

async function cleanDirectoryWithRetry(dirPath: string, retries = 5, delay = 50) {
    for (let i = 0; i < retries; i++) {
        try {
            if (fs.existsSync(dirPath)) {
                fs.rmSync(dirPath, { recursive: true, force: true });
            }
            return;
        } catch (e) {
            if (i === retries - 1) throw e;
            await new Promise(r => setTimeout(r, delay));
        }
    }
}

describe("Chaos Testing: Brutal Memory Architecture Stress Test", () => {
    let memoryManager: MemoryManager;

    beforeEach(async () => {
        try {
            await cleanDirectoryWithRetry(TEST_BASE_DIR);
        } catch {}

        memoryManager = new MemoryManager(TEST_AGENT_ID);
        await memoryManager.initialize();
        await memoryManager.initUHM(mockAiClient);
    });

    afterEach(async () => {
        await memoryManager.dispose();
        try {
            await cleanDirectoryWithRetry(TEST_BASE_DIR);
        } catch {}
        vi.restoreAllMocks();
    });

    it("TC-C1: Concurrency Write Bomb (Race Conditions / SQLITE_BUSY)", async () => {
        // Bắn 50 requests addMessage cùng lúc
        const concurrency = 50;
        const promises = [];

        for (let i = 0; i < concurrency; i++) {
            promises.push(
                memoryManager.addMessage(
                    "user",
                    `Chaos message ${i}: LIVA architecture must survive.`,
                    { category: "Chaos", domain: "System" }
                )
            );
        }

        // Must resolve without SQLITE_BUSY or queue deadlocks
        await expect(Promise.all(promises)).resolves.not.toThrow();

        // Check if memCache has them
        const historyLength = (memoryManager as any).memCache.length;
        expect(historyLength).toBe(concurrency);
    }, 30000);

    it("TC-C2: Overlapping Consolidation (Deadlock/Busy test)", async () => {
        // Vừa ghi dữ liệu liên tục vừa ép chạy Consolidation
        
        // 1. Ghi nền liên tục
        let stopWriting = false;
        const backgroundWrites = async () => {
            let i = 0;
            while (!stopWriting) {
                await memoryManager.addMessage("user", `Background write ${i}`);
                i++;
                await new Promise(r => setTimeout(r, 10)); // small delay
            }
        };

        const bgPromise = backgroundWrites();

        // 2. Ép tạo đủ events để Consolidation kích hoạt
        for (let i = 0; i < 15; i++) {
            await memoryManager.addMessage("user", `Trigger event ${i}`);
        }

        // 3. Chạy Consolidation ngầm (Sẽ trigger Step execution và transactions)
        // Dùng force = true để ép chạy không cần check Pin (battery)
        const consolidationPromise = memoryManager.consolidationCron!.consolidateNow(true);

        // 4. Chờ consolidation xong
        await expect(consolidationPromise).resolves.not.toThrow();
        
        stopWriting = true;
        await bgPromise;

        // DB không bị lỗi khóa Partial State
        const events = await (memoryManager as any).structuredMemory.getUnconsolidatedEvents();
        expect(events).toBeDefined();
    }, 45000);

    it("TC-C3: Aggressive Touch & Decay Pagination Test", async () => {
        const sm = (memoryManager as any).structuredMemory;
        const repo = (sm as any).vectorRepo;

        // Bơm 2500 facts giả vào SQLite để test Pagination (Chunk size 1000)
        const factParamSets = [];
        const now = Date.now();
        const tenDaysMs = 10 * 24 * 60 * 60 * 1000;
        const past = new Date(now - tenDaysMs).toISOString();
        
        for (let i = 0; i < 2500; i++) {
            factParamSets.push([`chaos_fact_${i}`, `value_${i}`, past, past, 0, "StressTest", "Test", 1.0, now - tenDaysMs, i % 5]);
        }
        await sm.dbBridge.runBatch(
            "INSERT OR REPLACE INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category, memory_strength, last_accessed_at, access_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            factParamSets
        );

        // Bơm 1200 vectors giả vào SQLite để test Pagination (Chunk size 500)
        const vParamSets = [];
        for (let i = 0; i < 1200; i++) {
            vParamSets.push([`chaos_vec_${i}`, `content_${i}`, now - tenDaysMs, now - tenDaysMs, i % 3]);
        }
        await sm.dbBridge.runBatch(
            "INSERT OR REPLACE INTO vectors_meta (vec_id, type, content, domain, category, created_at, last_accessed_at, decay_weight, access_count) VALUES (?, 'AXIOM', ?, 'General', 'Test', ?, ?, 1.0, ?)",
            vParamSets
        );

        // Chạy applyMemoryDecay (Sẽ dùng pagination chunking xử lý 2500 facts và 1200 vectors)
        const result = await sm.applyMemoryDecay(0.1);

        // Đảm bảo không bị OOM, decayed counts phải khớp
        expect(result.decayed).toBeGreaterThan(0);
        
        // Kiểm tra xem facts đã bị cập nhật (memory_strength giảm)
        const sampleFact = sm.db.prepare("SELECT memory_strength FROM facts WHERE key = 'chaos_fact_0'").get();
        expect(sampleFact.memory_strength).toBeLessThan(1.0);
    }, 30000);
});
