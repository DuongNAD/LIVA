/**
 * MemoryManager.test.ts — Core memory orchestrator tests
 * Tests encryption, session state, message buffering, hybrid context
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as path from "node:path";
import { logger } from "../../src/utils/logger";

vi.mock("node:fs/promises", async () => {
    const memfs = await import("memfs");
    return memfs.fs.promises;
});

vi.mock("node:fs", async () => {
    const memfs = await import("memfs");
    return memfs.fs;
});

vi.mock("fs/promises", async () => {
    const memfs = await import("memfs");
    return memfs.fs.promises;
});

vi.mock("fs", async () => {
    const memfs = await import("memfs");
    return memfs.fs;
});
import { vol } from "memfs";

vi.mock("node:sqlite", () => {
    return {
        DatabaseSync: class {
            exec() {}
            loadExtension() {}
            prepare() {
                return { 
                    get: vi.fn().mockReturnValue({ c: 0 }), 
                    run: vi.fn().mockReturnValue({ changes: 1 }), 
                    all: vi.fn().mockReturnValue([]) 
                };
            }
            close() {}
        }
    };
});

vi.mock("../../src/memory/DatabaseWorkerBridge", () => {
    return {
        DatabaseWorkerBridge: class {
            initialize() { return Promise.resolve(); }
            run() { return Promise.resolve({ changes: 1, lastInsertRowid: null }); }
            query() { return Promise.resolve([]); }
            runBatch() { return Promise.resolve({ changes: 1, lastInsertRowid: null }); }
            transactionBatch() { return Promise.resolve({ changes: 1, lastInsertRowid: null }); }
            exec() { return Promise.resolve(); }
            backup() { return Promise.resolve(); }
            dispose() { return Promise.resolve(); }
            prepare() {
                return {
                    get: vi.fn().mockResolvedValue({ c: 0 }),
                    all: vi.fn().mockResolvedValue([]),
                    run: vi.fn().mockResolvedValue({ changes: 1, lastInsertRowid: null })
                };
            }
        }
    };
});

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(), fatal: vi.fn(), trace: vi.fn() },
}));

// Mock EmbeddingService
vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: () => ({
            ensureReady: vi.fn().mockResolvedValue(undefined),
            embed: vi.fn().mockResolvedValue(new Array(384).fill(0.1)),
            embedWithTimeout: vi.fn().mockResolvedValue(new Array(384).fill(0.1)),
            ready: true,
            dimension: 384,
        }),
    },
}));

// Mock fs/promises
import { MemoryManager } from "../../src/MemoryManager";

describe("MemoryManager", () => {
    let mm: MemoryManager;

    beforeEach(() => {
        process.env.LIVA_USE_NATIVE = "true";
        process.env.LIVA_ENCRYPTION_KEY = "LIVA_TEST_KEY_32BYTES_XXXXXXXXXX";
        vol.reset();
        vol.fromJSON({
            [path.join(process.cwd(), "..", "data", "user_profile.json")]: JSON.stringify({ name: "Dương" }),
            [path.join(process.cwd(), "data", "memory", "structured_facts.json")]: JSON.stringify({ key: "value" }),
        });
        vi.clearAllMocks();
        mm = new MemoryManager("test-agent");
    });

    afterEach(async () => {
        await mm.dispose();
    });    describe("initialization", () => {
        it("should create a MemoryManager instance", () => {
            expect(mm).toBeInstanceOf(MemoryManager);
        });

        it("should initialize without errors", async () => {
            await expect(mm.initialize()).resolves.not.toThrow();
        });

        it("should handle missing turbo_quant_memory.jsonl gracefully during init", async () => {
            // [v27] turbo_quant_memory.jsonl no longer exists — init should succeed without it
            await mm.initialize();
            const history = await mm.getShortTermHistory();
            // May have cross-session warm-up context, but no JSONL-loaded messages
            expect(Array.isArray(history)).toBe(true);
        });

        it("[v27] should not depend on turbo_quant_memory.jsonl anymore", async () => {
            // Verify initialization works without any legacy file
            await mm.initialize();
            const history = await mm.getShortTermHistory();
            expect(Array.isArray(history)).toBe(true);
        });

        it("should load cross-session warm-up context (Lines 145-153)", async () => {
            const { StructuredMemory } = await import("../../src/memory/StructuredMemory");
            vi.spyOn(StructuredMemory, "create").mockResolvedValueOnce({
                getTurnsByTimeRange: vi.fn().mockReturnValue([{ userMsg: "Hello", aiReply: "Hi there" }]),
                flushTouchQueue: vi.fn().mockResolvedValue(undefined),
                close: vi.fn()
            } as any);
            const { logger } = await import("../../src/utils/logger");
            const infoSpy = vi.spyOn(logger, 'info').mockImplementation(() => {});

            await mm.initialize();
            
            expect(infoSpy).toHaveBeenCalledWith(expect.stringContaining("Loaded 2 recent turns from L1 SQLite (last 2h)."));
            const history = await mm.getShortTermHistory();
            expect(history.length).toBe(2);
            expect(history[0].content).toBe("Hello");
            expect(history[1].content).toBe("Hi there");

            const contextPrompt = await mm.getPreviousSessionContextPrompt();
            expect(contextPrompt).toContain("<PREVIOUS_SESSION_CONTEXT>");
            expect(contextPrompt).toContain("User: Hello");
            expect(contextPrompt).toContain("Assistant: Hi there");
        });

        it("should catch and log initialization errors (Line 160)", async () => {
            const embeddingService = (mm as any).embeddingService;
            vi.spyOn(embeddingService, 'ensureReady').mockRejectedValueOnce(new Error("Init failed"));
            const { logger } = await import("../../src/utils/logger");
            const errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {});

            await mm.initialize();
            expect(errorSpy).toHaveBeenCalledWith(expect.stringContaining("Lỗi khởi tạo (Initialization error):"));
        });
    });

    describe("message management", () => {
        it("should add a user message to memory", async () => {
            await mm.addMessage("user", "Xin chào LIVA");
            const history = await mm.getShortTermHistory();
            expect(history.length).toBe(1);
            expect(history[0].role).toBe("user");
            expect(history[0].content).toBe("Xin chào LIVA");
        });

        it("should add multiple messages", async () => {
            await mm.addMessage("user", "Hello");
            await mm.addMessage("assistant", "Hi there!");
            await mm.addMessage("user", "How are you?");
            const history = await mm.getShortTermHistory();
            expect(history.length).toBe(3);
        });

        it("should evict old messages when cache exceeds 50", async () => {
            // Add 60 messages
            for (let i = 0; i < 60; i++) {
                await mm.addMessage("user", `Message ${i}`);
            }
            const history = await mm.getShortTermHistory();
            // After adding 51st, cache gets sliced to 30, then more are added
            expect(history.length).toBeLessThanOrEqual(50);
        });
    });

    describe("session state", () => {
        it("should get session state", async () => {
            const state = await mm.getSessionState();
            expect(typeof state).toBe("string");
        });

        it("should update session state", async () => {
            await expect(mm.updateSessionState("# New Session")).resolves.not.toThrow();
        });
    });

    describe("long-term memory", () => {
        it("should get long-term markdown", async () => {
            const md = await mm.getLongTermMarkdown();
            expect(typeof md).toBe("string");
        });

        it("should append to long-term markdown", async () => {
            await expect(mm.appendLongTermMarkdown("## New Section\n- Fact 1")).resolves.not.toThrow();
        });

        it("should append daily log", async () => {
            await expect(mm.appendDailyLog("User discussed AI topics")).resolves.not.toThrow();
        });
    });

    describe("user profile", () => {
        it("should get user profile", async () => {
            const profile = await mm.getUserProfile();
            expect(profile).toEqual({ name: "Dương" });
        });

        it("should update user profile", async () => {
            await expect(mm.updateUserProfile({ age: 20 })).resolves.not.toThrow();
        });
    });

    describe("structured memory delegation", () => {
        beforeEach(async () => {
            await mm.initialize();
        });
        it("should set a structured fact", async () => {
            await expect(mm.setStructuredFact("key", "value")).resolves.not.toThrow();
        });

        it("should get structured facts", () => {
            const facts = mm.getStructuredFacts();
            expect(Array.isArray(facts)).toBe(true);
        });

        it("should get structured memory prompt", () => {
            const prompt = mm.getStructuredMemoryPrompt();
            expect(typeof prompt).toBe("string");
        });

        it("should delete a structured fact", async () => {
            // MemoryManager initializes getStructuredFacts implicitly, but structured fact deletion requires it to be present.
            await mm.setStructuredFact("key", "value");
            const result = await mm.deleteStructuredFact("key");
            expect(result).toBe(true);
        });
    });

    describe("hybrid context", () => {
        it("should return full history if under window size", async () => {
            await mm.addMessage("user", "Hello");
            await mm.addMessage("assistant", "Hi!");
            const ctx = await mm.getHybridContext("test query", 6);
            expect(ctx.length).toBe(2);
        });

        it("should apply RAG for large histories", async () => {
            // Add more than windowSize messages
            for (let i = 0; i < 10; i++) {
                await mm.addMessage("user", `Question ${i}`);
                await mm.addMessage("assistant", `Answer ${i}`);
            }
            const ctx = await mm.getHybridContext("Question 5", 6);
            // Should have recent window + possibly recalled memories
            expect(ctx.length).toBeGreaterThanOrEqual(6);
        });
    });

    describe("dispose", () => {
        it("should dispose without errors", async () => {
            await expect(mm.dispose()).resolves.not.toThrow();
        });
    });

    describe("long-term encrypted memory", () => {
        it("should get long-term context from encrypted file", async () => {
            const result = await mm.getLongTermContext();
            expect(typeof result).toBe("string");
        });

        it("should return empty string when encrypted file is missing", async () => {
            const fsp = await import("node:fs/promises");
            vi.spyOn(fsp, "readFile").mockRejectedValueOnce(new Error("ENOENT"));

            const result = await mm.getLongTermContext();
            expect(result).toBe("");
        });

        it("should update long-term memory with existing section", async () => {
            const fsp = await import("node:fs/promises");
            // Return raw text (unencrypted — decryptData will pass through non-3-part text)
            vi.spyOn(fsp, "readFile").mockResolvedValueOnce("## Thói quen\n- Coffee addict" as any);

            const writeSpy = vi.spyOn(fsp, "writeFile").mockResolvedValueOnce(undefined);
            await mm.updateLongTermMemory("Thói quen", ["Prefers dark mode"]);
            expect(writeSpy).toHaveBeenCalled();
        });

        it("should update long-term memory with new section", async () => {
            const fsp = await import("node:fs/promises");
            vi.spyOn(fsp, "readFile").mockResolvedValueOnce("# Existing content\n" as any);

            const writeSpy = vi.spyOn(fsp, "writeFile").mockResolvedValueOnce(undefined);
            await mm.updateLongTermMemory("New Category", ["Fact 1", "Fact 2"]);
            expect(writeSpy).toHaveBeenCalled();
        });
    });

    describe("user profile (error paths)", () => {
        it("should return null when user_profile.json is missing", async () => {
            const fsp = await import("node:fs/promises");
            vi.spyOn(fsp, "readFile").mockRejectedValueOnce(new Error("ENOENT: no such file"));

            const result = await mm.getUserProfile();
            expect(result).toBeNull();
        });

        it("should handle updateUserProfile error gracefully", async () => {
            const fsp = await import("node:fs/promises");
            // getUserProfile call inside updateUserProfile fails
            vi.spyOn(fsp, "readFile").mockRejectedValueOnce(new Error("ENOENT"));
            vi.spyOn(fsp, "writeFile").mockRejectedValueOnce(new Error("Permission denied"));

            // Should not throw
            await expect(mm.updateUserProfile({ age: 20 })).resolves.not.toThrow();
        });
    });

    describe("getStructuredMemoryInstance", () => {
        it("should return the StructuredMemory instance", async () => {
            await mm.initialize();
            const instance = mm.getStructuredMemoryInstance();
            expect(instance).toBeDefined();
            expect(typeof instance.setFact).toBe("function");
        });
    });

    describe("GDPR Purge & Vector Memory", () => {
        it("should purge user context safely (without vector memory)", async () => {
            // Memory manager has no initialized structuredMemory yet
            await expect(mm.purgeUserContext()).resolves.not.toThrow();
        });

        it("should purge user context safely (with vector memory via StructuredMemory)", async () => {
            await mm.initialize();
            const sm = mm.getStructuredMemoryInstance();
            const deleteAllSpy = vi.spyOn(sm, 'deleteAllVectors').mockImplementation(() => {});
            
            await mm.purgeUserContext();
            expect(deleteAllSpy).toHaveBeenCalled();
        });
    });

    describe("Cross-Session Warm-up & UHM Fallbacks", () => {
        it("should safely handle cross-session warm-up exception", async () => {
            const { StructuredMemory } = await import("../../src/memory/StructuredMemory");
            vi.spyOn(StructuredMemory, "create").mockResolvedValueOnce({
                getTurnsByTimeRange: vi.fn().mockImplementation(() => { throw new Error("Simulated UHM Error"); }),
                flushTouchQueue: vi.fn().mockResolvedValue(undefined),
                close: vi.fn()
            } as any);

            // Initialize should swallow the error and log it
            await expect(mm.initialize()).resolves.not.toThrow();
            const { logger } = await import("../../src/utils/logger");
            expect(logger.warn).toHaveBeenCalledWith(expect.stringContaining("Failed to warm-up from L1 turns"));
        });
    });

    describe("Error paths & conditions for 100% Coverage", () => {
        it("should catch and log error in appendDailyLog (Line 234)", async () => {
            const fsPromises = await import("node:fs/promises");
            vi.spyOn(fsPromises, 'appendFile').mockRejectedValueOnce(new Error("Disk full"));
            const { logger } = await import("../../src/utils/logger");
            const errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {});

            await mm.appendDailyLog("test log");
            expect(errorSpy).toHaveBeenCalledWith(expect.stringContaining("Lỗi ghi nhật ký hàng ngày"));
        });

        it("should catch and log embedding error in addMessage (Line 272)", async () => {
            const embeddingService = (mm as any).embeddingService;
            vi.spyOn(embeddingService, 'embed').mockRejectedValueOnce(new Error("API timeout"));
            const { logger } = await import("../../src/utils/logger");
            const warnSpy = vi.spyOn(logger, 'warn').mockImplementation(() => {});

            await mm.addMessage("user", "test error message");
            // Background task takes a tick (setTimeout)
            await new Promise(r => setTimeout(r, 50));
            
            expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining("Embedding lỗi (bỏ qua)"));
        });

        it("should catch embedding error in getHybridContext and skip semantic search", async () => {
            const embeddingService = (mm as any).embeddingService;
            vi.spyOn(embeddingService, 'embedWithTimeout').mockRejectedValueOnce(new Error("Network fail"));
            const { logger } = await import("../../src/utils/logger");
            const warnSpy = vi.spyOn(logger, 'warn').mockImplementation(() => {});

            for (let i = 0; i < 10; i++) {
                await mm.addMessage("user", `Question ${i}`);
                await mm.addMessage("assistant", `Answer ${i}`);
            }

            const result = await mm.getHybridContext("test query");
            expect(warnSpy).toHaveBeenCalledWith(
                expect.stringContaining("Embedding timeout/lỗi, bỏ qua semantic search:")
            );
            expect(Array.isArray(result)).toBe(true);
        });

        it("should execute decryptData and return result in getLongTermContext (Line 379)", async () => {
            const fsPromises = await import("node:fs/promises");
            vi.spyOn(fsPromises, 'readFile').mockResolvedValueOnce("00000000000000000000000000000000:00000000000000000000000000000000:00000000000000000000000000000000");

            const result = await mm.getLongTermContext();
            expect(result).toBe("00000000000000000000000000000000:00000000000000000000000000000000:00000000000000000000000000000000");
        });

        it("should successfully initialize UHM (initUHM)", async () => {
            const aiClientMock = {} as any;
            await mm.initialize();
            // initUHM no longer needs LanceMemory — it initializes ConsolidationCron with StructuredMemory
            await mm.initUHM(aiClientMock);
            expect(mm.consolidationCron).toBeDefined();
        });

        it("should catch and throw error when initUHM fails", async () => {
            const aiClientMock = {} as any;
            // Force it to throw by making initialize fail first
            (mm as any).structuredMemory = { 
                initVecDimension: () => { throw new Error("Vec Error"); },
                flushTouchQueue: vi.fn().mockResolvedValue(undefined),
                close: vi.fn()
            };
            
            await expect(mm.initUHM(aiClientMock)).rejects.toThrow("Vec Error");
        });

        it("should catch error in purgeUserContext", async () => {
            const { logger } = await import("../../src/utils/logger");
            const errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {});
            const fsPromises = await import("node:fs/promises");
            vi.spyOn(fsPromises, 'writeFile').mockRejectedValueOnce(new Error("File system locked"));

            await mm.purgeUserContext();
            expect(errorSpy).toHaveBeenCalledWith(expect.stringContaining("Lỗi trong quá trình Purge"));
        });

        it("should properly encrypt and decrypt a real string (line 37)", async () => {
            const fsPromises = await import("node:fs/promises");
            // Mock readFile to return a dummy encrypted string that decryptData will catch and return as raw text
            // Wait, actually decryptData falls back to returning the text if it's not valid format.
            // Let's just mock readFile to return something valid, or just let it return "Some raw content"
            vi.spyOn(fsPromises, 'readFile').mockResolvedValueOnce("Some raw content");
            let writtenContent = "";
            vi.spyOn(fsPromises, 'writeFile').mockImplementation(async (path, data) => {
                writtenContent = data as string;
            });

            await mm.updateLongTermMemory("Test Category", ["Fact 1"]);
            
            // Now decrypt the written content using getLongTermContext's inner call
            vi.spyOn(fsPromises, 'readFile').mockResolvedValueOnce(writtenContent);
            const context = await mm.getLongTermContext();
            
            expect(context).toContain("Test Category");
            expect(context).toContain("Fact 1");
        });

        it("should skip background embedding when embeddingService.ready is false (Line 288)", async () => {
            const embeddingService = (mm as any).embeddingService;
            const originalReady = embeddingService.ready;
            embeddingService.ready = false;
            const embedSpy = vi.spyOn(embeddingService, 'embed');

            await mm.addMessage("user", "test without embedding");
            // Give a tick for setImmediate
            await new Promise(r => setTimeout(r, 50));

            // embed should NOT have been called since ready is false
            expect(embedSpy).not.toHaveBeenCalled();

            // Restore
            embeddingService.ready = originalReady;
        });
    });

    describe("resetAllMemory", () => {
        it("should successfully reset all memory files and reinitialize", async () => {
            await mm.initialize();

            // Setup some dummy profile & files
            await mm.updateSessionState("# Previous Data");

            // Perform reset
            const result = await mm.resetAllMemory();
            expect(result).toEqual({ success: true });

            // Session state should be reset to default template
            const session = await mm.getSessionState();
            expect(session).toContain("# SESSION STATE");
            expect(session).toContain("## Core Intent");
            expect(session).not.toContain("# Previous Data");

            // Long-term memory markdown should be empty/fresh
            const ltMarkdown = await mm.getLongTermMarkdown();
            expect(ltMarkdown).toBe("# LONG-TERM MEMORY\n\n");

            // StructuredMemory should be reinitialized
            const sm = mm.getStructuredMemoryInstance();
            expect(sm).toBeDefined();
        });

        it("should return success: false and capture error on failure", async () => {
            await mm.initialize();

            // Force fs.mkdir to throw an error
            const fsPromises = await import("node:fs/promises");
            vi.spyOn(fsPromises, 'mkdir').mockRejectedValueOnce(new Error("Permission denied"));

            const result = await mm.resetAllMemory();
            expect(result.success).toBe(false);
            expect(result.error).toContain("Permission denied");
        });
    });
});
