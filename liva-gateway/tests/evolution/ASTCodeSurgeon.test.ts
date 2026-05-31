import { describe, it, expect, vi, beforeEach } from "vitest";
import { ASTCodeSurgeon, SecurityViolationError } from "../../src/evolution/ASTCodeSurgeon";
import * as fsp from "fs/promises";
import * as prettier from "prettier";

vi.mock("fs/promises");
vi.mock("prettier");
vi.mock("../../src/utils/FileUtils", () => ({
    safeRename: vi.fn().mockResolvedValue(undefined)
}));

const mockFormat = vi.mocked(prettier.format);
import { safeRename } from "../../src/utils/FileUtils";
const mockSafeRename = vi.mocked(safeRename);

const mockApplySurgery = vi.fn();

vi.mock("../../src/core/ASTWorkerBridge", () => ({
    ASTWorkerBridge: {
        applySurgery: (targetFile: string, instructions: any) => mockApplySurgery(targetFile, instructions)
    }
}));

describe("ASTCodeSurgeon", () => {
    let surgeon: ASTCodeSurgeon;

    beforeEach(() => {
        surgeon = new ASTCodeSurgeon();
        vi.clearAllMocks();
        mockFormat.mockResolvedValue("formatted text");
        mockApplySurgery.mockResolvedValue("full text");
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
        expect(mockSafeRename).toHaveBeenCalled();
    });

    it("should throw if file doesn't exist", async () => {
        const jsonInstruction = `{ "replaceFunctionBody": "console.log('test');", "functionName": "existingFunc" }`;
        mockApplySurgery.mockRejectedValueOnce(new Error("File không tồn tại: test.ts"));
        
        await expect(surgeon.applyAstSurgery("not-exist.ts", jsonInstruction)).rejects.toThrow("File không tồn tại");
    });

    it("should throw on pre-flight diagnostics error", async () => {
        const jsonInstruction = `{ "replaceFunctionBody": "console.log('test');", "functionName": "existingFunc" }`;
        mockApplySurgery.mockRejectedValueOnce(new Error("Lỗi cú pháp/Type script sau khi sửa"));

        await expect(surgeon.applyAstSurgery("test.ts", jsonInstruction)).rejects.toThrow("Lỗi cú pháp/Type script sau khi sửa");
    });

    it("should gracefully handle prettier failure and fallback to raw output", async () => {
        const jsonInstruction = `{ "replaceFunctionBody": "console.log('test');", "functionName": "existingFunc" }`;
        mockApplySurgery.mockResolvedValueOnce("raw text");
        mockFormat.mockRejectedValueOnce(new Error("Prettier error"));

        const res = await surgeon.applyAstSurgery("test.ts", jsonInstruction);
        expect(res).toBe("SUCCESS");
        expect(fsp.writeFile).toHaveBeenCalledWith(expect.any(String), "raw text", "utf-8"); // raw text
    });

    it("should revert file if I/O write fails", async () => {
        const jsonInstruction = `{ "replaceFunctionBody": "console.log('test');", "functionName": "existingFunc" }`;
        mockSafeRename.mockRejectedValueOnce(new Error("Write failed"));

        await expect(surgeon.applyAstSurgery("test.ts", jsonInstruction)).rejects.toThrow("Write failed");
        
        // revert should be called implicitly inside catch
        expect(mockSafeRename).toHaveBeenCalledTimes(2); 
    });

    describe("revert", () => {
        it("should revert successfully", async () => {
            const res = await surgeon.revert("test.ts");
            expect(res).toBe(true);
            expect(mockSafeRename).toHaveBeenCalled();
        });

        it("should return false if revert fails", async () => {
            mockSafeRename.mockRejectedValueOnce(new Error("Rename failed"));
            const res = await surgeon.revert("test.ts");
            expect(res).toBe(false);
        });
    });
});
