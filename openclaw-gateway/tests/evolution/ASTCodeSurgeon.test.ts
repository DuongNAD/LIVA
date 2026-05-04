import { describe, it, expect, vi, beforeEach } from "vitest";
import { ASTCodeSurgeon, SecurityViolationError } from "../../src/evolution/ASTCodeSurgeon";
import * as fsp from "fs/promises";
import * as prettier from "prettier";
import { Project } from "ts-morph";

vi.mock("fs/promises");
vi.mock("prettier");

const mockFormat = vi.mocked(prettier.format);

vi.mock("ts-morph", () => {
    return {
        ScriptTarget: { ESNext: 99 },
        Project: vi.fn().mockImplementation(function() {
            return {
                addSourceFileAtPath: vi.fn((path) => {
                    if (path.includes("not-exist")) throw new Error("File not found");
                    return {
                        getFunction: vi.fn((name) => {
                            if (name === "existingFunc") {
                                return {
                                    setBodyText: vi.fn()
                                };
                            }
                            return undefined;
                        }),
                        getFullText: vi.fn(() => "full text")
                    };
                }),
                getPreEmitDiagnostics: vi.fn(() => []),
                formatDiagnosticsWithColorAndContext: vi.fn(() => "mock diagnostics error")
            };
        })
    };
});

describe("ASTCodeSurgeon", () => {
    let surgeon: ASTCodeSurgeon;

    beforeEach(() => {
        surgeon = new ASTCodeSurgeon();
        vi.clearAllMocks();
        mockFormat.mockResolvedValue("formatted text");
    });

    it("should throw SecurityViolationError on path traversal", async () => {
        await expect(surgeon.applyAstSurgery("../outside.ts", "{}")).rejects.toThrow(SecurityViolationError);
        await expect(surgeon.revert("../outside.ts")).rejects.toThrow(SecurityViolationError);
    });

    it("should throw error on malformed JSON", async () => {
        await expect(surgeon.applyAstSurgery("test.ts", "not a json")).rejects.toThrow("Missing JSON braces");
        await expect(surgeon.applyAstSurgery("test.ts", "{ invalid json }")).rejects.toThrow("JSON parsing failed");
    });

    it("should parse repaired JSON and modify function", async () => {
        const jsonInstruction = `{ "replaceFunctionBody": "console.log('test');", "functionName": "existingFunc" }`;
        
        await surgeon.applyAstSurgery("test.ts", jsonInstruction);

        expect(fsp.copyFile).toHaveBeenCalled();
        expect(fsp.writeFile).toHaveBeenCalled();
        expect(fsp.rename).toHaveBeenCalled();
    });

    it("should throw if file doesn't exist", async () => {
        const jsonInstruction = `{ "replaceFunctionBody": "console.log('test');", "functionName": "existingFunc" }`;
        
        await expect(surgeon.applyAstSurgery("not-exist.ts", jsonInstruction)).rejects.toThrow("File không tồn tại");
    });

    it("should throw on pre-flight diagnostics error", async () => {
        const jsonInstruction = `{ "replaceFunctionBody": "console.log('test');", "functionName": "existingFunc" }`;
        
        vi.mocked(Project).mockImplementationOnce(function() {
            return {
                addSourceFileAtPath: vi.fn(() => ({ getFunction: vi.fn() })),
                getPreEmitDiagnostics: vi.fn(() => [{ messageText: "error" }]),
                formatDiagnosticsWithColorAndContext: vi.fn(() => "syntax error")
            } as any;
        });

        await expect(surgeon.applyAstSurgery("test.ts", jsonInstruction)).rejects.toThrow("Lỗi cú pháp/Type script sau khi sửa");
    });

    it("should gracefully handle prettier failure and fallback to raw output", async () => {
        const jsonInstruction = `{ "replaceFunctionBody": "console.log('test');", "functionName": "existingFunc" }`;
        
        mockFormat.mockRejectedValueOnce(new Error("Prettier error"));

        const res = await surgeon.applyAstSurgery("test.ts", jsonInstruction);
        expect(res).toBe("SUCCESS");
        expect(fsp.writeFile).toHaveBeenCalledWith(expect.any(String), "full text", "utf-8"); // raw text
    });

    it("should revert file if I/O write fails", async () => {
        const jsonInstruction = `{ "replaceFunctionBody": "console.log('test');", "functionName": "existingFunc" }`;
        
        vi.mocked(fsp.rename).mockRejectedValueOnce(new Error("Write failed"));

        await expect(surgeon.applyAstSurgery("test.ts", jsonInstruction)).rejects.toThrow("Write failed");
        
        // revert should be called implicitly inside catch
        // But the rename inside revert might also fail because it's mocked to fail or not, let's see:
        // First call is temp->orig, second is bak->orig
        expect(fsp.rename).toHaveBeenCalledTimes(2); 
    });

    describe("revert", () => {
        it("should revert successfully", async () => {
            const res = await surgeon.revert("test.ts");
            expect(res).toBe(true);
            expect(fsp.rename).toHaveBeenCalled();
        });

        it("should return false if revert fails", async () => {
            vi.mocked(fsp.rename).mockRejectedValueOnce(new Error("Rename failed"));
            const res = await surgeon.revert("test.ts");
            expect(res).toBe(false);
        });
    });
});
