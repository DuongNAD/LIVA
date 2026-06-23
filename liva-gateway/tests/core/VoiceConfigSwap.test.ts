import { describe, it, expect, vi, beforeAll, afterAll } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { EventEmitter } from "node:events";
import { VoiceOrchestrator } from "../../src/core/orchestrators/VoiceOrchestrator";
import { ConfigManager } from "../../src/core/config/ConfigManager";
import { EdgeTTSProvider } from "../../src/providers/tts/EdgeTTSProvider";
import { KokoroTTSProvider } from "../../src/providers/tts/KokoroTTSProvider";
import { NemotronSTTProvider } from "../../src/providers/stt/NemotronSTTProvider";
import { SenseVoiceSTTProvider } from "../../src/providers/stt/SenseVoiceSTTProvider";

// Mock the providers to avoid running actual backend models or workers
vi.mock("../../src/providers/tts/EdgeTTSProvider", () => {
    const { EventEmitter } = require("node:events");
    class MockEdge extends EventEmitter {
        speak = vi.fn().mockResolvedValue(true);
        pushTokens = vi.fn();
        flushTTS = vi.fn();
        preempt = vi.fn();
        destroy = vi.fn().mockResolvedValue(undefined);
    }
    return { EdgeTTSProvider: MockEdge };
});

vi.mock("../../src/providers/tts/KokoroTTSProvider", () => {
    const { EventEmitter } = require("node:events");
    class MockKokoro extends EventEmitter {
        speak = vi.fn().mockResolvedValue(true);
        pushTokens = vi.fn();
        flushTTS = vi.fn();
        preempt = vi.fn();
        destroy = vi.fn().mockResolvedValue(undefined);
    }
    return { KokoroTTSProvider: MockKokoro };
});

vi.mock("../../src/providers/stt/NemotronSTTProvider", () => {
    const { EventEmitter } = require("node:events");
    class MockNemotron extends EventEmitter {
        initialize = vi.fn().mockResolvedValue(undefined);
        pushAudioChunk = vi.fn();
        pushAudioChunkOnly = vi.fn();
        triggerTranscription = vi.fn();
        flush = vi.fn();
        destroy = vi.fn();
        isCircuitOpen = vi.fn().mockReturnValue(false);
    }
    return { NemotronSTTProvider: MockNemotron };
});

vi.mock("../../src/providers/stt/SenseVoiceSTTProvider", () => {
    const { EventEmitter } = require("node:events");
    class MockSenseVoice extends EventEmitter {
        initialize = vi.fn().mockResolvedValue(undefined);
        pushAudioChunk = vi.fn();
        pushAudioChunkOnly = vi.fn();
        triggerTranscription = vi.fn();
        flush = vi.fn();
        destroy = vi.fn();
        isCircuitOpen = vi.fn().mockReturnValue(false);
    }
    return { SenseVoiceSTTProvider: MockSenseVoice };
});

describe("Configuration-Driven Provider Swapping Integration Test", () => {
    const configPath = path.join(process.cwd(), "..", "data", "models.config.json");
    let originalConfigContent = "";

    beforeAll(() => {
        // Read and backup the original models.config.json
        if (fs.existsSync(configPath)) {
            originalConfigContent = fs.readFileSync(configPath, "utf8");
        } else {
            throw new Error(`Original config path not found: ${configPath}`);
        }
    });

    afterAll(() => {
        // Restore original models.config.json content
        if (originalConfigContent) {
            fs.writeFileSync(configPath, originalConfigContent, "utf8");
        }
        ConfigManager.resetInstance();
    });

    it("should resolve Nemotron STT and Edge TTS when config specifies them", async () => {
        // 1. Write Nemotron and Edge-TTS config
        const testConfig = {
            llm: {
                provider: "gemma",
                model: "gemma-4-26B-A4B-it-UD-Q6_K.gguf"
            },
            stt: {
                provider: "nemotron",
                language: "vi"
            },
            tts: {
                provider: "edge-tts",
                voice: "default"
            }
        };
        fs.writeFileSync(configPath, JSON.stringify(testConfig, null, 2), "utf8");

        // 2. Reset singleton to force re-reading the config file
        ConfigManager.resetInstance();
        const configManager = ConfigManager.getInstance();

        // 3. Verify ConfigManager resolved values
        expect(configManager.activeSttProvider).toBe("nemotron");
        expect(configManager.activeTtsProvider).toBe("edge-tts");

        // 4. Initialize VoiceOrchestrator and verify correct provider class instantiation
        const orchestrator = new VoiceOrchestrator();
        await orchestrator.initialize(null);

        expect(orchestrator.whisperNode).toBeInstanceOf(NemotronSTTProvider);
        expect(orchestrator.voiceEngine).toBeInstanceOf(EdgeTTSProvider);

        await orchestrator.dispose();
    });

    it("should resolve SenseVoice STT and Kokoro TTS when config specifies them", async () => {
        // 1. Write SenseVoice and Kokoro config
        const testConfig = {
            llm: {
                provider: "gemma",
                model: "gemma-4-26B-A4B-it-UD-Q6_K.gguf"
            },
            stt: {
                provider: "sensevoice",
                language: "vi"
            },
            tts: {
                provider: "kokoro",
                voice: "default"
            }
        };
        fs.writeFileSync(configPath, JSON.stringify(testConfig, null, 2), "utf8");

        // 2. Reset singleton to force re-reading the config file
        ConfigManager.resetInstance();
        const configManager = ConfigManager.getInstance();

        // 3. Verify ConfigManager resolved values
        expect(configManager.activeSttProvider).toBe("sensevoice");
        expect(configManager.activeTtsProvider).toBe("kokoro");

        // 4. Initialize VoiceOrchestrator and verify correct provider class instantiation
        const orchestrator = new VoiceOrchestrator();
        await orchestrator.initialize(null);

        expect(orchestrator.whisperNode).toBeInstanceOf(SenseVoiceSTTProvider);
        expect(orchestrator.voiceEngine).toBeInstanceOf(KokoroTTSProvider);

        await orchestrator.dispose();
    });
});
