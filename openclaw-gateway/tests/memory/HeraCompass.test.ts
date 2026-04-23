/**
 * HeraCompass.test.ts — Error Self-Healing Database Tests
 * ========================================================
 * Tests:
 * - Insight retrieval (RAG) via FlexSearch
 * - Learning from errors (LLM insight extraction)
 * - Utility score decay & garbage collection
 * - JSON extraction from LLM output (jsonrepair)
 * - Debounced atomic persistence
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================================
// Mocks (must be before imports)
// ============================================================
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

// Mock fs to prevent real file I/O
vi.mock("fs", () => ({
    existsSync: vi.fn().mockReturnValue(false),
    mkdirSync: vi.fn(),
    readFileSync: vi.fn().mockReturnValue("[]"),
    promises: {
        writeFile: vi.fn().mockResolvedValue(undefined),
        rename: vi.fn().mockResolvedValue(undefined),
    },
}));

vi.mock("flexsearch", () => {
    class MockDocument {
        private items: any[] = [];
        constructor() {}
        add(item: any) { this.items.push(item); }
        search(query: string, limit: number) {
            // Return items matching the query in any field
            const matched = this.items.filter(i =>
                i.error_trace?.includes(query) ||
                i.actionable_rule?.includes(query) ||
                i.tool_target?.includes(query)
            ).slice(0, limit);
            // Return in FlexSearch Document format: [{ field, result: [ids] }]
            return [{
                field: "error_trace",
                result: matched.map(m => m.insight_id)
            }];
        }
    }
    return { Document: MockDocument };
});

vi.mock("uuid", () => ({
    v4: vi.fn().mockReturnValue("test-uuid-001"),
}));

vi.mock("jsonrepair", () => ({
    jsonrepair: vi.fn((str: string) => str),
}));

// ============================================================
// Tests
// ============================================================
import { HeraCompass, type HeraInsight } from "../../src/memory/HeraCompass";

// Reset the singleton between tests
function getHeraInstance(): HeraCompass {
    return HeraCompass.getInstance();
}

describe("HeraCompass — Error Self-Healing DB", () => {
    let hera: HeraCompass;

    beforeEach(() => {
        // Reset singleton by clearing the static instance
        (HeraCompass as any).instance = null;
        hera = getHeraInstance();
    });

    afterEach(() => {
        vi.clearAllTimers();
    });

    describe("Singleton", () => {
        it("should be a singleton", () => {
            const a = HeraCompass.getInstance();
            const b = HeraCompass.getInstance();
            expect(a).toBe(b);
        });
    });

    describe("getRelatedInsight", () => {
        it("should return empty array when no insights exist", () => {
            const result = hera.getRelatedInsight("some error", "web_search");
            expect(result).toEqual([]);
        });

        it("should return matching insights by tool_target", () => {
            // Manually inject insights into the instance
            const insights: HeraInsight[] = [
                {
                    insight_id: "id-1",
                    tool_target: "web_search",
                    actionable_rule: "Always check URL encoding",
                    error_trace: "ECONNREFUSED",
                    utility_score: 1,
                    status: "Verified"
                },
                {
                    insight_id: "id-2",
                    tool_target: "execute_command",
                    actionable_rule: "Use full path for executables",
                    error_trace: "ENOENT",
                    utility_score: 0,
                    status: "Draft"
                }
            ];
            (hera as any).insights = insights;
            (hera as any).rebuildIndex();

            const result = hera.getRelatedInsight("ECONNREFUSED", "web_search");
            expect(result.length).toBeGreaterThanOrEqual(1);
            expect(result[0].tool_target).toBe("web_search");
        });
    });

    describe("updateUtilityScore", () => {
        it("should increment score on success and mark as Verified", () => {
            const insights: HeraInsight[] = [{
                insight_id: "score-1",
                tool_target: "search",
                actionable_rule: "test rule",
                error_trace: "test error",
                utility_score: 0,
                status: "Draft"
            }];
            (hera as any).insights = insights;

            hera.updateUtilityScore("score-1", true);

            expect(insights[0].utility_score).toBe(1);
            expect(insights[0].status).toBe("Verified");
        });

        it("should decrement score on failure", () => {
            const insights: HeraInsight[] = [{
                insight_id: "score-2",
                tool_target: "search",
                actionable_rule: "test rule",
                error_trace: "test error",
                utility_score: 0,
                status: "Draft"
            }];
            (hera as any).insights = insights;

            hera.updateUtilityScore("score-2", false);

            expect(insights[0].utility_score).toBe(-1);
        });

        it("should garbage-collect insight when score drops to -2", () => {
            const insights: HeraInsight[] = [{
                insight_id: "garbage-1",
                tool_target: "search",
                actionable_rule: "bad rule",
                error_trace: "test error",
                utility_score: -1,
                status: "Draft"
            }];
            (hera as any).insights = insights;

            hera.updateUtilityScore("garbage-1", false);

            expect(insights.length).toBe(0);
        });

        it("should do nothing for non-existent insight ID", () => {
            (hera as any).insights = [];
            // Should not throw
            hera.updateUtilityScore("nonexistent", true);
        });
    });

    describe("learnFromError", () => {
        it("should extract and store an insight from LLM response", async () => {
            const mockAI: any = {
                chat: {
                    completions: {
                        create: vi.fn().mockResolvedValue({
                            choices: [{ message: { content: "RULE: Always validate URL before fetch" } }]
                        }),
                    },
                },
            };

            const result = await hera.learnFromError(
                mockAI,
                "web_search",
                "Searching for API data",
                "TypeError: Invalid URL format causing crash in line 42"
            );

            expect(result).toBe("test-uuid-001");
            expect((hera as any).insights.length).toBeGreaterThanOrEqual(1);
            const lastInsight = (hera as any).insights[(hera as any).insights.length - 1];
            expect(lastInsight.actionable_rule).toBe("Always validate URL before fetch");
            expect(lastInsight.status).toBe("Draft");
        });

        it("should return null for short error messages (< 10 chars)", async () => {
            const mockAI: any = {};
            const result = await hera.learnFromError(mockAI, "tool", "ctx", "err");
            expect(result).toBeNull();
        });

        it("should return null when LLM returns garbage (no RULE: prefix)", async () => {
            const mockAI: any = {
                chat: {
                    completions: {
                        create: vi.fn().mockResolvedValue({
                            choices: [{ message: { content: "I don't understand" } }]
                        }),
                    },
                },
            };

            const result = await hera.learnFromError(
                mockAI, "tool", "context data",
                "Some error that is long enough to process"
            );
            expect(result).toBeNull();
        });

        it("should not crash when LLM call fails", async () => {
            const mockAI: any = {
                chat: {
                    completions: {
                        create: vi.fn().mockRejectedValue(new Error("AI timeout")),
                    },
                },
            };

            const result = await hera.learnFromError(
                mockAI, "tool", "context",
                "Network error ECONNREFUSED 127.0.0.1:8000"
            );
            expect(result).toBeNull();
        });
    });
});
