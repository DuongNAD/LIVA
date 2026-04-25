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

vi.mock("../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock EmbeddingService
vi.mock("../src/services/EmbeddingService", () => ({
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
import { MemoryManager } from "../src/MemoryManager";

describe("MemoryManager", () => {
    let mm: MemoryManager;

    beforeEach(() => {
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
});
