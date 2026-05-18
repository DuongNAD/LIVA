import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { NLCommandTranslator } from "../../src/core/NLCommandTranslator";

const mockCreate = vi.fn();

vi.mock("openai", () => {
    class MockOpenAI {
        chat = {
            completions: {
                create: mockCreate
            }
        };
    }
    return {
        OpenAI: MockOpenAI
    };
});

describe("NLCommandTranslator", () => {
    let translator: NLCommandTranslator;

    beforeEach(() => {
        vi.clearAllMocks();
        translator = new NLCommandTranslator();
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    describe("Initialization", () => {
        const originalEnv = process.env;

        beforeEach(() => {
            vi.resetModules();
            process.env = { ...originalEnv };
        });

        afterEach(() => {
            process.env = originalEnv;
        });

        it("should initialize with cloud configuration", () => {
            process.env.AI_PROVIDER = "cloud";
            process.env.AI_BASE_URL = "https://api.openai.com";
            process.env.AI_API_KEY = "test-key";
            const cloudTranslator = new NLCommandTranslator();
            expect(cloudTranslator).toBeDefined();
        });

        it("should initialize with default local configuration if env vars are missing", () => {
            delete process.env.AI_PROVIDER;
            delete process.env.ROUTER_PORT;
            delete process.env.ROUTER_MODEL_NAME;
            const defaultTranslator = new NLCommandTranslator();
            expect(defaultTranslator).toBeDefined();
        });
    });

    it("should correctly translate 'open file' intent", async () => {
        mockCreate.mockResolvedValueOnce({
            choices: [{
                message: {
                    content: JSON.stringify({
                        action: "open_file",
                        args: { filePath: "src/main.ts" },
                        confidence: 0.95,
                        reasoning: "User explicitly asked to open main.ts"
                    })
                }
            }]
        });

        const result = await translator.translate("Mở cho tôi file src/main.ts nhé");
        
        expect(result.action).toBe("open_file");
        expect(result.args.filePath).toBe("src/main.ts");
        expect(result.confidence).toBe(0.95);
        expect(mockCreate).toHaveBeenCalledTimes(1);
    });

    it("should correctly translate with context provided (Line 76)", async () => {
        mockCreate.mockResolvedValueOnce({
            choices: [{
                message: {
                    content: JSON.stringify({
                        action: "open_file",
                        args: { filePath: "src/index.ts" },
                        confidence: 0.9,
                        reasoning: "Context helped"
                    })
                }
            }]
        });

        const res = await translator.translate("open it", "I am working on src/index.ts");
        expect(res.action).toBe("open_file");
        expect(res.reasoning).toBe("Context helped");
        expect(mockCreate).toHaveBeenCalledTimes(1);
    });

    it("should correctly translate 'run terminal' intent", async () => {
        mockCreate.mockResolvedValueOnce({
            choices: [{
                message: {
                    content: JSON.stringify({
                        action: "run_terminal",
                        args: { command: "npm test" },
                        confidence: 0.9,
                        reasoning: "User wants to run tests"
                    })
                }
            }]
        });

        const result = await translator.translate("chạy npm test đi");
        
        expect(result.action).toBe("run_terminal");
        expect(result.args.command).toBe("npm test");
    });

    it("should return 'unknown' if LLM returns invalid JSON", async () => {
        mockCreate.mockResolvedValueOnce({
            choices: [{
                message: {
                    content: "This is not json"
                }
            }]
        });

        const result = await translator.translate("Làm gì đó đi");
        
        expect(result.action).toBe("unknown");
        expect(result.confidence).toBe(0);
        expect(result.reasoning).toContain("Unexpected token");
    });

    it("should return 'unknown' if structure is missing 'action'", async () => {
        mockCreate.mockResolvedValueOnce({
            choices: [{
                message: {
                    content: JSON.stringify({
                        args: { foo: "bar" },
                        confidence: 0.9
                    })
                }
            }]
        });

        const result = await translator.translate("Làm gì đó đi");
        
        expect(result.action).toBe("unknown");
        expect(result.reasoning).toBe("Invalid JSON structure from LLM");
    });

    it("should gracefully handle API failure", async () => {
        mockCreate.mockRejectedValueOnce(new Error("Network Timeout"));

        const result = await translator.translate("Mở file");
        
        expect(result.action).toBe("unknown");
        expect(result.reasoning).toBe("Network Timeout");
    
    });
});