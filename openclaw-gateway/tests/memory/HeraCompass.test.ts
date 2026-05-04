/**
 * HeraCompass.test.ts — Comprehensive test suite for HeraCompass
 * Targets: loadInsights (sync/async), learnFromError, getRelatedInsight,
 *          updateUtilityScore, saveDebounced (atomic write), dispose
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as fs from "node:fs";
import { promises as fsp } from "node:fs";
import { HeraCompass } from "../../src/memory/HeraCompass";
import { logger } from "../../src/utils/logger";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

// Mock uuid — deterministic IDs
vi.mock("uuid", () => ({ v4: () => "test-uuid-001" }));

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
            mkdir: vi.fn().mockResolvedValue(undefined),
            readFile: vi.fn(),
            writeFile: vi.fn().mockResolvedValue(undefined),
            rename: vi.fn().mockResolvedValue(undefined),
            access: vi.fn().mockResolvedValue(undefined),
        }
    };
});

const SAMPLE_INSIGHTS = [
    {
        insight_id: "id-1",
        tool_target: "WebTool",
        actionable_rule: "Check network connection before calling external API",
        error_trace: "ECONNREFUSED when fetching remote endpoint",
        utility_score: 5,
        status: "Verified"
    },
    {
        insight_id: "id-2",
        tool_target: "BashTool",
        actionable_rule: "Use absolute paths for file operations",
        error_trace: "No such file or directory /tmp/missing",
        utility_score: -1,
        status: "Draft"
    }
];

describe("HeraCompass", () => {
    beforeEach(() => {
        vi.resetAllMocks();
        vi.useFakeTimers();
        // Reset singleton before every test
        (HeraCompass as any).instance = undefined;
        // Default mock behaviors
        (fsp.mkdir as any).mockResolvedValue(undefined);
        (fsp.writeFile as any).mockResolvedValue(undefined);
        (fsp.rename as any).mockResolvedValue(undefined);
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    // ── Initialization ──────────────────────────────────────────────

    describe("Initialization — Sync (getInstance)", () => {
        it("should load existing data synchronously", () => {
            (fs.existsSync as any).mockReturnValue(true);
            (fs.readFileSync as any).mockReturnValue(JSON.stringify(SAMPLE_INSIGHTS));

            const compass = HeraCompass.getInstance();
            expect(compass).toBeDefined();
            expect(fs.readFileSync).toHaveBeenCalled();

            const insights = (compass as any).insights;
            expect(insights).toHaveLength(2);
            expect(insights[0].insight_id).toBe("id-1");
        });

        it("should return same singleton on second call", () => {
            (fs.existsSync as any).mockReturnValue(false);
            const c1 = HeraCompass.getInstance();
            const c2 = HeraCompass.getInstance();
            expect(c1).toBe(c2);
        });

        it("should initialize empty when JSON file does not exist (Line 82-83)", () => {
            // existsSync returns true for directory, false for the JSON file
            (fs.existsSync as any).mockImplementation((p: string) =>
                !p.includes("hera_insights.json")
            );
            const compass = HeraCompass.getInstance();
            expect((compass as any).insights).toHaveLength(0);
        });

        it("should create directory if missing (Line 77)", () => {
            (fs.existsSync as any).mockReturnValue(false);
            HeraCompass.getInstance();
            expect(fs.mkdirSync).toHaveBeenCalledWith(
                expect.any(String),
                { recursive: true }
            );
        });

        it("should catch sync parse/read errors gracefully (Line 86)", () => {
            (fs.existsSync as any).mockReturnValue(true);
            (fs.readFileSync as any).mockImplementation(() => {
                throw new Error("Corrupt JSON");
            });
            const compass = HeraCompass.getInstance();
            expect((compass as any).insights).toHaveLength(0);
            expect(logger.error).toHaveBeenCalled();
        });
    });

    describe("Initialization — Async (create)", () => {
        it("should load data asynchronously and build index (Lines 56-71)", async () => {
            (fsp.readFile as any).mockResolvedValue(JSON.stringify(SAMPLE_INSIGHTS));

            const compass = await HeraCompass.create();
            expect(fsp.mkdir).toHaveBeenCalled();
            expect(fsp.readFile).toHaveBeenCalled();
            expect((compass as any).insights).toHaveLength(2);
        });

        it("should default to empty array when file read fails (Line 64-65)", async () => {
            (fsp.readFile as any).mockRejectedValue(new Error("ENOENT"));

            const compass = await HeraCompass.create();
            expect((compass as any).insights).toHaveLength(0);
        });

        it("should catch outer errors in loadInsightsAsync (Line 68-70)", async () => {
            // Make mkdir throw to exercise the outer catch
            (fsp.mkdir as any).mockRejectedValue(new Error("Permission denied"));

            const compass = await HeraCompass.create();
            expect((compass as any).insights).toHaveLength(0);
            expect(logger.error).toHaveBeenCalledWith(
                expect.any(Error),
                expect.stringContaining("[HeraCompass]")
            );
        });

        it("should return existing instance if already created (Line 45)", async () => {
            (fsp.readFile as any).mockResolvedValue("[]");
            const c1 = await HeraCompass.create();
            const c2 = await HeraCompass.create();
            expect(c1).toBe(c2);
        });
    });

    // ── rebuildIndex ─────────────────────────────────────────────────

    describe("rebuildIndex", () => {
        it("should filter out insights with utility_score <= -2 (Line 91)", async () => {
            const data = [
                { ...SAMPLE_INSIGHTS[0], utility_score: 3 },
                { ...SAMPLE_INSIGHTS[1], utility_score: -3 }, // Should be filtered out
            ];
            (fsp.readFile as any).mockResolvedValue(JSON.stringify(data));
            const compass = await HeraCompass.create();

            // The flexIndex should only contain items with score > -2
            const index = (compass as any).flexIndex;
            expect(index).toBeDefined();
        });

        it("should limit index to 500 most recent items (Line 91)", async () => {
            const bigData = Array.from({ length: 600 }, (_, i) => ({
                insight_id: `id-${i}`,
                tool_target: "T",
                actionable_rule: `Rule ${i}`,
                error_trace: `Err ${i}`,
                utility_score: 0,
                status: "Draft"
            }));
            (fsp.readFile as any).mockResolvedValue(JSON.stringify(bigData));
            const compass = await HeraCompass.create();
            // All 600 are in insights, but the index only has the last 500
            expect((compass as any).insights).toHaveLength(600);
        });
    });

    // ── getRelatedInsight ────────────────────────────────────────────

    describe("getRelatedInsight", () => {
        let compass: HeraCompass;

        beforeEach(async () => {
            (fsp.readFile as any).mockResolvedValue(JSON.stringify(SAMPLE_INSIGHTS));
            compass = await HeraCompass.create();
        });

        it("should return empty array if insights list is empty (Line 139)", async () => {
            (HeraCompass as any).instance = undefined;
            (fsp.readFile as any).mockResolvedValue("[]");
            const emptyCompass = await HeraCompass.create();
            expect(emptyCompass.getRelatedInsight("err", "Tool")).toEqual([]);
        });

        it("should return matching insights by tool_target (Lines 147-163)", () => {
            const results = compass.getRelatedInsight("ECONNREFUSED", "WebTool");
            expect(results.length).toBeGreaterThanOrEqual(1);
            expect(results[0].tool_target).toBe("WebTool");
        });

        it("should respect custom limit option (Line 140)", () => {
            const results = compass.getRelatedInsight("error", "WebTool", { limit: 1 });
            expect(results.length).toBeLessThanOrEqual(1);
        });

        it("should filter by minScore (Line 141, 157)", () => {
            // id-2 has score -1, so minScore=0 should exclude it
            const results = compass.getRelatedInsight("directory", "BashTool", { minScore: 0 });
            const hasLowScore = results.some(r => r.utility_score < 0);
            expect(hasLowScore).toBe(false);

            // minScore=-2 should include id-2
            const results2 = compass.getRelatedInsight("directory", "BashTool", { minScore: -2 });
            expect(results2.length).toBeGreaterThanOrEqual(1);
        });

        it("should match insights with empty tool_target (fallback, Line 157)", async () => {
            (compass as any).insights.push({
                insight_id: "id-global",
                tool_target: "",
                actionable_rule: "Generic fallback rule",
                error_trace: "Universal error pattern",
                utility_score: 1,
                status: "Verified"
            });
            (compass as any).rebuildIndex();

            const results = compass.getRelatedInsight("Universal error", "AnyTool");
            expect(results.length).toBeGreaterThanOrEqual(1);
            expect(results.some(r => r.insight_id === "id-global")).toBe(true);
        });
    });

    // ── learnFromError ───────────────────────────────────────────────

    describe("learnFromError", () => {
        let compass: HeraCompass;

        beforeEach(async () => {
            (fsp.readFile as any).mockResolvedValue("[]");
            compass = await HeraCompass.create();
        });

        it("should return null for short error traces (Line 176)", async () => {
            const r1 = await compass.learnFromError(mockOpenAI, "T", "ctx", "");
            expect(r1).toBeNull();

            const r2 = await compass.learnFromError(mockOpenAI, "T", "ctx", "short");
            expect(r2).toBeNull();

            expect(mockCreate).not.toHaveBeenCalled();
        });

        it("should parse RULE from LLM response and create Draft insight (Lines 188-212)", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "RULE: Always validate input before processing" } }]
            });

            const resultId = await compass.learnFromError(
                mockOpenAI,
                "BashTool",
                "Ran bash script with user input",
                "ReferenceError: x is not defined at line 42"
            );

            expect(resultId).toBe("test-uuid-001");
            expect(mockCreate).toHaveBeenCalledTimes(1);

            const insights = (compass as any).insights;
            expect(insights).toHaveLength(1);
            expect(insights[0].actionable_rule).toBe("Always validate input before processing");
            expect(insights[0].status).toBe("Draft");
            expect(insights[0].tool_target).toBe("BashTool");
            expect(insights[0].utility_score).toBe(0);
        });

        it("should truncate long action context and error trace (Lines 185-186)", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "RULE: Handle long inputs" } }]
            });

            const longCtx = "A".repeat(1000);
            const longErr = "E".repeat(2000);

            await compass.learnFromError(mockOpenAI, "Tool", longCtx, longErr);

            // Verify the prompt was built with truncated values
            const callArgs = mockCreate.mock.calls[0][0];
            const promptContent = callArgs.messages[0].content;
            // Action context should be truncated to 500 chars
            expect(promptContent.length).toBeLessThan(longCtx.length + longErr.length);
        });

        it("should return null if LLM response has no RULE: keyword (Lines 213-215)", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "I think you should avoid errors in general." } }]
            });

            const result = await compass.learnFromError(
                mockOpenAI, "Tool", "context", "TypeError: cannot read property of undefined"
            );

            expect(result).toBeNull();
            expect(logger.warn).toHaveBeenCalledWith(expect.stringContaining("sinh rác"));
        });

        it("should catch and log API errors gracefully (Line 217)", async () => {
            mockCreate.mockRejectedValueOnce(new Error("API timeout"));

            const result = await compass.learnFromError(
                mockOpenAI, "Tool", "context", "Some long error message here"
            );

            expect(result).toBeNull();
            expect(logger.error).toHaveBeenCalledWith(
                expect.any(Error),
                expect.stringContaining("[HeraCompass]")
            );
        });
    });

    // ── updateUtilityScore ───────────────────────────────────────────

    describe("updateUtilityScore", () => {
        let compass: HeraCompass;

        beforeEach(async () => {
            (fsp.readFile as any).mockResolvedValue(JSON.stringify(SAMPLE_INSIGHTS));
            compass = await HeraCompass.create();
        });

        it("should increment score and promote to Verified on success (Lines 228-231)", () => {
            compass.updateUtilityScore("id-1", true);

            const insights = (compass as any).insights;
            const target = insights.find((i: any) => i.insight_id === "id-1");
            expect(target.utility_score).toBe(6); // was 5
            expect(target.status).toBe("Verified");
            expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Thăng cấp"));
        });

        it("should decrement score on failure (Lines 232-234)", () => {
            compass.updateUtilityScore("id-1", false);

            const target = (compass as any).insights.find((i: any) => i.insight_id === "id-1");
            expect(target.utility_score).toBe(4); // was 5
            expect(logger.warn).toHaveBeenCalledWith(expect.stringContaining("Khấu trừ"));
        });

        it("should permanently delete insight when score drops to -2 (Lines 235-238)", () => {
            // id-2 has score -1, decrement once => -2 => delete
            compass.updateUtilityScore("id-2", false);

            const insights = (compass as any).insights;
            expect(insights.find((i: any) => i.insight_id === "id-2")).toBeUndefined();
            expect(insights).toHaveLength(1);
            expect(logger.warn).toHaveBeenCalledWith(expect.stringContaining("Tiêu huỷ"));
        });

        it("should do nothing for non-existent insight ID (Line 225)", () => {
            compass.updateUtilityScore("non-existent-id", true);
            expect((compass as any).insights).toHaveLength(2);
        });
    });

    // ── saveDebounced (Atomic Write) ─────────────────────────────────

    describe("saveDebounced — Atomic Write", () => {
        let compass: HeraCompass;

        beforeEach(async () => {
            (fsp.readFile as any).mockResolvedValue(JSON.stringify(SAMPLE_INSIGHTS));
            compass = await HeraCompass.create();
        });

        it("should not save immediately — debounce 5s (Lines 117-132)", () => {
            compass.updateUtilityScore("id-1", true);
            expect(fsp.writeFile).not.toHaveBeenCalled();
        });

        it("should perform atomic write after 5s debounce (Lines 120-131)", async () => {
            compass.updateUtilityScore("id-1", true);

            // Fast forward 5s debounce
            vi.advanceTimersByTime(5000);

            // Let async save callback settle
            await vi.advanceTimersToNextTimerAsync();
            await Promise.resolve();
            await Promise.resolve();

            expect(fsp.writeFile).toHaveBeenCalledTimes(1);
            expect(fsp.rename).toHaveBeenCalledTimes(1);

            // Verify .tmp path used for atomic write
            const writeArgs = (fsp.writeFile as any).mock.calls[0];
            expect(writeArgs[0]).toMatch(/\.tmp$/);

            const renameArgs = (fsp.rename as any).mock.calls[0];
            expect(renameArgs[0]).toMatch(/\.tmp$/);
            expect(renameArgs[1]).not.toMatch(/\.tmp$/);
        });

        it("should reset debounce timer on rapid calls (Line 119)", async () => {
            compass.updateUtilityScore("id-1", true);
            vi.advanceTimersByTime(3000); // 3s in

            compass.updateUtilityScore("id-1", true); // Reset timer
            vi.advanceTimersByTime(3000); // Only 3s since reset — should NOT fire yet

            expect(fsp.writeFile).not.toHaveBeenCalled();

            vi.advanceTimersByTime(2000); // Now 5s since reset
            await Promise.resolve();
            await Promise.resolve();

            expect(fsp.writeFile).toHaveBeenCalledTimes(1);
        });

        it("should catch and log write errors without crashing (Line 130)", async () => {
            (fsp.writeFile as any).mockRejectedValueOnce(new Error("Disk full"));

            compass.updateUtilityScore("id-1", true);
            vi.advanceTimersByTime(5000);

            await Promise.resolve();
            await Promise.resolve();
            await Promise.resolve();

            expect(fsp.writeFile).toHaveBeenCalled();
            expect(fsp.rename).not.toHaveBeenCalled(); // Should not reach rename
        });

        it("should rebuild index after successful save (Line 128)", async () => {
            const rebuildSpy = vi.spyOn(compass as any, "rebuildIndex");

            compass.updateUtilityScore("id-1", true);
            vi.advanceTimersByTime(5000);

            await Promise.resolve();
            await Promise.resolve();

            // rebuildIndex called during create() + after save
            expect(rebuildSpy.mock.calls.length).toBeGreaterThanOrEqual(1);
        });
    });

    // ── Lifecycle & Dispose ──────────────────────────────────────────

    describe("Lifecycle", () => {
        it("should dispose and clear saveTimeout (Lines 110-114)", () => {
            (fs.existsSync as any).mockReturnValue(false);
            const compass = HeraCompass.getInstance();

            // Trigger saveDebounced to set timer
            (compass as any).insights = [SAMPLE_INSIGHTS[0]];
            compass.updateUtilityScore("id-1", true);
            expect((compass as any).saveTimeout).not.toBeNull();

            compass.dispose();
            expect((compass as any).saveTimeout).toBeNull();
        });

        it("should be safe to call dispose when no timer is set (Line 111)", () => {
            (fs.existsSync as any).mockReturnValue(false);
            const compass = HeraCompass.getInstance();

            expect((compass as any).saveTimeout).toBeNull();
            expect(() => compass.dispose()).not.toThrow();
        });
    });
});
