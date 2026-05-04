/**
 * MemoryManager.test.ts — Core memory orchestrator tests
 * Tests encryption, session state, message buffering, hybrid context
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as path from "node:path";

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

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock EmbeddingService
vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: () => ({
            ensureReady: vi.fn().mockResolvedValue(undefined),
            embed: vi.fn().mockResolvedValue(new Array(256).fill(0.1)),
            embedWithTimeout: vi.fn().mockResolvedValue(new Array(256).fill(0.1)),
            ready: true,
        }),
    },
}));

// Mock fs/promises
import { MemoryManager } from "../../src/MemoryManager";

describe("MemoryManager", () => {
    let mm: MemoryManager;

    beforeEach(() => {
        process.env.LIVA_USE_NATIVE = "true";
        vol.reset();
        vol.fromJSON({
            [path.join(process.cwd(), "src", "user_profile.json")]: JSON.stringify({ name: "Dương" }),
            [path.join(process.cwd(), "data", "memory", "structured_facts.json")]: JSON.stringify({ key: "value" }),
        });
        vi.clearAllMocks();
        mm = new MemoryManager("test-agent");
    });

    afterEach(() => {
        mm.dispose();
    });    describe("initialization", () => {
        it("should create a MemoryManager instance", () => {
            expect(mm).toBeInstanceOf(MemoryManager);
        });

        it("should initialize without errors", async () => {
            await expect(mm.initialize()).resolves.not.toThrow();
        });

        it("should parse existing short-term history and catch invalid json (Lines 128-136)", async () => {
            const fsPromises = await import("node:fs/promises");
            const shortTermPath = path.join(process.cwd(), "data", "agents", "test-agent", "turbo_quant_memory.jsonl");
            
            // Write some valid and invalid JSON lines
            await fsPromises.mkdir(path.dirname(shortTermPath), { recursive: true });
            await fsPromises.writeFile(shortTermPath, '{"role":"user","content":"hi"}\nINVALID_JSON\n');
            
            // When initializing, it will read the file and fail on the second line
            await mm.initialize();
            
            // memCache should be reset to [] due to the catch block
            const history = await mm.getShortTermHistory();
            expect(history.length).toBe(0);
        });

        it("should load cross-session warm-up context (Lines 145-153)", async () => {
            const structuredMemory = mm.getStructuredMemoryInstance();
            vi.spyOn(structuredMemory, 'getTurnsByTimeRange').mockReturnValue([
                { userMsg: "Hello", aiReply: "Hi there" }
            ]);
            const { logger } = await import("../../src/utils/logger");
            const infoSpy = vi.spyOn(logger, 'info').mockImplementation(() => {});

            await mm.initialize();
            
            expect(infoSpy).toHaveBeenCalledWith(expect.stringContaining("Cross-session warm-up: loaded 1 turn(s)"));
            const history = await mm.getShortTermHistory();
            // Should contain the system warm-up message
            expect(history.some(m => m.role === "system" && m.content.includes("PREVIOUS SESSION CONTEXT"))).toBe(true);
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

        it("should evict old messages when cache exceeds 200", async () => {
            // Add 210 messages
            for (let i = 0; i < 210; i++) {
                await mm.addMessage("user", `Message ${i}`);
            }
            const history = await mm.getShortTermHistory();
            // After adding 201st, cache gets sliced to 100, then more are added
            expect(history.length).toBeLessThanOrEqual(200);
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
        it("should set a structured fact", () => {
            expect(() => mm.setStructuredFact("key", "value")).not.toThrow();
        });

        it("should get structured facts", () => {
            const facts = mm.getStructuredFacts();
            expect(Array.isArray(facts)).toBe(true);
        });

        it("should get structured memory prompt", () => {
            const prompt = mm.getStructuredMemoryPrompt();
            expect(typeof prompt).toBe("string");
        });

        it("should delete a structured fact", () => {
            // MemoryManager initializes getStructuredFacts implicitly, but structured fact deletion requires it to be present.
            mm.setStructuredFact("key", "value");
            const result = mm.deleteStructuredFact("key");
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
        it("should dispose without errors", () => {
            expect(() => mm.dispose()).not.toThrow();
        });
    });

    describe("long-term encrypted memory", () => {
        it("should get long-term context from encrypted file", async () => {
            const result = await mm.getLongTermContext();
            expect(typeof result).toBe("string");
        });

        it("should return empty string when encrypted file is missing", async () => {
            const fsp = await import("fs/promises");
            vi.spyOn(fsp, "readFile").mockRejectedValueOnce(new Error("ENOENT"));

            const result = await mm.getLongTermContext();
            expect(result).toBe("");
        });

        it("should update long-term memory with existing section", async () => {
            const fsp = await import("fs/promises");
            // Return raw text (unencrypted — decryptData will pass through non-3-part text)
            vi.spyOn(fsp, "readFile").mockResolvedValueOnce("## Thói quen\n- Coffee addict" as any);

            const writeSpy = vi.spyOn(fsp, "writeFile").mockResolvedValueOnce(undefined);
            await mm.updateLongTermMemory("Thói quen", ["Prefers dark mode"]);
            expect(writeSpy).toHaveBeenCalled();
        });

        it("should update long-term memory with new section", async () => {
            const fsp = await import("fs/promises");
            vi.spyOn(fsp, "readFile").mockResolvedValueOnce("# Existing content\n" as any);

            const writeSpy = vi.spyOn(fsp, "writeFile").mockResolvedValueOnce(undefined);
            await mm.updateLongTermMemory("New Category", ["Fact 1", "Fact 2"]);
            expect(writeSpy).toHaveBeenCalled();
        });
    });

    describe("user profile (error paths)", () => {
        it("should return null when user_profile.json is missing", async () => {
            const fsp = await import("fs/promises");
            vi.spyOn(fsp, "readFile").mockRejectedValueOnce(new Error("ENOENT: no such file"));

            const result = await mm.getUserProfile();
            expect(result).toBeNull();
        });

        it("should handle updateUserProfile error gracefully", async () => {
            const fsp = await import("fs/promises");
            // getUserProfile call inside updateUserProfile fails
            vi.spyOn(fsp, "readFile").mockRejectedValueOnce(new Error("ENOENT"));
            vi.spyOn(fsp, "writeFile").mockRejectedValueOnce(new Error("Permission denied"));

            // Should not throw
            await expect(mm.updateUserProfile({ age: 20 })).resolves.not.toThrow();
        });
    });

    describe("getStructuredMemoryInstance", () => {
        it("should return the StructuredMemory instance", () => {
            const instance = mm.getStructuredMemoryInstance();
            expect(instance).toBeDefined();
            expect(typeof instance.setFact).toBe("function");
        });
    });

    describe("GDPR Purge & LanceMemory", () => {
        it("should purge user context safely (without LanceMemory)", async () => {
            // Memory manager has no lanceMemory initially
            await expect(mm.purgeUserContext()).resolves.not.toThrow();
        });

        it("should purge user context safely (with LanceMemory)", async () => {
            const mockLance = { deleteVectors: vi.fn().mockResolvedValue(undefined), dispose: vi.fn() };
            (mm as any).lanceMemory = mockLance;
            
            await mm.purgeUserContext();
            expect(mockLance.deleteVectors).toHaveBeenCalledWith("type != ''");
        });
    });

    describe("Cross-Session Warm-up & UHM Fallbacks", () => {
        it("should safely handle cross-session warm-up exception", async () => {
            // Mock structuredMemory to throw
            vi.spyOn(mm.getStructuredMemoryInstance(), "getTurnsByTimeRange").mockImplementationOnce(() => {
                throw new Error("Simulated UHM Error");
            });

            // Initialize should swallow the error and log it
            await expect(mm.initialize()).resolves.not.toThrow();
            const { logger } = await import("../../src/utils/logger");
            expect(logger.warn).toHaveBeenCalledWith(expect.stringContaining("Cross-session warm-up failed"));
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

        it("should catch embedding error in getHybridContext and return dummy vector (Line 298)", async () => {
            const embeddingService = (mm as any).embeddingService;
            vi.spyOn(embeddingService, 'embedWithTimeout').mockRejectedValueOnce(new Error("Network fail"));
            const { logger } = await import("../../src/utils/logger");
            const warnSpy = vi.spyOn(logger, 'warn').mockImplementation(() => {});

            const result = await mm.getHybridContext("test query");
            expect(warnSpy).toHaveBeenCalledWith(
                expect.stringContaining("Embedding timeout/lỗi, dùng dummy vector cho semantic search:"),
                "Network fail"
            );
            expect(Array.isArray(result)).toBe(true);
        });

        it("should execute decryptData and return result in getLongTermContext (Line 379)", async () => {
            const fsPromises = await import("fs/promises");
            vi.spyOn(fsPromises, 'readFile').mockResolvedValueOnce("00000000000000000000000000000000:00000000000000000000000000000000:00000000000000000000000000000000");

            const result = await mm.getLongTermContext();
            expect(result).toBe("00000000000000000000000000000000:00000000000000000000000000000000:00000000000000000000000000000000");
        });

        it("should successfully initialize UHM (initUHM)", async () => {
            const aiClientMock = {} as any;
            await mm.initUHM(aiClientMock);
            expect(mm.lanceMemory).toBeDefined();
            expect(mm.consolidationCron).toBeDefined();
        });

        it("should catch and throw error when initUHM fails", async () => {
            const aiClientMock = {} as any;
            // Force it to throw by mocking LanceMemoryManager
            const { LanceMemoryManager } = await import("../../src/memory/LanceMemory");
            vi.spyOn(LanceMemoryManager.prototype, 'connect').mockRejectedValueOnce(new Error("Lance DB Error"));
            
            await expect(mm.initUHM(aiClientMock)).rejects.toThrow("Lance DB Error");
        });

        it("should catch error in purgeUserContext", async () => {
            const { logger } = await import("../../src/utils/logger");
            const errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {});
            const fsPromises = await import("fs/promises");
            vi.spyOn(fsPromises, 'writeFile').mockRejectedValueOnce(new Error("File system locked"));

            await mm.purgeUserContext();
            expect(errorSpy).toHaveBeenCalledWith(expect.stringContaining("Lỗi trong quá trình Purge"));
        });

        it("should properly encrypt and decrypt a real string (line 37)", async () => {
            const fsPromises = await import("fs/promises");
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
});
