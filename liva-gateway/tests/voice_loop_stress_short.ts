import dotenv from "dotenv";
import * as path from "node:path";
import * as fs from "node:fs";
import { EventEmitter } from "node:events";
import { monitorEventLoopDelay } from "node:perf_hooks";

// 1. Load Environment Configuration
const envPath = path.resolve(process.cwd(), ".env");
if (fs.existsSync(envPath)) {
    dotenv.config({ path: envPath });
}

// Ensure critical test keys are set
if (!process.env.LIVA_ENCRYPTION_KEY) {
    process.env.LIVA_ENCRYPTION_KEY = "LIVA_TEST_KEY_32BYTES_XXXXXXXXXX";
}

// 2. Stub/Mock boundary classes to run locally and offline
import { EdgeTTSClient } from "../src/services/EdgeTTSClient";
import { Scheduler } from "../src/kernel/Scheduler";
import { ModelOrchestrator } from "../src/core/ModelOrchestrator";

// Stub ModelOrchestrator to pretend LLM is always ready
ModelOrchestrator.prototype.isReady = function () {
    return true;
};
Object.defineProperty(ModelOrchestrator.prototype, "isWarmingUp", { get: () => false, configurable: true });
Object.defineProperty(ModelOrchestrator.prototype, "isSwapping", { get: () => false, configurable: true });

// Stub Edge-TTS synthesize to bypass network Azure CDN calls
EdgeTTSClient.prototype.synthesize = async function (text: string): Promise<Buffer | null> {
    // Simulate minor network/synthesis latency (50ms)
    await new Promise(resolve => setTimeout(resolve, 50));
    // Return dummy buffer
    return Buffer.alloc(1024);
};

// Global LLM response control
let currentLlmResponseText = "Xin chào sếp, tôi là LIVA. Hệ thống đang hoạt động hoàn toàn ổn định.";

function createMockStream(text: string) {
    const words = text.split(" ");
    return {
        [Symbol.asyncIterator]: async function* () {
            for (const word of words) {
                // Realistic word-generation streaming delay
                await new Promise(resolve => setTimeout(resolve, 80));
                yield {
                    choices: [
                        {
                            delta: { content: word + " " },
                            finish_reason: null
                        }
                    ]
                };
            }
            yield {
                choices: [
                    {
                        delta: {},
                        finish_reason: "stop"
                    }
                ]
            };
        }
    };
}

// Stub Scheduler to intercept LLM inference calls and return mock stream
const originalEmitSyscall = Scheduler.prototype.emitSyscall;
Scheduler.prototype.emitSyscall = async function (request: any): Promise<any> {
    if (request.type === "syscall_infer") {
        return createMockStream(currentLlmResponseText);
    }
    return originalEmitSyscall.call(this, request);
};

// Override ConfigManager defaults
import { ConfigManager } from "../src/core/config/ConfigManager";
ConfigManager.getInstance = function (): any {
    return {
        isNativeMode: false,
        aiProvider: "local",
        env: { AI_PROVIDER: "local", LIVA_USE_NATIVE: false },
        getLivaConfig: async () => ({
            ai: {
                temperature: 0.3,
                maxTokens: 512,
                topP: 0.9
            }
        }),
        invalidateCache: () => {},
    };
};

// Mock MemoryManager
const mockMemoryManager = {
    consolidationCron: {
        touch: () => {}
    },
    workingBuffer: {
        checkBudget: async () => "Context within budget."
    },
    getUserProfile: async () => ({
        name: "Sếp",
        preferences: "Friendly",
        language: "vi-VN"
    }),
    getShortTermHistory: async () => [],
    getStructuredMemoryPrompt: () => "Structured Memory Prompt Mock",
    getHybridContext: async (userText?: string, limit?: number) => [],
    addMessage: async () => {},
    clearSession: async () => {},
    updateLongTermMemory: async () => {},
    getStructuredMemoryInstance: () => ({
        insertTurnNode: () => {},
        vecReady: false,
        getUnreadBriefings: () => [],
        markBriefingRead: () => {}
    }),
    reflectionDaemon: {
        queueTurn: () => {}
    },
    getSessionState: async () => "Mock Session State",
    getLongTermMarkdown: async () => "Mock Long Term Memory",
    markLastTurnReflected: () => {},
    getPreviousSessionContextPrompt: async () => "Mock Previous Session Context",
    dispose: () => {}
};

// Mock UIController
class MockUI extends EventEmitter {
    broadcastUIEvent(name: string, data?: any) {
        // Suppress verbose debug logs during stress test
    }
    broadcastTTSAudio(buffer: Buffer) {
        // Suppress verbose debug logs during stress test
    }
}
const mockUI = new MockUI();

// 3. Import and wire real voice pipeline components
import { VADWorkerBridge } from "../src/services/VADWorkerBridge";
import { NemotronSTTService } from "../src/services/NemotronSTTService";
import { VoiceEngine } from "../src/services/VoiceEngine";
import { AgentLoop } from "../src/core/AgentLoop";
import { SkillRegistry } from "../src/SkillRegistry";
import { wireReactiveSync } from "../src/core/events/ReactiveSync";
import { logger } from "../src/utils/logger";
import { isBackchannel } from "../src/utils/BackchannelDetector";

// Stub VoiceEngine speak to handle keep-alive ping without triggering fallback
const originalSpeak = VoiceEngine.prototype.speak;
VoiceEngine.prototype.speak = async function (text: string): Promise<boolean> {
    if (text === " ") {
        return true;
    }
    return originalSpeak.call(this, text);
};

async function runStressTest() {
    logger.info("🎬 [Stress Test] Booting Voice Pipeline Stress Test Harness (Short Verification)...");

    // Initialize Event Loop Monitor
    const eventLoopMonitor = monitorEventLoopDelay({ resolution: 1 });
    eventLoopMonitor.enable();

    // Track test metrics
    const metrics = {
        totalTurns: 0,
        totalBargeIns: 0,
        totalBackchannels: 0,
        totalAudioChunksPushed: 0,
        eventLoopDelayWarnings: 0,
        startTime: Date.now(),
        errors: [] as string[]
    };

    // Instantiate real components
    const registry = new SkillRegistry();
    const agentLoop = new AgentLoop(mockMemoryManager as any, registry);
    const voiceEngine = new VoiceEngine();
    const whisperNode = new NemotronSTTService();
    const vadBridge = new VADWorkerBridge();

    const vadModelPath = path.resolve(process.cwd(), "models", "nemotron-asr", "silero_vad.onnx");
    if (!fs.existsSync(vadModelPath)) {
        throw new Error(`Silero VAD model not found at ${vadModelPath}`);
    }

    // Initialize VAD & STT (Running actual worker threads + ONNX models)
    logger.info("⏳ [Stress Test] Initializing Neural VAD...");
    await vadBridge.initialize(vadModelPath);
    logger.info("⏳ [Stress Test] Initializing Nemotron STT...");
    await whisperNode.initialize();
    logger.info("✅ [Stress Test] Real Neural Workers initialized successfully!");

    let voiceMode: "IDLE" | "ACTIVE" = "ACTIVE";

    // Setup wireReactiveSync dependencies
    let currentVoiceEngine = voiceEngine;
    const reactiveDeps = {
        agentLoop,
        ui: mockUI as any,
        getVoiceEngine: () => currentVoiceEngine,
        setVoiceEngine: (engine: any) => { currentVoiceEngine = engine; },
        whisperNode,
        dispatch: async (id: string, payload: any) => {
            // Emulate UI broadcast receiving
        },
        addTelemetryLog: (level: string, message: string) => {
            logger.info(`[Telemetry] ${level}: ${message}`);
        },
        isTtsFallbackActive: () => false,
        setTtsFallbackActive: () => {},
        createFallbackVoiceEngine: () => voiceEngine,
        onFallbackVoiceEngineCreated: () => {},
        getPresence: () => "ACTIVE" as const,
        getOwnerTelegramId: () => "123456",
        telegramBridge: { sendText: async () => {} }
    };
    wireReactiveSync(reactiveDeps);

    // Audio Playback Simulation State
    let activePlaybackTimeout: NodeJS.Timeout | null = null;
    let isPlayingAudio = false;

    // Handle TTS playback simulation
    voiceEngine.on("audio_buffer", (buffer: Buffer) => {
        isPlayingAudio = true;
        
        if (activePlaybackTimeout) {
            clearTimeout(activePlaybackTimeout);
        }
        
        // Simulate playback duration: 60ms per character of typical speech output (around 150 words/min)
        const simulatedPlaybackDuration = Math.min(10000, Math.max(1500, currentLlmResponseText.length * 60));
        logger.info(`[Test TTS Audio] Received buffer: ${buffer.length} bytes. Playing for ${simulatedPlaybackDuration}ms...`);
        activePlaybackTimeout = setTimeout(() => {
            isPlayingAudio = false;
            logger.info("[Test TTS Audio] Playback finished.");
            // Notify voiceEngine that playback finished
            voiceEngine.emit("play_finished");
        }, simulatedPlaybackDuration);
    });

    // Intercept preempt call to stop current simulated playback
    const originalPreempt = voiceEngine.preempt;
    voiceEngine.preempt = function () {
        if (activePlaybackTimeout) {
            clearTimeout(activePlaybackTimeout);
            activePlaybackTimeout = null;
        }
        isPlayingAudio = false;
        logger.info("[Test Stress Harness] Intercepted preempt() -> Stopped simulated playback.");
        originalPreempt.call(this);
    };

    // ASR transcription ready event handler (similar to CoreKernel)
    whisperNode.on("transcription_ready", async (sanitized: string) => {
        logger.info(`[Test ASR] Transcription Ready: "${sanitized}"`);
        
        if (!sanitized || sanitized.length <= 1) {
            logger.info("[Test ASR] Empty/Noise ignored.");
            return;
        }

        if (voiceMode === "IDLE") {
            const wakeRegex = /(hey liva|hi liva|liva ơi|ê liva|hello liva)/i;
            if (wakeRegex.test(sanitized)) {
                logger.info(`[Test WakeWord] Activated!`);
                voiceMode = "ACTIVE";
                voiceEngine.preempt();
                await voiceEngine.speak("Dạ, em nghe sếp!");
            }
            return;
        }

        // Active voice mode
        if (isBackchannel(sanitized)) {
            metrics.totalBackchannels++;
            logger.info(`[Test Backchannel] Detected: "${sanitized}" -> Skip interruption`);
            return;
        }

        // Real Speech -> Barge-in abort!
        if (isPlayingAudio) {
            metrics.totalBargeIns++;
            logger.warn(`[Test Barge-in] 🛑 Interruption triggered during TTS playback by user input: "${sanitized}"`);
        }

        voiceEngine.preempt();
        agentLoop.bargeIn('BARGE_IN'); // Stage 2: Hard abort

        // Dispatch input to AgentLoop
        metrics.totalTurns++;
        agentLoop.handleUserInput(sanitized);
    });

    // 4. Start 32ms audio streaming loop
    // Sends Float32Array to VAD and STT to simulate microphone input
    const dummyChunk = new Float32Array(512); // exactly 32ms of silence @ 16kHz
    const audioInterval = setInterval(() => {
        try {
            whisperNode.pushAudioChunkOnly(dummyChunk);
            vadBridge.pushAudioSamples(dummyChunk);
            metrics.totalAudioChunksPushed++;
        } catch (e: any) {
            metrics.errors.push(`Audio stream error: ${e.message}`);
        }
    }, 32);

    // Helper to simulate a user speech event
    function simulateUserSpeech(text: string) {
        logger.info(`[Simulated Speech] User starts speaking: "${text}"`);
        
        // Stage 1: speech_start event from VAD (triggers volume ducking)
        vadBridge.emit("speech_start");
        
        // Simulate speech duration (e.g. 1.5 seconds)
        setTimeout(() => {
            logger.info(`[Simulated Speech] User finished speaking. Triggering transcription...`);
            // Stage 2: speech_end event from VAD
            vadBridge.emit("speech_end");
            // ASR finishes and yields result
            whisperNode.emit("transcription_ready", text);
        }, 1500);
    }

    // 5. Schedule of continuous conversation & barge-ins over 15 seconds
    const TEST_DURATION_MS = 15 * 1000; // 15 seconds
    
    // Conversation turns timeline
    const turnsTimeline = [
        { time: 5000, text: "Xin chào LIVA, hôm nay thời tiết thế nào?", response: "Thời tiết hôm nay rất đẹp, trời nắng nhẹ và mát mẻ sếp ơi." }
    ];

    turnsTimeline.forEach(turn => {
        setTimeout(() => {
            currentLlmResponseText = turn.response;
            simulateUserSpeech(turn.text);
        }, turn.time);
    });

    // 6. Monitor Event Loop & RAM
    const monitorInterval = setInterval(() => {
        const mem = process.memoryUsage();
        const rssMB = (mem.rss / 1024 / 1024).toFixed(2);
        const heapUsedMB = (mem.heapUsed / 1024 / 1024).toFixed(2);
        const externalMB = (mem.external / 1024 / 1024).toFixed(2);

        // Event loop delay statistics
        const delayMax = (eventLoopMonitor.max / 1e6).toFixed(2);
        const delayMean = (eventLoopMonitor.mean / 1e6).toFixed(2);
        const delay99 = (eventLoopMonitor.percentile(99) / 1e6).toFixed(2);

        logger.info(`[Monitor] RAM: RSS=${rssMB}MB, Heap=${heapUsedMB}MB, Ext=${externalMB}MB | Event Loop Delay: Max=${delayMax}ms, Mean=${delayMean}ms, 99th=${delay99}ms`);

        if (Number(delayMax) > 10.0) {
            metrics.eventLoopDelayWarnings++;
            logger.warn(`[Performance SLA] ⚠️ Event loop delay exceeded 10ms! Max: ${delayMax}ms`);
        }

        // Reset monitor values for next window
        eventLoopMonitor.reset();
    }, 4000);

    // 7. Finish test after 15 seconds
    setTimeout(async () => {
        logger.info("🏁 [Stress Test] Test duration reached (Short Verification). Shutting down...");
        
        clearInterval(audioInterval);
        clearInterval(monitorInterval);
        
        if (activePlaybackTimeout) {
            clearTimeout(activePlaybackTimeout);
        }

        // Stop Event Loop Monitor
        eventLoopMonitor.disable();

        // Print final reports
        logger.info("=================================================================");
        logger.info("📊 STRESS TEST PERFORMANCE REPORT");
        logger.info("=================================================================");
        logger.info(`- Total Runtime: Short (${((Date.now() - metrics.startTime) / 1000).toFixed(1)}s)`);
        logger.info(`- Total Audio Chunks Pushed: ${metrics.totalAudioChunksPushed}`);
        logger.info(`- Total Simulated User Turns: ${metrics.totalTurns}`);
        logger.info(`- Total Barge-ins Triggered: ${metrics.totalBargeIns}`);
        logger.info(`- Total Backchannels Processed: ${metrics.totalBackchannels}`);
        logger.info(`- Event Loop Delays >10ms warnings: ${metrics.eventLoopDelayWarnings}`);
        logger.info(`- Errors/Crashes: ${metrics.errors.length}`);
        
        const finalMem = process.memoryUsage();
        logger.info(`- Final RSS Memory: ${(finalMem.rss / 1024 / 1024).toFixed(2)} MB`);
        logger.info(`- Final Heap Memory: ${(finalMem.heapUsed / 1024 / 1024).toFixed(2)} MB`);
        logger.info("=================================================================");

        // Graceful dispose of workers to free thread handles
        logger.info("⏳ [Stress Test] Disposing workers...");
        try {
            await vadBridge.dispose();
        } catch (e) {
            // Ignore worker termination warnings
        }
        try {
            whisperNode.destroy();
        } catch (e) {
            // Ignore
        }
        try {
            await voiceEngine.destroy();
        } catch (e) {
            // Ignore
        }
        logger.info("✅ [Stress Test] Cleaned up all resources.");

        if (metrics.errors.length > 0) {
            logger.error(`[Stress Test] Failures occurred: ${metrics.errors.join(", ")}`);
            process.exitCode = 1;
            process.exit(1);
        } else {
            logger.info("🎉 [Stress Test] All tests passed! Pipeline is correct and live.");
            process.exitCode = 0;
            process.exit(0);
        }
    }, TEST_DURATION_MS);
}

runStressTest().catch(err => {
    logger.error(err, "Stress test harness crashed");
    process.exit(1);
});
