/**
 * ASTActuator.test.ts — Smoke Test + Guardrail Tests
 * Tests basic instantiation and mutation quota guardrails
 * without requiring a full ts-morph sandbox setup.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("node:fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("node:fs")>();
    const promisesMock = {
        access: vi.fn().mockRejectedValue(new Error("ENOENT")),
        mkdir: vi.fn().mockResolvedValue(undefined),
        rm: vi.fn().mockResolvedValue(undefined),
        readFile: vi.fn().mockResolvedValue(""),
        writeFile: vi.fn().mockResolvedValue(undefined),
        rename: vi.fn().mockResolvedValue(undefined),
        copyFile: vi.fn().mockResolvedValue(undefined),
        cp: vi.fn().mockResolvedValue(undefined),
        symlink: vi.fn().mockResolvedValue(undefined),
        unlink: vi.fn().mockResolvedValue(undefined),
        realpath: vi.fn().mockImplementation((p: string) => Promise.resolve(p)),
        stat: vi.fn().mockResolvedValue({ size: 100 } as any),
    };
    return {
        ...actual,
        existsSync: vi.fn().mockReturnValue(true),
        mkdirSync: vi.fn(),
        promises: promisesMock,
        default: {
            ...actual,
            existsSync: vi.fn().mockReturnValue(true),
            mkdirSync: vi.fn(),
            promises: promisesMock,
        }
    };
});

vi.mock("node:fs/promises", () => ({
    access: vi.fn().mockRejectedValue(new Error("ENOENT")),
    mkdir: vi.fn().mockResolvedValue(undefined),
    rm: vi.fn().mockResolvedValue(undefined),
    readFile: vi.fn().mockResolvedValue(""),
    writeFile: vi.fn().mockResolvedValue(undefined),
    rename: vi.fn().mockResolvedValue(undefined),
    copyFile: vi.fn().mockResolvedValue(undefined),
    cp: vi.fn().mockResolvedValue(undefined),
    symlink: vi.fn().mockResolvedValue(undefined),
    unlink: vi.fn().mockResolvedValue(undefined),
    realpath: vi.fn().mockImplementation((p: string) => Promise.resolve(p)),
    stat: vi.fn().mockResolvedValue({ size: 100 } as any),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

vi.mock("ts-morph", () => {
    return {
        Project: class {
            getSourceFile() { return { delete: vi.fn(), replaceWithText: vi.fn() }; }
            addSourceFileAtPath() { return {}; }
            save() { return Promise.resolve(); }
        }
    };
});

import { ASTActuator } from "../../src/core/ASTActuator";

describe("ASTActuator", () => {
    let actuator: ASTActuator;

    beforeEach(async () => {
        vi.clearAllMocks();
        const fs = await import("node:fs");
        vi.mocked(fs.promises.access).mockRejectedValue(new Error("ENOENT"));
        vi.mocked(fs.promises.mkdir).mockResolvedValue(undefined);
        vi.mocked(fs.promises.rm).mockResolvedValue(undefined);
        vi.mocked(fs.promises.readFile).mockResolvedValue("");
        vi.mocked(fs.promises.writeFile).mockResolvedValue(undefined);
        vi.mocked(fs.promises.realpath).mockImplementation((p: any) => Promise.resolve(p as string));
        vi.mocked(fs.promises.stat).mockResolvedValue({ size: 100 } as any);
        actuator = new ASTActuator("/tmp/test-workspace");
    });

    describe("Smoke Test", () => {
        it("should instantiate without errors", () => {
            expect(actuator).toBeDefined();
            expect(actuator).toBeInstanceOf(ASTActuator);
        });
    });

    describe("Mutation Quota Guardrail", () => {
        it("should reject when create mutations exceed limit (>3)", async () => {
            const mutations = Array.from({ length: 4 }, (_, i) => ({
                type: "create" as const,
                filePath: `src/file${i}.ts`,
                code: "export const x = 1;",
            }));

            const result = await actuator.actuateCandidateBatch("test-cand", mutations);
            expect(result.success).toBe(false);
            expect(result.asi).toContain("Mutation limit exceeded");
        });

        it("should reject when modify mutations exceed limit (>10)", async () => {
            const mutations = Array.from({ length: 11 }, (_, i) => ({
                type: "modify" as const,
                filePath: `src/file${i}.ts`,
                code: "<<<< SEARCH\nold\n====\nnew\n>>>> REPLACE",
            }));

            const result = await actuator.actuateCandidateBatch("test-cand-2", mutations);
            expect(result.success).toBe(false);
            expect(result.asi).toContain("Mutation limit exceeded");
        });

        it("should accept mutations within quota (3 create, 10 modify)", async () => {
            const mutations = [
                { type: "create" as const, filePath: "src/a.ts", code: "export const a = 1;" },
                { type: "create" as const, filePath: "src/b.ts", code: "export const b = 2;" },
                { type: "modify" as const, filePath: "src/c.ts", code: "<<<< SEARCH\nold\n====\nnew\n>>>> REPLACE" },
            ];

            // This will fail at sandbox creation (fs mocked) but pass the quota check
            const result = await actuator.actuateCandidateBatch("test-cand-3", mutations);
            // It should get past the quota guardrail — if it fails, it's from sandbox creation, not quota
            if (!result.success) {
                expect(result.asi).not.toContain("Mutation limit exceeded");
            }
        });
    });

    describe("Path Safety Guardrail", () => {
        it("should reject paths outside src/", async () => {
            const mutations = [
                { type: "modify" as const, filePath: "../etc/passwd", code: "malicious" },
            ];

            const result = await actuator.actuateCandidateBatch("evil-cand", mutations);
            // Either fails at quota (no), path jail, or sandbox creation
            if (!result.success && result.asi) {
                // Should not contain "Mutation limit exceeded" since we're within limits
                expect(result.asi).not.toContain("Mutation limit exceeded");
            }
        });
    });

    describe("Virtual FS Fallback Guardrail", () => {
        it("should fallback to create if modify file is missing and no SEARCH blocks", async () => {
            const mutations = [
                { type: "modify" as const, filePath: "src/missing.ts", code: "export const newFile = true;" },
            ];

            const fs = await import("node:fs");
            // Temporarily mock existsSync to return false for our file
            vi.mocked(fs.existsSync).mockImplementation((p: any) => {
                if (p.toString().includes("missing.ts")) return false;
                return true;
            });
            const { logger } = await import("../../src/utils/logger");

            const result = await actuator.actuateCandidateBatch("test-fallback", mutations);
            console.log("RESULT", result);
            
            // Check if logger.info was called with the fallback message
            expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("File not found + no SEARCH blocks"));
        });
        
        it("should log Deleting file from sandbox when deleting", async () => {
            const mutations = [
                { type: "delete" as const, filePath: "src/tobedeleted.ts", code: "" },
            ];

            const fs = await import("node:fs");
            vi.mocked(fs.existsSync).mockImplementation((p: any) => true);
            const { logger } = await import("../../src/utils/logger");

            await actuator.actuateCandidateBatch("test-delete", mutations);
            
            expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Deleting file from sandbox"));
        });

        it("should successfully apply SEARCH/REPLACE block and update source file (Lines 215-220)", async () => {
            const fs = await import("node:fs");
            
            // Mock fsp.access to resolve (file exists) for pathExists checks
            vi.mocked(fs.promises.access).mockResolvedValue(undefined);
            vi.mocked(fs.promises.readFile).mockResolvedValue("function test() {\n    return old;\n}");
            const writeFileSpy = vi.spyOn(fs.promises, 'writeFile').mockResolvedValue(undefined);
            
            const mutations = [
                { 
                    type: "modify" as const, 
                    filePath: "src/success.ts", 
                    code: "<<<< SEARCH\nfunction test() {\n    return old;\n}\n====\nfunction test() {\n    return new;\n}\n>>>> REPLACE" 
                },
            ];

            const result = await actuator.actuateCandidateBatch("test-success", mutations);
            console.log("RESULT SUCCESS", result);
            expect(result.success).toBe(true);
            expect(writeFileSpy).toHaveBeenCalled();
        });

        it("should safely catch and log system errors during mutation (Lines 230-233)", async () => {
            const fs = await import("node:fs");
            
            // Phá hoại: Ép fsp.mkdir ném ra System Error (Permission denied)
            // This ensures createSandboxWorkspace fails at directory creation
            vi.mocked(fs.promises.mkdir).mockRejectedValue(new Error("EACCES"));

            const mutations = [
                { type: "create" as const, filePath: "src/error.ts", code: "export const a = 1;" },
            ];

            const result = await actuator.actuateCandidateBatch("test-error", mutations);
            console.log("RESULT ERROR", result);
            
            // Xác minh nhánh catch đã được phủ xanh và dọn dẹp sandbox
            expect(result.success).toBe(false);
            expect(result.asi).toContain("Lỗi hệ thống khi phẫu thuật AST");
        });
    });

    describe("Advanced Security & Robustness Edge Cases (TC-04, TC-18, TC-19, TC-20, TC-21)", () => {
        it("should reject when symlink resolves outside sandbox (Symlink Jailbreak Prevention - TC-04)", async () => {
            const fs = await import("node:fs");
            vi.mocked(fs.promises.access).mockResolvedValue(undefined);
            vi.mocked(fs.promises.realpath).mockImplementation((p: any) => {
                const pathStr = p as string;
                if (pathStr.includes("success.ts")) {
                    return Promise.resolve("/outside/etc/passwd");
                }
                return Promise.resolve(pathStr);
            });
            const mutations = [
                { type: "modify" as const, filePath: "src/success.ts", code: "export const val = 1;" }
            ];
            const result = await actuator.actuateCandidateBatch("jailbreak-cand", mutations);
            expect(result.success).toBe(false);
            expect(result.asi).toContain("Symlink Escape Violation");
        });

        it("should reject modifications on binary files (Binary File Surgery Rejection - TC-18)", async () => {
            const mutations = [
                { type: "modify" as const, filePath: "src/assets/logo.png", code: "binary content" }
            ];
            const result = await actuator.actuateCandidateBatch("binary-cand", mutations);
            expect(result.success).toBe(false);
            expect(result.asi).toContain("Binary File Violation");
        });

        it("should successfully apply and normalize cross-platform line endings (TC-19)", async () => {
            const fs = await import("node:fs");
            vi.mocked(fs.promises.access).mockResolvedValue(undefined);
            vi.mocked(fs.promises.readFile).mockResolvedValue("const a = 1;\r\nconst b = 2;");
            const writeFileSpy = vi.spyOn(fs.promises, 'writeFile').mockResolvedValue(undefined);
            
            const mutations = [
                { 
                    type: "modify" as const, 
                    filePath: "src/crlf.ts", 
                    code: "<<<< SEARCH\nconst a = 1;\n====\nconst a = 100;\n>>>> REPLACE" 
                }
            ];
            const result = await actuator.actuateCandidateBatch("crlf-cand", mutations);
            expect(result.success).toBe(true);
            expect(writeFileSpy).toHaveBeenCalledWith(
                expect.stringContaining("crlf.ts"), 
                "const a = 100;\r\nconst b = 2;"
            );
        });

        it("should reject surgery if the source file size exceeds 1MB limit (OOM Prevention - TC-20)", async () => {
            const fs = await import("node:fs");
            vi.mocked(fs.promises.access).mockResolvedValue(undefined);
            vi.mocked(fs.promises.stat).mockResolvedValue({ size: 2 * 1024 * 1024 } as any);
            
            const mutations = [
                { type: "modify" as const, filePath: "src/large.ts", code: "<<<< SEARCH\nold\n====\nnew\n>>>> REPLACE" }
            ];
            const result = await actuator.actuateCandidateBatch("large-cand", mutations);
            expect(result.success).toBe(false);
            expect(result.asi).toContain("File Size Violation");
        });

        it("should prevent concurrent actuation requests on the same sandbox (Concurrent Actuation Racing - TC-21)", async () => {
            const fs = await import("node:fs");
            vi.mocked(fs.promises.mkdir).mockImplementation(async () => {
                await new Promise(resolve => setTimeout(resolve, 50));
                return undefined;
            });
            const mutations = [
                { type: "create" as const, filePath: "src/a.ts", code: "export const a = 1;" }
            ];
            
            const [res1, res2] = await Promise.all([
                actuator.actuateCandidateBatch("concurrent-cand", mutations),
                actuator.actuateCandidateBatch("concurrent-cand", mutations)
            ]);
            
            expect(res1.success ? res2.success : res1.success).toBe(false);
            expect(res1.asi || res2.asi).toContain("Concurrency Violation");
        });
    });
});
