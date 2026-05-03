import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ASTCodeSurgeon, SecurityViolationError } from "../../src/evolution/ASTCodeSurgeon";
import * as fsp from "fs/promises";
import * as prettier from "prettier";
import { Project } from "ts-morph";

vi.mock("fs/promises");
vi.mock("prettier", () => ({
    format: vi.fn().mockImplementation((code) => code)
}));
vi.mock("ts-morph", () => {
    return {
        Project: vi.fn(),
        ScriptTarget: { ESNext: 99 }
    };
});

describe("ASTCodeSurgeon", () => {
    let surgeon: ASTCodeSurgeon;

    beforeEach(() => {
        surgeon = new ASTCodeSurgeon();
        vi.clearAllMocks();
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    it("should block path traversal", async () => {
        await expect(surgeon.applyAstSurgery("../../../etc/shadow", "{}")).rejects.toThrow(SecurityViolationError);
    });

    it("should run atomic write on success", async () => {
        const mockProject = {
            addSourceFileAtPath: vi.fn().mockReturnValue({
                getFunction: vi.fn(),
                getFullText: vi.fn().mockReturnValue("const a = 1;")
            }),
            getPreEmitDiagnostics: vi.fn().mockReturnValue([]) // No errors
        };
        vi.mocked(Project).mockImplementation(function() { return mockProject; } as any);

        vi.mocked(fsp.copyFile).mockResolvedValue(undefined);
        vi.mocked(fsp.writeFile).mockResolvedValue(undefined);
        vi.mocked(fsp.rename).mockResolvedValue(undefined);

        const result = await surgeon.applyAstSurgery("test.ts", JSON.stringify({ replaceFunctionBody: "123", functionName: "test" }));

        expect(result).toBe("SUCCESS");
        expect(fsp.copyFile).toHaveBeenCalled();
        expect(fsp.writeFile).toHaveBeenCalled();
        expect(fsp.rename).toHaveBeenCalled();
    });

    it("should throw on pre-flight diagnostics error and NOT write to disk", async () => {
        const mockProject = {
            addSourceFileAtPath: vi.fn().mockReturnValue({
                getFunction: vi.fn(),
                getFullText: vi.fn().mockReturnValue("const a = 1;")
            }),
            getPreEmitDiagnostics: vi.fn().mockReturnValue([{ messageText: "Type error" }]), // Simulated Error
            formatDiagnosticsWithColorAndContext: vi.fn().mockReturnValue("Formatted Error")
        };
        vi.mocked(Project).mockImplementation(function() { return mockProject; } as any);

        await expect(surgeon.applyAstSurgery("test.ts", JSON.stringify({}))).rejects.toThrow("Lỗi cú pháp/Type script sau khi sửa:\nFormatted Error");
        
        // Assert atomic write is NOT called
        expect(fsp.writeFile).not.toHaveBeenCalled();
    });
});
