import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import { ConfigManager } from "../../src/core/config/ConfigManager";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
        child: vi.fn().mockReturnThis()
    },
}));

describe("ConfigManager Fallback Tests", () => {
    let existsSpy: any;
    let readSpy: any;

    beforeEach(() => {
        ConfigManager.resetInstance();
        existsSpy = vi.spyOn(fs, "existsSync");
        readSpy = vi.spyOn(fs, "readFileSync");
    });

    afterEach(() => {
        vi.restoreAllMocks();
        ConfigManager.resetInstance();
    });

    it("should load valid configuration correctly", () => {
        const validJson = JSON.stringify({
            llm: { provider: "openai", model: "gpt-4" },
            stt: { provider: "whisper", language: "en" },
            tts: { provider: "openai-tts", voice: "alloy" }
        });

        existsSpy.mockImplementation((p: string) => p.includes("models.config.json"));
        readSpy.mockReturnValue(validJson);

        const config = ConfigManager.getInstance();
        expect(config.activeLlmProvider).toBe("openai");
        expect(config.activeSttProvider).toBe("whisper");
        expect(config.activeTtsProvider).toBe("openai-tts");
    });

    it("should fall back to defaults when models.config.json is missing", () => {
        existsSpy.mockReturnValue(false); // No config file found

        const config = ConfigManager.getInstance();
        expect(config.activeLlmProvider).toBe("gemma");
        expect(config.activeSttProvider).toBe("nemotron");
        expect(config.activeTtsProvider).toBe("edge-tts");
    });

    it("should fall back to defaults when models.config.json is malformed JSON", () => {
        existsSpy.mockImplementation((p: string) => p.includes("models.config.json"));
        readSpy.mockReturnValue("{ malformed json ");

        const config = ConfigManager.getInstance();
        expect(config.activeLlmProvider).toBe("gemma");
        expect(config.activeSttProvider).toBe("nemotron");
        expect(config.activeTtsProvider).toBe("edge-tts");
    });

    it("should fall back to defaults when fields are missing", () => {
        // Only llm is provided, stt and tts are missing
        const missingFieldsJson = JSON.stringify({
            llm: { provider: "openai", model: "gpt-4" }
        });

        existsSpy.mockImplementation((p: string) => p.includes("models.config.json"));
        readSpy.mockReturnValue(missingFieldsJson);

        const config = ConfigManager.getInstance();
        expect(config.activeLlmProvider).toBe("openai"); // provided
        expect(config.activeSttProvider).toBe("nemotron"); // default fallback
        expect(config.activeTtsProvider).toBe("edge-tts"); // default fallback
    });

    it("should fall back to defaults when provider types are invalid", () => {
        // llm.provider is a number, which violates z.string()
        const invalidProviderJson = JSON.stringify({
            llm: { provider: 12345, model: "gpt-4" },
            stt: { provider: "whisper", language: "en" },
            tts: { provider: "openai-tts", voice: "alloy" }
        });

        existsSpy.mockImplementation((p: string) => p.includes("models.config.json"));
        readSpy.mockReturnValue(invalidProviderJson);

        const config = ConfigManager.getInstance();
        expect(config.activeLlmProvider).toBe("gemma"); // default fallback due to Zod validation failure
        expect(config.activeSttProvider).toBe("nemotron"); // default fallback
        expect(config.activeTtsProvider).toBe("edge-tts"); // default fallback
    });
});
