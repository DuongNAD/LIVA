import { describe, it, expect, vi, beforeEach } from "vitest";
import { GemmaLLMProvider } from "../../src/providers/llm/GemmaLLMProvider";
import { LlamaLLMProvider } from "../../src/providers/llm/LlamaLLMProvider";
import { SenseVoiceSTTProvider } from "../../src/providers/stt/SenseVoiceSTTProvider";
import { NemotronSTTProvider } from "../../src/providers/stt/NemotronSTTProvider";
import { NativeIPCClient } from "../../src/utils/NativeIPCClient";

vi.mock("../../src/utils/NativeIPCClient", () => {
    const mockChatCreate = vi.fn();
    const mockHealthCheck = vi.fn().mockResolvedValue(true);
    const mockSwapModel = vi.fn().mockResolvedValue({ success: true, errorMessage: "", loadedModel: "new-model", swapDurationMs: 120 });
    const mockEmbed = vi.fn().mockResolvedValue({ data: [{ embedding: [0.1, 0.2], index: 0 }], model: "embedding", dimensions: 2 });
    const mockDestroy = vi.fn();

    const NativeIPCClient = vi.fn().mockImplementation(() => {
        return {
            chat: {
                completions: {
                    create: mockChatCreate
                }
            },
            healthCheck: mockHealthCheck,
            swapModel: mockSwapModel,
            embed: mockEmbed,
            destroy: mockDestroy
        };
    });

    return { NativeIPCClient, mockChatCreate, mockHealthCheck, mockSwapModel, mockEmbed, mockDestroy };
});

vi.mock("../../src/services/NemotronSTTService", () => {
    class NemotronSTTService {
        initialize = vi.fn().mockResolvedValue(undefined);
        pushAudioChunk = vi.fn();
        pushAudioChunkOnly = vi.fn();
        triggerTranscription = vi.fn();
        flush = vi.fn();
        destroy = vi.fn();
        isCircuitOpen = vi.fn().mockReturnValue(false);
    }
    return { NemotronSTTService };
});

describe("GemmaLLMProvider and LlamaLLMProvider", () => {
    let mockClient: any;
    let gemmaProvider: GemmaLLMProvider;
    let llamaProvider: LlamaLLMProvider;

    beforeEach(() => {
        vi.clearAllMocks();
        mockClient = new NativeIPCClient();
        gemmaProvider = new GemmaLLMProvider(mockClient);
        llamaProvider = new LlamaLLMProvider(mockClient);
    });

    it("should call chat.completions.create with model gemma", async () => {
        const mockResponse = { id: "1", object: "chat.completion", choices: [{ index: 0, message: { role: "assistant", content: "hello" }, finish_reason: "stop" }], model: "gemma" };
        mockClient.chat.completions.create.mockResolvedValue(mockResponse);

        const result = await gemmaProvider.chat.completions.create({ messages: [{ role: "user", content: "hi" }] });
        expect(result).toEqual(mockResponse);
        expect(mockClient.chat.completions.create).toHaveBeenCalledWith({
            messages: [{ role: "user", content: "hi" }],
            model: "gemma"
        }, undefined);
    });

    it("should call chat.completions.create with model llama", async () => {
        const mockResponse = { id: "1", object: "chat.completion", choices: [{ index: 0, message: { role: "assistant", content: "hello" }, finish_reason: "stop" }], model: "llama" };
        mockClient.chat.completions.create.mockResolvedValue(mockResponse);

        const result = await llamaProvider.chat.completions.create({ messages: [{ role: "user", content: "hi" }] });
        expect(result).toEqual(mockResponse);
        expect(mockClient.chat.completions.create).toHaveBeenCalledWith({
            messages: [{ role: "user", content: "hi" }],
            model: "llama"
        }, undefined);
    });

});

describe("SenseVoiceSTTProvider", () => {
    it("should simulate transcription dynamically based on audio length", async () => {
        const provider = new SenseVoiceSTTProvider();
        await provider.initialize();

        let partialEmitted = "";
        let finalEmitted = "";

        provider.on("transcription_partial", (text) => { partialEmitted = text; });
        provider.on("transcription_ready", (text) => { finalEmitted = text; });

        // Push 1 second of audio (16000 samples)
        const chunk = new Float32Array(16000);
        provider.pushAudioChunkOnly(chunk);

        expect(partialEmitted).toContain("[SenseVoice Partial: 1.0s]");

        provider.triggerTranscription();
        expect(finalEmitted).toContain("Simulated transcription for 1.00 seconds of audio.");
    });
});

describe("NemotronSTTProvider", () => {
    it("should instantiate and expose STT provider methods", () => {
        const provider = new NemotronSTTProvider();
        expect(provider.initialize).toBeDefined();
        expect(provider.pushAudioChunk).toBeDefined();
        expect(provider.isCircuitOpen).toBeDefined();
    });
});
