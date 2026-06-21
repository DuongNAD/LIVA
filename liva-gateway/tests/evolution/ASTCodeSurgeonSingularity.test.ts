import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as fsp from "fs/promises";
import * as path from "path";
import { ASTCodeSurgeon } from "../../src/evolution/ASTCodeSurgeon";

const mockApplySurgery = vi.fn();

vi.mock("../../src/core/ASTWorkerBridge", () => ({
    ASTWorkerBridge: {
        applySurgery: (targetFile: string, instructions: any) => mockApplySurgery(targetFile, instructions)
    }
}));

describe("ASTCodeSurgeon Singularity", () => {
    let surgeon: ASTCodeSurgeon;
    const dummyFilePath = path.resolve(process.cwd(), "dummy_target.ts");
    const tempGraphDir = path.resolve(process.cwd(), "data");
    const tempGraphPath = path.resolve(tempGraphDir, "ast_graph.json");

    const originalContent = `export function originalFunc() {\n    console.log("hello world");\n}\n\nexport function anotherFunc() {\n    return 42;\n}\n`;
    const modifiedContent = `export function originalFunc() {\n    console.log("modified");\n}\n\nexport function anotherFunc() {\n    return 42;\n}\n`;

    const mockGraph = {
      "type": "repository",
      "name": "src",
      "children": [
        {
          "type": "file",
          "name": "liva-gateway\\src\\bridges\\CDPBridge.ts",
          "filePath": "liva-gateway\\src\\bridges\\CDPBridge.ts",
          "children": [
            {
              "type": "class",
              "name": "CDPBridge",
              "filePath": "liva-gateway\\src\\bridges\\CDPBridge.ts",
              "children": [
                {
                  "type": "method",
                  "name": "connect",
                  "filePath": "liva-gateway\\src\\bridges\\CDPBridge.ts",
                  "calls": ["send"]
                },
                {
                  "type": "method",
                  "name": "send",
                  "filePath": "liva-gateway\\src\\bridges\\CDPBridge.ts",
                  "calls": []
                },
                {
                  "type": "method",
                  "name": "disconnect",
                  "filePath": "liva-gateway\\src\\bridges\\CDPBridge.ts",
                  "calls": []
                }
              ]
            }
          ]
        },
        {
          "type": "file",
          "name": "liva-gateway\\src\\core\\AgentLoop.ts",
          "filePath": "liva-gateway\\src\\core\\AgentLoop.ts",
          "children": [
            {
              "type": "method",
              "name": "run",
              "filePath": "liva-gateway\\src\\core\\AgentLoop.ts",
              "calls": ["connect"]
            }
          ]
        },
        {
          "type": "file",
          "name": "liva-gateway\\src\\other\\Helper.ts",
          "filePath": "liva-gateway\\src\\other\\Helper.ts",
          "children": [
            {
              "type": "method",
              "name": "help1",
              "filePath": "liva-gateway\\src\\other\\Helper.ts",
              "calls": ["helperFunc"]
            },
            {
              "type": "method",
              "name": "help2",
              "filePath": "liva-gateway\\src\\other\\Helper.ts",
              "calls": ["helperFunc"]
            },
            {
              "type": "method",
              "name": "help3",
              "filePath": "liva-gateway\\src\\other\\Helper.ts",
              "calls": ["helperFunc"]
            },
            {
              "type": "method",
              "name": "help4",
              "filePath": "liva-gateway\\src\\other\\Helper.ts",
              "calls": ["helperFunc"]
            },
            {
              "type": "method",
              "name": "helperFunc",
              "filePath": "liva-gateway\\src\\other\\Helper.ts",
              "calls": []
            }
          ]
        },
        {
          "type": "file",
          "name": "liva-gateway\\src\\other\\SomeFile.ts",
          "filePath": "liva-gateway\\src\\other\\SomeFile.ts",
          "children": [
            {
              "type": "method",
              "name": "someFunc",
              "filePath": "liva-gateway\\src\\other\\SomeFile.ts",
              "calls": ["someTarget"]
            },
            {
              "type": "method",
              "name": "someTarget",
              "filePath": "liva-gateway\\src\\other\\SomeFile.ts",
              "calls": []
            }
          ]
        }
      ]
    };

    beforeEach(async () => {
        surgeon = new ASTCodeSurgeon();
        
        // Write the dummy file
        await fsp.writeFile(dummyFilePath, originalContent, "utf-8");

        // Write the temp ast_graph.json
        await fsp.mkdir(tempGraphDir, { recursive: true });
        await fsp.writeFile(tempGraphPath, JSON.stringify(mockGraph, null, 2), "utf-8");
        
        mockApplySurgery.mockReset();
    });

    afterEach(async () => {
        // Cleanup dummy file
        try {
            await fsp.unlink(dummyFilePath);
        } catch {}

        // Cleanup temp ast_graph.json
        try {
            await fsp.unlink(tempGraphPath);
        } catch {}
    });

    it("should perform dryRun surgery, generate diff and calculate blast-radius report without mutating target file (CRITICAL risk)", async () => {
        mockApplySurgery.mockResolvedValue(modifiedContent);

        const instructions = {
            replaceFunctionBody: 'console.log("modified");',
            functionName: 'send'
        };

        const resultJson = await surgeon.applyAstSurgery("dummy_target.ts", JSON.stringify(instructions), true);
        const result = JSON.parse(resultJson);

        expect(result.success).toBe(true);
        expect(result.diff).toContain("originalFunc");
        expect(result.diff).toContain("modified");
        
        // Target file must remain unmodified
        const diskContent = await fsp.readFile(dummyFilePath, "utf-8");
        expect(diskContent).toBe(originalContent);

        // Blast radius validation
        expect(result.blastRadius.target).toBe("send");
        // directCallers of 'send' is 'connect'
        expect(result.blastRadius.directCallers).toContain("connect");
        // transitiveCallers of 'send' is 'run'
        expect(result.blastRadius.transitiveCallers).toContain("run");
        // riskLevel should be CRITICAL because 'run' is in AgentLoop.ts
        expect(result.blastRadius.riskLevel).toBe("CRITICAL");
    });

    it("should calculate HIGH risk when total callers > 3 and not in core files", async () => {
        mockApplySurgery.mockResolvedValue(modifiedContent);

        const instructions = {
            replaceFunctionBody: 'console.log("modified");',
            functionName: 'helperFunc'
        };

        const resultJson = await surgeon.applyAstSurgery("dummy_target.ts", JSON.stringify(instructions), true);
        const result = JSON.parse(resultJson);

        expect(result.success).toBe(true);
        expect(result.blastRadius.target).toBe("helperFunc");
        expect(result.blastRadius.directCallers).toContain("help1");
        expect(result.blastRadius.directCallers).toContain("help2");
        expect(result.blastRadius.directCallers).toContain("help3");
        expect(result.blastRadius.directCallers).toContain("help4");
        expect(result.blastRadius.transitiveCallers).toHaveLength(0);
        expect(result.blastRadius.riskLevel).toBe("HIGH");
    });

    it("should calculate MEDIUM risk when total callers between 1 and 3", async () => {
        mockApplySurgery.mockResolvedValue(modifiedContent);

        const instructions = {
            replaceFunctionBody: 'console.log("modified");',
            functionName: 'someTarget'
        };

        const resultJson = await surgeon.applyAstSurgery("dummy_target.ts", JSON.stringify(instructions), true);
        const result = JSON.parse(resultJson);

        expect(result.success).toBe(true);
        expect(result.blastRadius.target).toBe("someTarget");
        expect(result.blastRadius.directCallers).toContain("someFunc");
        expect(result.blastRadius.riskLevel).toBe("MEDIUM");
    });

    it("should calculate LOW risk when total callers is 0", async () => {
        mockApplySurgery.mockResolvedValue(modifiedContent);

        const instructions = {
            replaceFunctionBody: 'console.log("modified");',
            functionName: 'disconnect'
        };

        const resultJson = await surgeon.applyAstSurgery("dummy_target.ts", JSON.stringify(instructions), true);
        const result = JSON.parse(resultJson);

        expect(result.success).toBe(true);
        expect(result.blastRadius.target).toBe("disconnect");
        expect(result.blastRadius.directCallers).toHaveLength(0);
        expect(result.blastRadius.riskLevel).toBe("LOW");
    });
});
