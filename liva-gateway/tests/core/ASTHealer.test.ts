/**
 * ASTHealer.test.ts — AST Self-Healing & Diagnostic Tests
 * =========================================================
 * Tests auto-import healing, ASI diagnostic extraction, and error handling.
 * ts-morph is mocked — NO real TypeScript compilation.
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
// Mock ts-morph to avoid real TS compilation
// ============================================================
const mockFixMissingImports = vi.fn();
const mockOrganizeImports = vi.fn();
const mockFixUnusedIdentifiers = vi.fn();
const mockFormatText = vi.fn();
const mockSave = vi.fn().mockResolvedValue(undefined);
const mockGetFilePath = vi.fn().mockReturnValue("/project/src/test.ts");
const mockGetBaseName = vi.fn().mockReturnValue("test.ts");
const mockGetMessageText = vi.fn().mockReturnValue("Type 'string' is not assignable to type 'number'");
const mockGetLineNumber = vi.fn().mockReturnValue(42);
const mockGetSourceFile = vi.fn().mockReturnValue({ getFilePath: mockGetFilePath, getBaseName: mockGetBaseName });

const mockSourceFile = {
    getFilePath: mockGetFilePath,
    fixMissingImports: mockFixMissingImports,
    organizeImports: mockOrganizeImports,
    fixUnusedIdentifiers: mockFixUnusedIdentifiers,
    formatText: mockFormatText,
};

const mockDiagnostic = {
    getMessageText: mockGetMessageText,
    getLineNumber: mockGetLineNumber,
    getSourceFile: mockGetSourceFile,
};

export const mockGetPreEmitDiagnostics = vi.fn().mockReturnValue([]);

vi.mock("ts-morph", () => {
    return {
        Project: class MockProject {
            constructor() {}
            getSourceFiles() { return [mockSourceFile]; }
            getPreEmitDiagnostics() { return mockGetPreEmitDiagnostics(); }
            save() { return mockSave(); }
        },
        SourceFile: class MockSourceFile {},
    };
});



const { ASTHealer } = await import("../../src/core/ASTHealer");
let healer: any;

describe("ASTHealer", () => {
    beforeEach(() => {
        healer = new ASTHealer();
        vi.clearAllMocks();
    });

    describe("autoHealImportsOnSandbox()", () => {
        it("should call fixMissingImports, organizeImports, fixUnusedIdentifiers for each source file", async () => {
            const result = await healer.autoHealImportsOnSandbox("/sandbox/project");

            expect(result.success).toBe(true);
            expect(result.logs).toContain("✅");
            expect(mockFixMissingImports).toHaveBeenCalled();
            expect(mockOrganizeImports).toHaveBeenCalled();
            expect(mockFixUnusedIdentifiers).toHaveBeenCalled();
            expect(mockFormatText).toHaveBeenCalled();
            expect(mockSave).toHaveBeenCalled();
        });

        it("should skip node_modules files", async () => {
            // Override getFilePath to return a node_modules path
            mockGetFilePath.mockReturnValueOnce("/sandbox/node_modules/pkg/index.ts");

            const result = await healer.autoHealImportsOnSandbox("/sandbox/project");

            expect(result.success).toBe(true);
            // fixMissingImports should NOT be called for node_modules files
            expect(mockFixMissingImports).not.toHaveBeenCalled();
        });

        it("should handle ts-morph errors gracefully", async () => {
            // Make save() throw
            mockSave.mockRejectedValueOnce(new Error("EACCES: permission denied"));

            const result = await healer.autoHealImportsOnSandbox("/sandbox/broken");

            expect(result.success).toBe(false);
            expect(result.logs).toContain("Cảnh báo Healer");
        });
    });

    describe("getASIFromPreEmitDiagnosticsOnSandbox()", () => {
        it("should return empty string when no diagnostics exist", () => {
            mockGetPreEmitDiagnostics.mockReturnValueOnce([]);
            const result = healer.getASIFromPreEmitDiagnosticsOnSandbox("/sandbox/clean");
            expect(result).toBe("");
        });

        it("should return ASI report when diagnostics exist", () => {
            mockGetPreEmitDiagnostics.mockReturnValueOnce([mockDiagnostic]);
            const result = healer.getASIFromPreEmitDiagnosticsOnSandbox("/sandbox/dirty");
            expect(result).toContain("<actionable_side_information>");
            expect(result).toContain("[File: test.ts] Dòng [42]: Type 'string' is not assignable to type 'number'");
        });

        it("should handle diagnostic message as object", () => {
            const complexMessage = { getMessageText: () => "Complex type error" };
            const diagnosticWithObjectMessage = { ...mockDiagnostic, getMessageText: () => complexMessage };
            mockGetPreEmitDiagnostics.mockReturnValueOnce([diagnosticWithObjectMessage]);
            const result = healer.getASIFromPreEmitDiagnosticsOnSandbox("/sandbox/complex");
            expect(result).toContain("Complex type error");
        });

        it("should ignore diagnostics from node_modules", () => {
            const diagnosticFromNodeModules = {
                ...mockDiagnostic,
                getSourceFile: () => ({ getFilePath: () => "/sandbox/node_modules/pkg/index.ts", getBaseName: () => "index.ts" })
            };
            mockGetPreEmitDiagnostics.mockReturnValueOnce([diagnosticFromNodeModules]);
            const result = healer.getASIFromPreEmitDiagnosticsOnSandbox("/sandbox/nodemodules");
            expect(result).toBe("");
        });

        it("should handle missing line or source file gracefully", () => {
            const incompleteDiagnostic = {
                getMessageText: () => "Incomplete diagnostic",
                getLineNumber: () => undefined,
                getSourceFile: () => undefined,
            };
            mockGetPreEmitDiagnostics.mockReturnValueOnce([incompleteDiagnostic]);
            const result = healer.getASIFromPreEmitDiagnosticsOnSandbox("/sandbox/incomplete");
            expect(result).toContain("UnknownFile");
            expect(result).toContain("Unknown");
        });

        it("should handle ts-morph errors gracefully in ASI extraction", () => {
            mockGetPreEmitDiagnostics.mockImplementationOnce(() => { throw new Error("Compilation crashed"); });
            const result = healer.getASIFromPreEmitDiagnosticsOnSandbox("/broken/path");
            expect(result).toContain("[ASI Engine Fatal Error]");
            expect(result).toContain("Compilation crashed");
        });
    });
});
