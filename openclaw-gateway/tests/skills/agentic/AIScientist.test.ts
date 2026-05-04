import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("openai", () => ({
    default: class {
        chat = { completions: { create: vi.fn().mockResolvedValue({ choices: [{ message: { content: "<candidate id='cand_A'></candidate>" } }] }) } };
    }
}));

vi.mock("@evolution/DarwinianEvolver.js", () => ({
    DarwinianEvolver: class {
        evaluateBatchPopulation = vi.fn().mockResolvedValue({ bestCandidateId: null, bestSandboxRoot: null, asiFeedbackReport: "All eliminated" });
    }
}));

vi.mock("@evolution/LearningLog.js", () => ({
    LearningLog: class {
        connect = vi.fn().mockResolvedValue(undefined);
        getRelevantAxioms = vi.fn().mockResolvedValue("axiom1");
        recordAttempt = vi.fn().mockResolvedValue(undefined);
    }
}));

vi.mock("@sandbox/MicroVMDaemon.js", () => ({
    MicroVMDaemon: class {
        verifyShadowCandidate = vi.fn().mockResolvedValue({ pass: false, vmLogs: "test fail", executionTimeMs: 100 });
    }
}));

vi.mock("@deployment/BlueGreenRouter.js", () => ({
    BlueGreenRouter: class {
        deployToGreenBatch = vi.fn().mockResolvedValue(true);
        autoRollbackBatch = vi.fn().mockResolvedValue(undefined);
        autoRollback = vi.fn().mockResolvedValue(undefined);
    }
}));

vi.mock("@evolution/QualityChecker.js", () => ({
    QualityChecker: class {
        evaluateCodeQuality = vi.fn().mockResolvedValue({ pass: true, feedback: "OK" });
    }
}));

vi.mock("@evolution/StructuredExtractor.js", () => ({
    extractXMLPatches: vi.fn().mockReturnValue({ success: false, errors: ["no candidates"], method: "xml" }),
}));

vi.mock("@evolution/WebResearchAgent.js", () => ({
    fullResearch: vi.fn().mockResolvedValue({ goalInsights: "insight", errorFixes: "", totalResults: 3 })
}));

// Mock node:fs — the source imports both sync and promises from this module
const mockExistsSync = vi.fn().mockReturnValue(true);
const mockRmSync = vi.fn();
const mockReadFile = vi.fn().mockResolvedValue("const x = 1;");

vi.mock("node:fs", () => ({
    existsSync: (...args: any[]) => mockExistsSync(...args),
    rmSync: (...args: any[]) => mockRmSync(...args),
    promises: {
        readFile: (...args: any[]) => mockReadFile(...args)
    }
}));

import { execute, metadata } from "../../../src/skills/agentic/AIScientist";

describe("Skill - AIScientist", () => {
    beforeEach(() => { vi.clearAllMocks(); mockExistsSync.mockReturnValue(true); });

    it("should export metadata", () => { expect(metadata.name).toBe("liva_ai_scientist"); });

    it("should return error when target file not found", async () => {
        mockExistsSync.mockReturnValue(false);
        const result = await execute({ goal: "Fix bug", targetFilePath: "nonexistent.ts" });
        expect(result).toContain("Target file not found");
    });

    it("should handle extraction failure and exhaust cycles", async () => {
        const result = await execute({ goal: "Improve perf", targetFilePath: "src/test.ts" });
        expect(result).toContain("Evolution stalled");
    });
});
