/**
 * HeraCompass.test.ts — Test suite for HeraCompass
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as fs from "node:fs";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { HeraCompass } from "../../src/memory/HeraCompass";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

// Mock OpenAI
const mockCreate = vi.fn();
const mockOpenAI = {
    chat: {
        completions: {
            create: mockCreate
        }
    }
} as any;

// Mock fs & fsp
vi.mock("node:fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("node:fs")>();
    return {
        ...actual,
        existsSync: vi.fn(),
        mkdirSync: vi.fn(),
        readFileSync: vi.fn(),
        promises: {
            mkdir: vi.fn(),
            readFile: vi.fn(),
            writeFile: vi.fn(),
            rename: vi.fn(),
            access: vi.fn().mockResolvedValue(undefined),
        }
    };
});

describe("HeraCompass", () => {
    beforeEach(() => {
        vi.resetAllMocks();
        vi.useFakeTimers();
        // Reset singleton
        (HeraCompass as any).instance = undefined;
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    describe("Initialization", () => {
        it("should initialize synchronously (legacy) with existing data", () => {
            const mockData = [{
                insight_id: "123",
                tool_target: "TestTool",
                actionable_rule: "Rule 1",
                error_trace: "Error X",
                utility_score: 1,
                status: "Verified"
            }];
            
            (fs.existsSync as any).mockReturnValue(true);
            (fs.readFileSync as any).mockReturnValue(JSON.stringify(mockData));

            const compass = HeraCompass.getInstance();
            expect(compass).toBeDefined();
            expect(fs.readFileSync).toHaveBeenCalled();

            const insights = (compass as any).insights;
            expect(insights.length).toBe(1);
            expect(insights[0].insight_id).toBe("123");
        });

        it("should initialize synchronously (legacy) with no existing data (Line 82)", () => {
            (fs.existsSync as any).mockImplementation((p: string) => p !== path.join(process.cwd(), "data", "agents", "test-agent", "hera_insights.json"));
            
            const compass = HeraCompass.getInstance();
            const insights = (compass as any).insights;
            expect(insights.length).toBe(0);
        });

        it("should handle sync init failure gracefully", () => {
            (fs.existsSync as any).mockReturnValue(true);
            (fs.readFileSync as any).mockImplementation(() => { throw new Error("File error"); });

            const compass = HeraCompass.getInstance();
            const insights = (compass as any).insights;
            expect(insights.length).toBe(0); // Failed to load, defaults to []
        });

        it("should initialize asynchronously (v4.0)", async () => {
            const mockData = [{
                insight_id: "async-1",
                tool_target: "TestTool",
                actionable_rule: "Rule Async",
                error_trace: "Error Y",
                utility_score: 0,
                status: "Draft"
            }];
            
            (fsp.mkdir as any).mockResolvedValue(undefined);
            (fsp.readFile as any).mockResolvedValue(JSON.stringify(mockData));

            const compass = await HeraCompass.create();
            expect(fsp.readFile).toHaveBeenCalled();

            const insights = (compass as any).insights;
            expect(insights.length).toBe(1);
            expect(insights[0].insight_id).toBe("async-1");
        });

        it("should handle async init failure gracefully (Line 68)", async () => {
            // Mock mkdir to throw so it reaches the outer catch block
            (fsp.mkdir as any).mockRejectedValue(new Error("Disk error"));

            const compass = await HeraCompass.create();
            const insights = (compass as any).insights;
            expect(insights.length).toBe(0);
        });
    });

    describe("learnFromError", () => {
        let compass: HeraCompass;

        beforeEach(async () => {
            (fsp.readFile as any).mockRejectedValue(new Error("No file"));
            compass = await HeraCompass.create();
        });

        it("should return null if error trace is too short", async () => {
            const result = await compass.learnFromError(mockOpenAI, "Tool", "Context", "short");
            expect(result).toBeNull();
            expect(mockCreate).not.toHaveBeenCalled();
        });

        it("should parse RULE correctly and save draft insight", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "RULE: Do not use undefined variables" } }]
            });

            const resultId = await compass.learnFromError(
                mockOpenAI, 
                "BashTool", 
                "Ran bash script", 
                "ReferenceError: x is not defined"
            );

            expect(resultId).toBeDefined();
            expect(resultId).not.toBeNull();
            expect(mockCreate).toHaveBeenCalledTimes(1);

            const insights = (compass as any).insights;
            expect(insights.length).toBe(1);
            expect(insights[0].actionable_rule).toBe("Do not use undefined variables");
            expect(insights[0].status).toBe("Draft");
            expect(insights[0].tool_target).toBe("BashTool");
        });

        it("should handle OpenAI bad response format", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "I think you should avoid errors." } }] // No RULE: keyword
            });

            const result = await compass.learnFromError(
                mockOpenAI, 
                "BashTool", 
                "Ran bash script", 
                "ReferenceError: x is not defined"
            );

            expect(result).toBeNull();
            const insights = (compass as any).insights;
            expect(insights.length).toBe(0);
        });

        it("should catch API errors gracefully", async () => {
            mockCreate.mockRejectedValueOnce(new Error("API Down"));

            const result = await compass.learnFromError(
                mockOpenAI, 
                "BashTool", 
                "Ran bash script", 
                "ReferenceError: x is not defined"
            );

            expect(result).toBeNull();
        });
    });

    describe("getRelatedInsight", () => {
        let compass: HeraCompass;

        beforeEach(async () => {
            const mockData = [
                {
                    insight_id: "id-1",
                    tool_target: "WebTool",
                    actionable_rule: "Check network connection",
                    error_trace: "ECONNREFUSED when fetching",
                    utility_score: 5,
                    status: "Verified"
                },
                {
                    insight_id: "id-2",
                    tool_target: "BashTool",
                    actionable_rule: "Use absolute paths",
                    error_trace: "No such file or directory",
                    utility_score: -1, // Low score
                    status: "Draft"
                }
            ];
            (fsp.readFile as any).mockResolvedValue(JSON.stringify(mockData));
            compass = await HeraCompass.create();
        });

        it("should return empty if index is null or empty", async () => {
            (HeraCompass as any).instance = undefined;
            (fsp.readFile as any).mockResolvedValue("[]");
            const emptyCompass = await HeraCompass.create();
            
            const results = emptyCompass.getRelatedInsight("ECONNREFUSED", "WebTool");
            expect(results.length).toBe(0);
        });

        it("should return matching insights", () => {
            const results = compass.getRelatedInsight("ECONNREFUSED", "WebTool");
            expect(results.length).toBe(1);
            expect(results[0].insight_id).toBe("id-1");
        });

        it("should filter by minimum score", () => {
            // Search matches "directory" -> id-2
            const resultsHighMin = compass.getRelatedInsight("directory", "BashTool", { minScore: 0 });
            expect(resultsHighMin.length).toBe(0); // id-2 has score -1

            const resultsLowMin = compass.getRelatedInsight("directory", "BashTool", { minScore: -2 });
            expect(resultsLowMin.length).toBe(1);
            expect(resultsLowMin[0].insight_id).toBe("id-2");
        });

        it("should fallback if toolTarget does not strictly match but item has no tool_target", async () => {
            // Add a global insight
            (compass as any).insights.push({
                insight_id: "id-3",
                tool_target: "",
                actionable_rule: "Generic rule",
                error_trace: "Generic error",
                utility_score: 1,
                status: "Verified"
            });
            (compass as any).rebuildIndex();

            const results = compass.getRelatedInsight("Generic error", "UnknownTool");
            expect(results.length).toBe(1);
            expect(results[0].insight_id).toBe("id-3");
        });
    });

    describe("updateUtilityScore & Atomic Save", () => {
        let compass: HeraCompass;

        beforeEach(async () => {
            const mockData = [
                {
                    insight_id: "id-1",
                    tool_target: "WebTool",
                    actionable_rule: "Rule",
                    error_trace: "Error",
                    utility_score: 0,
                    status: "Draft"
                },
                {
                    insight_id: "id-delete",
                    tool_target: "WebTool",
                    actionable_rule: "Bad Rule",
                    error_trace: "Error",
                    utility_score: -1,
                    status: "Draft"
                }
            ];
            (fsp.readFile as any).mockResolvedValue(JSON.stringify(mockData));
            (fsp.writeFile as any).mockResolvedValue(undefined);
            (fsp.rename as any).mockResolvedValue(undefined);
            
            compass = await HeraCompass.create();
        });

        it("should increment score and set to Verified on success", () => {
            compass.updateUtilityScore("id-1", true);
            
            const insights = (compass as any).insights;
            const target = insights.find((i: any) => i.insight_id === "id-1");
            expect(target.utility_score).toBe(1);
            expect(target.status).toBe("Verified");
        });

        it("should decrement score on failure", () => {
            compass.updateUtilityScore("id-1", false);
            
            const insights = (compass as any).insights;
            const target = insights.find((i: any) => i.insight_id === "id-1");
            expect(target.utility_score).toBe(-1);
            expect(target.status).toBe("Draft");
        });

        it("should permanently delete insight if score drops <= -2", () => {
            compass.updateUtilityScore("id-delete", false); // Score drops from -1 to -2
            
            const insights = (compass as any).insights;
            const target = insights.find((i: any) => i.insight_id === "id-delete");
            expect(target).toBeUndefined(); // Deleted
            expect(insights.length).toBe(1);
        });

        it("should do nothing if insight not found", () => {
            compass.updateUtilityScore("non-existent", true);
            const insights = (compass as any).insights;
            expect(insights.length).toBe(2);
        });

        it("should trigger debounced atomic write", async () => {
            compass.updateUtilityScore("id-1", true);
            
            // Should not save immediately
            expect(fsp.writeFile).not.toHaveBeenCalled();

            // Fast forward 5000ms debounce timer
            vi.advanceTimersByTime(5000);
            
            // Advance microtasks for the async save callback
            await Promise.resolve();
            await Promise.resolve();

            expect(fsp.writeFile).toHaveBeenCalledTimes(1);
            expect(fsp.rename).toHaveBeenCalledTimes(1);
            
            // Verify tmp path and target path
            const renameCalls = (fsp.rename as any).mock.calls;
            expect(renameCalls[0][0]).toMatch(/\.tmp$/); // source
            expect(renameCalls[0][1]).not.toMatch(/\.tmp$/); // dest
        });
        
        it("should catch and log save errors", async () => {
            (fsp.writeFile as any).mockRejectedValueOnce(new Error("Disk full"));
            
            compass.updateUtilityScore("id-1", true);
            vi.advanceTimersByTime(5000);
            
            await Promise.resolve();
            await Promise.resolve();

            // Should fail but be caught, no unhandled rejection
            expect(fsp.writeFile).toHaveBeenCalled();
            expect(fsp.rename).not.toHaveBeenCalled(); // Skipping rename due to error
        });
    });
});
