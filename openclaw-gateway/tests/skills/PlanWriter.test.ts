/**
 * PlanWriter.test.ts — Plan Generation Skill Tests
 * ===================================================
 * Tests: metadata, workspace creation, LLM multi-section generation, error handling.
 * fs, LivaEngine, and ZaloNotifier are FULLY MOCKED — NO real disk/network ops.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("fs/promises", async () => {
    const memfs = await import("memfs");
    return memfs.fs.promises;
});

vi.mock("fs", async () => {
    const memfs = await import("memfs");
    return memfs.fs;
});
import { vol } from "memfs";


// ============================================================
// Mock all external dependencies
// ============================================================
const mockChatCreate = vi.fn();



vi.mock("../../src/utils/LivaEngine", () => ({
    livaEngine: {
        chat: {
            completions: {
                create: (...args: any[]) => mockChatCreate(...args),
            },
        },
        getSeal: vi.fn().mockReturnValue("MOCK_SEAL"),
        secureChatCompletion: (...args: any[]) => mockChatCreate(...args),
    },
    generateSmartFilename: vi.fn().mockResolvedValue("smart_plan_name"),
}));

vi.mock("../../src/utils/NativeIPCClient", () => ({
    NativeIPCClient: vi.fn(),
}));

const { execute, metadata } = await import("../../src/skills/PlanWriter");

describe("PlanWriter Skill", () => {
    beforeEach(() => {
        vol.reset();
        vi.clearAllMocks();
    });

    describe("metadata", () => {
        it("should have correct skill name", () => {
            expect(metadata.name).toBe("plan_writer");
        });

        it("should require projectName and fileLocation", () => {
            expect(metadata.parameters.required).toContain("projectName");
            expect(metadata.parameters.required).toContain("fileLocation");
        });

        it("should have search_keywords", () => {
            expect(metadata.search_keywords).toBeDefined();
            expect(metadata.search_keywords.length).toBeGreaterThan(0);
        });
    });

    describe("execute() — Happy Path", () => {
        it("should generate a plan with 8 sections", async () => {
            const result = await execute({
                projectName: "Marketing Quý 2",
                fileLocation: "C:/mock/workspace",
                providedContext: "Ngân sách 500 triệu, deadline Tháng 6"
            });

            expect(result).toContain("Hoàn tất");
            // 8 sections → 8 LLM calls
            expect(mockChatCreate).toHaveBeenCalledTimes(8);
            const files = vol.readdirSync("C:/mock/workspace");
            expect(files.length).toBeGreaterThan(0);
        });
    });

    describe("execute() — Workspace Creation", () => {
        it("should create workspace directory if it does not exist", async () => {
            await execute({
                projectName: "Test Plan",
                fileLocation: "C:/new/workspace",
            });

            expect(vol.existsSync("C:/new/workspace")).toBe(true);
        });

        it("should skip mkdir if workspace already exists", async () => {
            vol.mkdirSync("C:/existing/workspace", { recursive: true });

            await execute({
                projectName: "Test Plan",
                fileLocation: "C:/existing/workspace",
            });

            expect(vol.existsSync("C:/existing/workspace")).toBe(true);
        });
    });

    describe("execute() — Error Handling", () => {
        it("should continue generating when one section fails", async () => {
            // Make 3rd LLM call fail, rest succeed
            mockChatCreate
                .mockResolvedValueOnce({ choices: [{ message: { content: "Section 1 content" } }] })
                .mockResolvedValueOnce({ choices: [{ message: { content: "Section 2 content" } }] })
                .mockRejectedValueOnce(new Error("VRAM exhausted"))
                .mockResolvedValue({ choices: [{ message: { content: "Section N content" } }] });

            const result = await execute({
                projectName: "Error Test Plan",
                fileLocation: "C:/mock/workspace",
            });

            // Should still complete — errors are caught per-section
            expect(result).toContain("Hoàn tất");
        });

        it("should handle empty LLM response gracefully", async () => {
            mockChatCreate.mockResolvedValue({
                choices: [{ message: { content: "" } }],
            });

            const result = await execute({
                projectName: "Empty Response Test",
                fileLocation: "C:/mock/workspace",
            });

            expect(result).toContain("Hoàn tất");
        });
    });
});
