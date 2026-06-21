import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================================
// DEEP MOCKING: Prevent any actual ML or DB initializations
// ============================================================
process.env.AI_PROVIDER = "local";
process.env.TELEGRAM_ALLOWED_IDS = "12345";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
        child: vi.fn().mockReturnThis()
    },
}));

vi.mock("../../src/core/UIController", () => {
    return {
        UIController: class {
            on = vi.fn();
            off = vi.fn();
            emit = vi.fn();
            start = vi.fn();
            removeListener = vi.fn();
            broadcastUIEvent = vi.fn();
            broadcastTTSAudio = vi.fn();
        }
    };
});

vi.mock("../../src/SkillRegistry", () => {
    return {
        SkillRegistry: class {
            registerLocalSkills = vi.fn().mockResolvedValue(undefined);
            getAllSkills = vi.fn().mockReturnValue([]);
            whitelist = { load: vi.fn(), getAll: vi.fn().mockReturnValue({}) };
            circuitBreaker = { getOpenCircuits: vi.fn().mockReturnValue(new Set()) };
            warmUpCache = vi.fn().mockResolvedValue(undefined);
            whitelistDispose = vi.fn();
        }
    };
});

vi.mock("../../src/MemoryManager", () => {
    return {
        MemoryManager: class {
            dispose = vi.fn();
            initialize = vi.fn().mockResolvedValue(undefined);
            getShortTermHistory = vi.fn().mockResolvedValue([]);
            getSessionState = vi.fn().mockResolvedValue("");
            getUserProfile = vi.fn().mockResolvedValue({});
            initUHM = vi.fn();
            markLastTurnReflected = vi.fn();
        }
    };
});

vi.mock("../../src/services/VoiceEngine", () => {
    return {
        VoiceEngine: class {
            speak = vi.fn().mockResolvedValue(true);
            pushTokens = vi.fn();
            flushTTS = vi.fn();
            preempt = vi.fn();
            destroy = vi.fn();
            on = vi.fn();
            off = vi.fn();
        }
    };
});

vi.mock("../../src/services/NemotronSTTService", () => {
    return {
        NemotronSTTService: class {
            initialize = vi.fn().mockResolvedValue(undefined);
            flush = vi.fn();
            destroy = vi.fn();
            on = vi.fn();
            off = vi.fn();
            pushAudioChunk = vi.fn();
            pushAudioChunkOnly = vi.fn();
            triggerTranscription = vi.fn();
            isCircuitOpen = vi.fn().mockReturnValue(false);
        }
    };
});


vi.mock("../../src/services/VADWorkerBridge", async () => {
    const { EventEmitter } = await import("node:events");
    return {
        VADWorkerBridge: class extends EventEmitter {
            initialize = vi.fn().mockResolvedValue(undefined);
            pushAudioSamples = vi.fn();
            dispose = vi.fn().mockResolvedValue(undefined);
            mute = vi.fn();
            unmute = vi.fn();
            isReady = true;
            isSpeaking = false;
        }
    };
});

vi.mock("../../src/memory/SensoryManager", () => ({
    SensoryManager: {
        getInstance: vi.fn().mockReturnValue({
            dispose: vi.fn(),
        }),
    },
}));

vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: vi.fn().mockReturnValue({
            dispose: vi.fn(),
            setVramGuardCheck: vi.fn(),
        }),
    },
}));

vi.mock("../../src/services/KokoroVoiceEngine", () => {
    return {
        KokoroVoiceEngine: class {
            pushTokens = vi.fn();
            destroy = vi.fn();
            preempt = vi.fn();
            on = vi.fn();
            off = vi.fn();
            flushTTS = vi.fn();
        }
    };
});


vi.mock("../../src/core/ZaloPolling", () => {
    return {
        ZaloPolling: class {
            static create = vi.fn().mockResolvedValue(new this());
            stop = vi.fn();
            start = vi.fn();
            on = vi.fn();
            off = vi.fn();
        }
    };
});

vi.mock("../../src/core/HeartbeatManager", () => {
    return {
        HeartbeatManager: class {
            static create = vi.fn().mockResolvedValue(new this());
            stop = vi.fn();
            start = vi.fn();
            on = vi.fn();
        }
    };
});

vi.mock("../../src/services/AppWatcherService", () => {
    return {
        AppWatcherService: class {
            static create = vi.fn().mockResolvedValue(new this());
            stop = vi.fn();
            start = vi.fn();
            on = vi.fn();
            setCallback = vi.fn();
        }
    };
});

vi.mock("../../src/skills/core/BrowserHarness", () => ({
    shutdownBrowserHarness: vi.fn().mockResolvedValue(undefined),
}));

const { watchCloseMock } = vi.hoisted(() => ({
    watchCloseMock: vi.fn()
}));

import fs from "fs";
vi.mock("fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("fs")>();
    return {
        ...actual,
        default: {
            ...actual,
            watch: vi.fn().mockReturnValue({ close: watchCloseMock }),
            existsSync: vi.fn((p) => String(p).includes("silero_vad.onnx") || String(p).includes("skills") ? true : false),
        },
        watch: vi.fn().mockReturnValue({ close: watchCloseMock }),
        existsSync: vi.fn((p) => String(p).includes("silero_vad.onnx") || String(p).includes("skills") ? true : false),
    };
});

vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
}));

import { DependencyContainer } from "../../src/core/bootstrap/DependencyContainer";
import { CoreKernel } from "../../src/core/CoreKernel";

describe("Presence Detection Integration Tests", () => {
    let kernel: CoreKernel;

    beforeEach(() => {
        vi.clearAllMocks();
        DependencyContainer.resetInstance();
        kernel = new CoreKernel();
    });

    afterEach(async () => {
        if (kernel) {
            await kernel.shutdown();
        }
    });

    it("should initialize PresenceDetector on CoreKernel construction", () => {
        expect(kernel.presenceDetector).toBeDefined();
        expect(kernel.presence).toBe("ACTIVE");
    });

    it("should update presence on 'presence_changed' event", () => {
        kernel.presenceDetector.emit("presence_changed", { presence: "AWAY" });
        expect(kernel.presence).toBe("AWAY");

        kernel.presenceDetector.emit("presence_changed", { presence: "ACTIVE" });
        expect(kernel.presence).toBe("ACTIVE");
    });

    it("should broadcast 'presence_changed' to UI", () => {
        const broadcastSpy = vi.spyOn(kernel.ui, "broadcastUIEvent");
        kernel.presenceDetector.emit("presence_changed", { presence: "AWAY" });
        expect(broadcastSpy).toHaveBeenCalledWith("presence_changed", { presence: "AWAY" });
    });

    it("should mute VoiceEngine audio buffer broadcast when AWAY", async () => {
        const broadcastSpy = vi.spyOn(kernel.ui, "broadcastTTSAudio");
        
        // Find the audio_buffer listener registered in CoreKernel constructor
        const voiceMock = (kernel as any).voiceEngine;
        const audioBufferHandler = voiceMock.on.mock.calls.find((call: any[]) => call[0] === "audio_buffer")[1];

        // 1. ACTIVE state -> audio should broadcast
        kernel.presence = "ACTIVE";
        audioBufferHandler(Buffer.from("audio-data"));
        expect(broadcastSpy).toHaveBeenCalledWith(Buffer.from("audio-data"));

        // 2. AWAY state -> audio should NOT broadcast
        broadcastSpy.mockClear();
        kernel.presence = "AWAY";
        audioBufferHandler(Buffer.from("audio-data"));
        expect(broadcastSpy).not.toHaveBeenCalled();
    });

    it("should ignore wake word trigger when AWAY", async () => {
        const broadcastSpy = vi.spyOn(kernel.ui, "broadcastUIEvent");
        const speakSpy = vi.spyOn((kernel as any).voiceEngine, "speak");

        // Find the wake_word_triggered listener
        const uiMock = kernel.ui as any;
        const wakeWordHandler = uiMock.on.mock.calls.find((call: any[]) => call[0] === "wake_word_triggered")[1];

        // 1. AWAY state -> ignore
        kernel.presence = "AWAY";
        wakeWordHandler();
        expect(broadcastSpy).not.toHaveBeenCalled();
        expect(speakSpy).not.toHaveBeenCalled();

        // 2. ACTIVE state -> process
        kernel.presence = "ACTIVE";
        wakeWordHandler();
        expect(broadcastSpy).toHaveBeenCalledWith("wake_word_detected", { trailingText: "" });
    });

    it("should redirect spoken responses to Telegram when AWAY", async () => {
        const broadcastSpy = vi.spyOn(kernel.ui, "broadcastUIEvent");
        const sendTextSpy = vi.spyOn(kernel.telegram, "sendText").mockResolvedValue(undefined as any);

        // 1. ACTIVE state -> speak locally
        kernel.presence = "ACTIVE";
        await kernel.agentLoop.onSpokenResponse!("Hello there");
        expect(broadcastSpy).toHaveBeenCalledWith("ai_spoken_response", { text: "Hello there" });
        expect(sendTextSpy).not.toHaveBeenCalled();

        // 2. AWAY state -> send to Telegram
        broadcastSpy.mockClear();
        kernel.presence = "AWAY";
        await kernel.agentLoop.onSpokenResponse!("Hello there");
        expect(broadcastSpy).not.toHaveBeenCalled();
        expect(sendTextSpy).toHaveBeenCalledWith("12345", "Hello there");
    });

    it("should gate stream start, stream chunk, thought chunk, recovery reset, system busy notifications, and latency masks when AWAY", async () => {
        const broadcastSpy = vi.spyOn(kernel.ui, "broadcastUIEvent");
        kernel.presence = "AWAY";

        // 1. Thinking Start/End should be gated
        broadcastSpy.mockClear();
        await kernel.agentLoop.onThinkingStart!();
        expect(broadcastSpy).not.toHaveBeenCalled();

        await kernel.agentLoop.onThinkingEnd!();
        expect(broadcastSpy).not.toHaveBeenCalled();

        // 2. Stream Start/Chunk/Thought Chunk/Recovery Reset should be gated
        await kernel.agentLoop.onStreamStart!();
        expect(broadcastSpy).not.toHaveBeenCalled();

        await kernel.agentLoop.onStreamChunk!("token");
        expect(broadcastSpy).not.toHaveBeenCalled();

        await kernel.agentLoop.onThoughtChunk!("thought");
        expect(broadcastSpy).not.toHaveBeenCalled();

        await kernel.agentLoop.onRecoveryReset!();
        expect(broadcastSpy).not.toHaveBeenCalled();

        // 3. System busy notifications gated
        await kernel.agentLoop.onSystemBusy!("Busy!");
        expect(broadcastSpy).not.toHaveBeenCalled();

        // 4. Latency masks gated
        kernel.agentLoop.onLatencyMask!("heavy_route");
        expect(broadcastSpy).not.toHaveBeenCalled();
    });
});
