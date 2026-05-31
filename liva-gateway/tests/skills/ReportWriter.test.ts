/**
 * ReportWriter.test.ts — Report Generation Skill Tests
 * ======================================================
 * Tests: metadata, workspace creation, LLM multi-section report, academic mode, error handling.
 * fs, LivaEngine, ZaloNotifier, and HttpClient are FULLY MOCKED — NO real disk/network ops.
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
const mockSafeFetch = vi.fn();
const mockChatCreate = vi.fn();

vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: (...args: any[]) => mockSafeFetch(...args),
}));

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
    generateSmartFilename: vi.fn().mockResolvedValue("smart_report_name"),
}));

vi.mock("../../src/utils/NativeIPCClient", () => ({
    NativeIPCClient: vi.fn(),
}));

const { execute, metadata } = await import("../../src/skills/docs/ReportWriter");

describe("ReportWriter Skill", () => {
    beforeEach(() => {
        vol.reset();
        vi.clearAllMocks();
    });

    describe("metadata", () => {
        it("should have correct skill name", () => {
            expect(metadata.name).toBe("report_writer");
        });

        it("should require topic and fileLocation", () => {
            expect(metadata.parameters.required).toContain("topic");
            expect(metadata.parameters.required).toContain("fileLocation");
        });

        it("should have isAcademic optional parameter", () => {
            expect(metadata.parameters.properties.isAcademic).toBeDefined();
            expect(metadata.parameters.properties.isAcademic.type).toBe("boolean");
        });
    });

    describe("execute() — Standard Report", () => {
        it("should generate a report with 7 sections", async () => {
            const result = await execute({
                topic: "Doanh thu Tháng 4",
                fileLocation: "C:/mock/reports",
            });

            expect(result).toContain("Báo cáo đã xuất bản");
            // 7 sections → 7 LLM calls
            expect(mockChatCreate).toHaveBeenCalledTimes(7);
            const files = vol.readdirSync("C:/mock/reports");
            expect(files.length).toBeGreaterThan(0);
        });
    });

    describe("execute() — Academic Report with Semantic Scholar", () => {
        it("should fetch Semantic Scholar papers when isAcademic=true", async () => {
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({
                    data: [
                        {
                            title: "Deep Learning for NLP",
                            abstract: "A comprehensive survey...",
                            authors: [{ name: "John Doe" }],
                            year: 2024,
                            url: "https://semanticscholar.org/paper/123",
                            citationCount: 100
                        }
                    ]
                })
            });

            const result = await execute({
                topic: "AI in Healthcare",
                fileLocation: "/mock/academic",
                isAcademic: true,
            });

            expect(result).toContain("Báo cáo đã xuất bản");
            expect(mockSafeFetch).toHaveBeenCalledTimes(1);
            expect(mockSafeFetch).toHaveBeenCalledWith(
                expect.stringContaining("semanticscholar.org"),
                expect.any(Object),
                10000
            );
        });

        it("should continue gracefully if Semantic Scholar API fails", async () => {
            mockSafeFetch.mockRejectedValueOnce(new Error("Network timeout"));

            const result = await execute({
                topic: "AI Research",
                fileLocation: "/mock/academic",
                isAcademic: true,
            });

            // Should still generate the report despite scholar API failure
            expect(result).toContain("Báo cáo đã xuất bản");
            expect(mockChatCreate).toHaveBeenCalledTimes(7);
        });
    });

    describe("execute() — Error Handling", () => {
        it("should continue generating when one section LLM call fails", async () => {
            mockChatCreate
                .mockResolvedValueOnce({ choices: [{ message: { content: "Section 1" } }] })
                .mockRejectedValueOnce(new Error("VRAM exhausted"))
                .mockResolvedValue({ choices: [{ message: { content: "Section N" } }] });

            const result = await execute({
                topic: "Error Test Report",
                fileLocation: "C:/mock/workspace",
            });

            expect(result).toContain("Báo cáo đã xuất bản");
            // Check that files were written in memfs
            const files = vol.readdirSync("C:/mock/workspace");
            expect(files.length).toBeGreaterThan(0);
        });

        it("should create workspace if it does not exist", async () => {
            mockChatCreate.mockResolvedValueOnce({
                choices: [{ message: { content: JSON.stringify({ sections: [{ id: "1", desc: "desc" }] }) } }],
            }).mockResolvedValue({ choices: [{ message: { content: "Section" } }] });

            await execute({
                topic: "New Workspace Report",
                fileLocation: "C:/new/reports/dir",
            });

            expect(vol.existsSync("C:/new/reports/dir")).toBe(true);
        });
    });
});
