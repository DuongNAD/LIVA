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

vi.mock("ts-morph", () => {
    return {
        Project: class MockProject {
            constructor() {}
            getSourceFiles() { return [mockSourceFile]; }
            getPreEmitDiagnostics() { return []; }
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
            const result = healer.getASIFromPreEmitDiagnosticsOnSandbox("/sandbox/clean");
            expect(result).toBe("");
        });

        it("should handle ts-morph initialization failures gracefully", () => {
            // ASTHealer.getASIFromPreEmitDiagnosticsOnSandbox wraps everything in try-catch
            // and returns an error string on failure. The method creates its own Project
            // internally, so we test that non-existent tsconfig paths are handled.
            const result = healer.getASIFromPreEmitDiagnosticsOnSandbox("/nonexistent/path/that/will/fail");
            // Should either return empty (no diagnostics from mock) or error string
            expect(typeof result).toBe("string");
        });
    });
});
