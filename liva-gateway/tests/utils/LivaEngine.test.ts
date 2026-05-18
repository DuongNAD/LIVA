/**
 * LivaEngine.test.ts — Seal Token Validation & Security Tests
 * =============================================================
 * Tests: Branded type seal validation, security violations,
 * backward-compatible .chat.completions.create() interface.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock NativeIPCClient and OpenAI to avoid real connections
const mockCreate = vi.fn().mockResolvedValue({
    choices: [{ message: { content: "test_filename" } }]
});

vi.mock("../../src/utils/NativeIPCClient", () => {
    return {
        NativeIPCClient: class MockNativeIPCClient {
            chat = {
                completions: {
                    create: mockCreate
                }
            };
        }
    };
});

vi.mock("openai", () => ({
    default: vi.fn().mockImplementation(() => ({
        chat: {
            completions: {
                create: vi.fn().mockResolvedValue({
                    choices: [{ message: { content: "test_response" } }]
                })
            }
        }
    }))
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

const { livaEngine, generateSmartFilename } = await import("../../src/utils/LivaEngine");

describe("LivaEngine — Seal Token Validation", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    describe("secureChatCompletion()", () => {
        it("should succeed with valid seal token", async () => {
            const validSeal = livaEngine.getSeal();
            const result = await livaEngine.secureChatCompletion(
                { model: "test", messages: [{ role: "user", content: "hello" }] },
                validSeal
            );
            expect(result).toBeDefined();
        });

        it("should throw SECURITY VIOLATION with invalid seal token", async () => {
            const fakeSeal = "FAKE_TOKEN" as any;
            await expect(
                livaEngine.secureChatCompletion(
                    { model: "test", messages: [] },
                    fakeSeal
                )
            ).rejects.toThrow("SECURITY VIOLATION");
        });
    });

    describe("getSeal()", () => {
        it("should return a non-empty seal token", () => {
            const seal = livaEngine.getSeal();
            expect(seal).toBeTruthy();
            expect(typeof seal).toBe("string");
        });
    });

    describe("Backward-compatible .chat.completions.create()", () => {
        it("should expose chat.completions.create interface", () => {
            expect(livaEngine.chat).toBeDefined();
            expect(livaEngine.chat.completions).toBeDefined();
            expect(livaEngine.chat.completions.create).toBeDefined();
            expect(typeof livaEngine.chat.completions.create).toBe("function");
        });

        it("should auto-inject seal and call underlying engine", async () => {
            const result = await livaEngine.chat.completions.create({
                model: "router",
                messages: [{ role: "user", content: "test" }]
            });
            expect(result).toBeDefined();
        });
    });

    describe("generateSmartFilename()", () => {
        it("should return a cleaned filename string (no space — Line 109 true branch)", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "q1_revenue_report" } }]
            });
            const result = await generateSmartFilename("Báo cáo doanh số Quý 1", "default_name");
            expect(result).toBe("q1_revenue_report");
        });

        it("should handle aiName with spaces (Line 109 false/else branch)", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "revenue report summary" } }]
            });
            const result = await generateSmartFilename("topic", "default_name");
            expect(result).toBe("revenue_report_summary");
        });

        it("should return default name when LLM returns empty/undefined (Line 107 false)", async () => {
            mockCreate.mockResolvedValueOnce({
                choices: [{ message: { content: "" } }]
            });
            const result = await generateSmartFilename("test topic", "my_default");
            expect(result).toBe("my_default");
        });

        it("should return default name when choices are missing (Line 107 false)", async () => {
            mockCreate.mockResolvedValueOnce({ choices: [] });
            const result = await generateSmartFilename("test topic", "my_default");
            expect(result).toBe("my_default");
        });

        it("should catch SECURITY VIOLATION and log critical alert (Line 117 true)", async () => {
            const { logger } = await import("../../src/utils/logger");
            mockCreate.mockRejectedValueOnce(new Error("[LivaEngine] SECURITY VIOLATION: Unauthorized Seal Token provided."));
            const result = await generateSmartFilename("topic", "fallback_name");
            expect(result).toBe("fallback_name");
            expect(logger.error).toHaveBeenCalledWith(expect.stringContaining("CRITICAL SECURITY ALERT"));
        });

        it("should catch generic LLM errors gracefully (Line 117 false/else)", async () => {
            const { logger } = await import("../../src/utils/logger");
            mockCreate.mockRejectedValueOnce(new Error("Network timeout"));
            const result = await generateSmartFilename("topic", "fallback_name");
            expect(result).toBe("fallback_name");
            expect(logger.error).toHaveBeenCalledWith(expect.stringContaining("Smart Naming Error"));
        });
    });
});
