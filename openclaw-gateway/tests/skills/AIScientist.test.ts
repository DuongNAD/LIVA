/**
 * AIScientist.test.ts — Evolution Engine skill tests
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock all evolution subsystem dependencies
vi.mock("../../src/evolution/DarwinianEvolver.js", () => {
    function MockDarwinianEvolver() {
        (this as any).evaluateBatchPopulation = vi.fn().mockResolvedValue({
            bestCandidateId: null,
            bestSandboxRoot: null,
            asiFeedbackReport: "All candidates eliminated",
        });
    }
    return { DarwinianEvolver: MockDarwinianEvolver };
});

vi.mock("../../src/evolution/LearningLog.js", () => {
    function MockLearningLog() {
        (this as any).connect = vi.fn().mockResolvedValue(undefined);
        (this as any).getRelevantAxioms = vi.fn().mockResolvedValue("No axioms available");
        (this as any).recordAttempt = vi.fn().mockResolvedValue(undefined);
    }
    return { LearningLog: MockLearningLog };
});

vi.mock("../../src/sandbox/MicroVMDaemon.js", () => {
    function MockMicroVMDaemon() {
        (this as any).verifyShadowCandidate = vi.fn().mockResolvedValue({ pass: false, vmLogs: "test failed", executionTimeMs: 100 });
    }
    return { MicroVMDaemon: MockMicroVMDaemon };
});

vi.mock("../../src/deployment/BlueGreenRouter.js", () => {
    function MockBlueGreenRouter() {
        (this as any).deployToGreenBatch = vi.fn().mockResolvedValue(false);
        (this as any).autoRollbackBatch = vi.fn().mockResolvedValue(undefined);
        (this as any).autoRollback = vi.fn().mockResolvedValue(undefined);
    }
    return { BlueGreenRouter: MockBlueGreenRouter };
});

vi.mock("../../src/evolution/QualityChecker.js", () => {
    function MockQualityChecker() {
        (this as any).evaluateCodeQuality = vi.fn().mockResolvedValue({ pass: true, feedback: "" });
    }
    return { QualityChecker: MockQualityChecker };
});

vi.mock("../../src/evolution/StructuredExtractor.js", () => ({
    extractXMLPatches: vi.fn().mockReturnValue({
        success: false,
        method: "xml",
        data: null,
        errors: ["No candidates found"],
    }),
}));

vi.mock("../../src/evolution/WebResearchAgent.js", () => ({
    fullResearch: vi.fn().mockResolvedValue({
        goalInsights: "",
        errorFixes: "",
        totalResults: 0,
    }),
}));

vi.mock("openai", () => {
    function MockOpenAI() {
        (this as any).chat = {
            completions: {
                create: vi.fn().mockResolvedValue({
                    choices: [{ message: { content: "<candidate id='cand_A'></candidate>" } }],
                }),
            },
        };
    }
    return { default: MockOpenAI };
});

// Mock node:fs with hoisted functions for test control
const { mockFsExistsSync, mockFsReadFile, mockFsAccess } = vi.hoisted(() => ({
    mockFsExistsSync: vi.fn(),
    mockFsReadFile: vi.fn(),
    mockFsAccess: vi.fn(),
}));

vi.mock("node:fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("node:fs")>();
    return {
        ...actual,
        existsSync: mockFsExistsSync,
        rmSync: vi.fn(),
        promises: {
            ...actual.promises,
            readFile: mockFsReadFile,
            access: mockFsAccess,
        },
    };
});

import { metadata, execute } from "../../src/skills/agentic/AIScientist";

describe("AIScientist", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        // Default mock behavior: file exists with content
        mockFsExistsSync.mockReturnValue(true);
        mockFsReadFile.mockResolvedValue("const x = 1;\n");
        mockFsAccess.mockResolvedValue(undefined); // fsp.access success
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("liva_ai_scientist");
        });

        it("should require goal and targetFilePath", () => {
            expect(metadata.parameters.required).toContain("goal");
            expect(metadata.parameters.required).toContain("targetFilePath");
        });

        it("should have description in English", () => {
            expect(metadata.description).toContain("Evolution Engine");
        });
    });

    describe("execute", () => {
        it("should return error for non-existent file", async () => {
            mockFsAccess.mockRejectedValueOnce(new Error("ENOENT"));

            const result = await execute({
                goal: "test goal",
                targetFilePath: "nonexistent.ts",
            });
            expect(result).toContain("not found");
        });

        it("should handle full evolution loop and return report", async () => {
            const result = await execute({
                goal: "Optimize function X",
                targetFilePath: "src/core/test.ts",
            });
            // Should complete all cycles and stall
            expect(result).toContain("EVOLUTION ENGINE");
            expect(result).toContain("stalled");
        });

    });
});
