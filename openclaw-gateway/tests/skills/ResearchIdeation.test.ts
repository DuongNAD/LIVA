import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock FS
vi.mock("fs/promises", async () => {
    const memfs = await import("memfs");
    return memfs.fs.promises;
});

vi.mock("fs", async () => {
    const memfs = await import("memfs");
    return { ...memfs.fs, default: memfs.fs };
});

vi.mock("node:fs", async () => {
    const memfs = await import("memfs");
    return { ...memfs.fs, default: memfs.fs };
});

import { vol, fs as memfs } from "memfs";

// Mock Logger & Notifier
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

vi.mock("../../src/utils/ZaloNotifier", () => ({
    notifyZalo: vi.fn().mockResolvedValue(true)
}));

// Mock HTTP Fetch
const mockSafeFetch = vi.fn();
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: (...args: any[]) => mockSafeFetch(...args)
}));

// Mock LLM
const mockChatCreate = vi.fn();
vi.mock("../../src/utils/LivaEngine", () => ({
    livaEngine: {
        chat: { completions: { create: (...args: any[]) => mockChatCreate(...args) } },
        getSeal: vi.fn().mockReturnValue("MOCK_SEAL"),
        secureChatCompletion: (...args: any[]) => mockChatCreate(...args),
    },
    generateSmartFilename: vi.fn().mockResolvedValue("smart_idea")
}));

// Import target module
import { execute, metadata } from "../../src/skills/agentic/ResearchIdeation";

import * as path from "node:path";
const TEST_DIR = path.join(process.cwd(), "mock_ideation");

describe("ResearchIdeation Skill", () => {
    beforeEach(() => {
        vol.reset();
        vol.mkdirSync(process.cwd(), { recursive: true });
        vi.clearAllMocks();
    });

    describe("metadata", () => {
        it("should have correct skill name", () => {
            expect(metadata.name).toBe("sakana_ideation");
        });

        it("should require topic and fileLocation", () => {
            expect(metadata.parameters.required).toContain("topic");
            expect(metadata.parameters.required).toContain("fileLocation");
        });
    });

    describe("execute()", () => {
        it("should generate ideation report", async () => {
            // Mock Ideation generation
            mockChatCreate.mockResolvedValueOnce({
                choices: [{
                    message: {
                        content: JSON.stringify({
                            ideas: [
                                { name: "Idea 1", description: "Desc 1", noveltyScore: 80, keywords: "keyword1" }
                            ]
                        })
                    }
                }]
            });

            // Mock Semantic Scholar search
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({ data: [] })
            });

            // Mock Evaluation
            mockChatCreate.mockResolvedValueOnce({
                choices: [{
                    message: { content: "Novelty: 85\nFeasibility: 90\nDetailed evaluation..." }
                }]
            });

            // Mock Final Proposal
            mockChatCreate.mockResolvedValueOnce({
                choices: [{
                    message: { content: "# Kế Hoạch\nChi tiết..." }
                }]
            });

            const result = await execute({
                topic: "AI Agents",
                fileLocation: TEST_DIR
            });

            expect(result).toContain("LIVA Sakana Loop hoàn tất");
            // Check that the file was actually written to memfs
            const files = memfs.readdirSync(TEST_DIR);
            expect(files.length).toBeGreaterThan(0);
        });

        it("should handle invalid JSON from LLM gracefully", async () => {
            mockChatCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "Not JSON" } }]
            });

            const result = await execute({
                topic: "AI Agents",
                fileLocation: TEST_DIR
            });

            expect(result).toContain("Lỗi Ideation");
        });
    });
});
