/**
 * ASTHealer.test.ts — AST Self-Healing & Diagnostic Facade Tests
 * ===============================================================
 * [AI_CONTEXT §4] Heavy ts-morph work is offloaded to ASTWorker (worker thread).
 * ASTHealer is now a thin async facade over ASTWorkerBridge, so these tests mock
 * the bridge (not ts-morph) and verify delegation + error handling.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const mockHealImports = vi.fn();
const mockGetDiagnostics = vi.fn();

vi.mock("../../src/core/ASTWorkerBridge", () => ({
    ASTWorkerBridge: {
        healImports: (sandboxRoot: string) => mockHealImports(sandboxRoot),
        getDiagnostics: (sandboxRoot: string) => mockGetDiagnostics(sandboxRoot),
    },
}));

const { ASTHealer } = await import("../../src/core/ASTHealer");
let healer: any;

describe("ASTHealer (worker-offloaded facade)", () => {
    beforeEach(() => {
        healer = new ASTHealer();
        vi.clearAllMocks();
    });

    describe("autoHealImportsOnSandbox()", () => {
        it("should delegate to the worker bridge and return its result", async () => {
            mockHealImports.mockResolvedValueOnce({ success: true, logs: "✅ healed" });

            const result = await healer.autoHealImportsOnSandbox("/sandbox/project");

            expect(mockHealImports).toHaveBeenCalledWith("/sandbox/project");
            expect(result.success).toBe(true);
            expect(result.logs).toContain("✅");
        });

        it("should return success and logs for partial healing (TC-01 Partial Success)", async () => {
            mockHealImports.mockResolvedValueOnce({ 
                success: true, 
                logs: "Healed: fileA.ts. Failed: fileB.ts (Syntax error on line 12)." 
            });

            const result = await healer.autoHealImportsOnSandbox("/sandbox/project");

            expect(result.success).toBe(true);
            expect(result.logs).toContain("Healed: fileA.ts");
            expect(result.logs).toContain("Failed: fileB.ts");
        });

        it("should surface worker failures as a graceful warning result", async () => {
            mockHealImports.mockRejectedValueOnce(new Error("AST worker crashed"));

            const result = await healer.autoHealImportsOnSandbox("/sandbox/broken");

            expect(result.success).toBe(false);
            expect(result.logs).toContain("Cảnh báo Healer");
            expect(result.logs).toContain("AST worker crashed");
        });
    });

    describe("getASIFromPreEmitDiagnosticsOnSandbox()", () => {
        it("should return empty string when worker reports no diagnostics", async () => {
            mockGetDiagnostics.mockResolvedValueOnce("");
            const result = await healer.getASIFromPreEmitDiagnosticsOnSandbox("/sandbox/clean");
            expect(mockGetDiagnostics).toHaveBeenCalledWith("/sandbox/clean");
            expect(result).toBe("");
        });

        it("should pass through the ASI report from the worker", async () => {
            const asi = "<actionable_side_information>\n- [File: test.ts] Dòng [42]: Type 'string' is not assignable to type 'number'\n</actionable_side_information>";
            mockGetDiagnostics.mockResolvedValueOnce(asi);
            const result = await healer.getASIFromPreEmitDiagnosticsOnSandbox("/sandbox/dirty");
            expect(result).toContain("<actionable_side_information>");
            expect(result).toContain("[File: test.ts] Dòng [42]");
        });

        it("should return a fatal-error ASI string when the worker throws", async () => {
            mockGetDiagnostics.mockRejectedValueOnce(new Error("Compilation crashed"));
            const result = await healer.getASIFromPreEmitDiagnosticsOnSandbox("/broken/path");
            expect(result).toContain("[ASI Engine Fatal Error]");
            expect(result).toContain("Compilation crashed");
        });
    });

    describe("Event Loop Non-blocking (TC-06)", () => {
        it("should execute asynchronously without blocking the host event loop", async () => {
            mockHealImports.mockImplementationOnce(async () => {
                return new Promise(resolve => {
                    setTimeout(() => resolve({ success: true, logs: "healed" }), 50);
                });
            });

            const startTime = Date.now();
            const promise = healer.autoHealImportsOnSandbox("/sandbox/async");

            let otherTaskExecuted = false;
            await new Promise<void>(resolve => {
                setTimeout(() => {
                    otherTaskExecuted = true;
                    resolve();
                }, 10);
            });

            const result = await promise;
            const duration = Date.now() - startTime;

            expect(result.success).toBe(true);
            expect(otherTaskExecuted).toBe(true);
            expect(duration).toBeGreaterThanOrEqual(40);
        });
    });

    describe("Edge Cases (TC-07, TC-08, TC-09)", () => {
        it("should handle concurrent autoHealImportsOnSandbox requests (TC-07)", async () => {
            mockHealImports.mockImplementation(async (path: string) => {
                return { success: true, logs: `healed ${path}` };
            });

            const paths = ["/sandbox/1", "/sandbox/2", "/sandbox/3", "/sandbox/4", "/sandbox/5"];
            const promises = paths.map(p => healer.autoHealImportsOnSandbox(p));
            const results = await Promise.all(promises);

            results.forEach((res, i) => {
                expect(res.success).toBe(true);
                expect(res.logs).toBe(`healed ${paths[i]}`);
            });
        });

        it("should handle worker thread timeout gracefully (TC-08)", async () => {
            mockGetDiagnostics.mockRejectedValueOnce(new Error("AST worker timed out after 10000ms"));

            const result = await healer.getASIFromPreEmitDiagnosticsOnSandbox("/sandbox/timeout");

            expect(result).toContain("[ASI Engine Fatal Error]");
            expect(result).toContain("timed out");
        });

        it("should reject invalid or unsafe paths early without calling worker bridge (TC-09)", async () => {
            const badPaths = ["", "   ", "/sandbox/../../etc/passwd", "..", "parent/../outside"];
            
            for (const badPath of badPaths) {
                const healRes = await healer.autoHealImportsOnSandbox(badPath);
                expect(healRes.success).toBe(false);
                expect(healRes.logs).toContain("không hợp lệ hoặc không an toàn");

                const diagRes = await healer.getASIFromPreEmitDiagnosticsOnSandbox(badPath);
                expect(diagRes).toContain("[ASI Engine Fatal Error]");
                expect(diagRes).toContain("không hợp lệ hoặc không an toàn");
            }

            // Đảm bảo không gọi xuống worker bridge với các đường dẫn xấu này
            expect(mockHealImports).not.toHaveBeenCalled();
            expect(mockGetDiagnostics).not.toHaveBeenCalled();
        });
    });
});
