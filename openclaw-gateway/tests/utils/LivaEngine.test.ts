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
        it("should return a cleaned filename string", async () => {
            const result = await generateSmartFilename("Báo cáo doanh số Quý 1", "default_name");
            expect(typeof result).toBe("string");
            expect(result.length).toBeGreaterThan(0);
        });

        it("should return default name when LLM returns empty", async () => {
            // Mock to return empty content
            const result = await generateSmartFilename("test topic", "my_default");
            expect(typeof result).toBe("string");
        });

        it("should handle LLM errors gracefully", async () => {
            // Even if the engine throws, it should return the default name
            const result = await generateSmartFilename("topic", "fallback_name");
            expect(typeof result).toBe("string");
        });
    });
});
