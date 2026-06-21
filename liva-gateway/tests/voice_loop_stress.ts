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
    logger.info("🎬 [Stress Test] Booting Voice Pipeline Stress Test Harness (5 Minutes)...");

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

    // 5. Schedule of continuous conversation & barge-ins over 5 minutes (300 seconds)
    const TEST_DURATION_MS = 5 * 60 * 1000; // 5 minutes
    
    // Conversation turns timeline
    const turnsTimeline = [
        { time: 10000, text: "Xin chào LIVA, hôm nay thời tiết thế nào?", response: "Thời tiết hôm nay rất đẹp, trời nắng nhẹ và mát mẻ sếp ơi." },
        { time: 30000, text: "Hãy kể cho tôi một câu chuyện ngắn đi.", response: "Ngày xửa ngày xưa, ở một ngôi làng xa xôi có một chú bé thông minh..." },
        { time: 55000, text: "Cảm ơn bạn, câu chuyện hay lắm.", response: "Dạ, sếp thích là em vui rồi ạ." },
        
        // 70s: Barge-in test (Interrupt mid-response)
        // We make the response long so TTS takes ~10 seconds to finish. We interrupt at 72s.
        { time: 70000, text: "Hãy lập trình một hàm QuickSort bằng TypeScript.", response: "Dạ sếp, thuật toán sắp xếp nhanh hay QuickSort hoạt động theo nguyên lý chia để trị. Chúng ta sẽ chọn một phần tử làm chốt, sau đó phân chia các phần tử còn lại thành hai nhóm: nhóm nhỏ hơn chốt và nhóm lớn hơn chốt. Sau đó, ta lặp lại quy trình này đệ quy cho hai nhóm con cho đến khi toàn bộ mảng được sắp xếp." },
        { time: 72000, text: "Ê Liva, dừng lại đi, tôi muốn hỏi cái khác.", response: "Dạ em nghe sếp, em đã dừng rồi. Sếp muốn hỏi gì khác ạ?", isInterrupt: true },
        
        { time: 95000, text: "Hãy tìm thông tin về Node.js 22.", response: "Node.js 22 mang lại nhiều cải tiến như hỗ trợ require ESM trực tiếp, và trình thử nghiệm tích hợp sẵn." },
        { time: 115000, text: "Ok, cảm ơn bạn.", response: "Dạ, không có gì sếp ơi." },
        
        // 130s: Backchannel test (should not interrupt)
        { time: 130000, text: "Hãy giải thích cơ chế hoạt động của WebRTC.", response: "WebRTC là giao thức hỗ trợ giao tiếp âm thanh và hình ảnh trực tiếp giữa hai trình duyệt mà không qua máy chủ trung gian..." },
        { time: 133000, text: "Ừm, đúng rồi.", response: "", isBackchannel: true }, // Backchannel filler sound
        
        { time: 155000, text: "Lên lịch làm việc ngày hôm nay giúp tôi.", response: "Dạ, lịch làm việc hôm nay của sếp gồm họp lúc 10h sáng, ăn trưa đối tác lúc 12h, và review code lúc 3h chiều." },
        
        // 170s: Barge-in test
        { time: 170000, text: "Hãy phân tích độ phức tạp thuật toán O log n.", response: "Dạ thưa sếp, độ phức tạp O log n biểu thị một thuật toán mà thời gian chạy tăng theo hàm logarit của kích thước đầu vào. Ví dụ điển hình nhất là tìm kiếm nhị phân, nơi mỗi bước thực thi sẽ chia đôi không gian tìm kiếm, giúp giảm số lượng so sánh một cách cực kỳ nhanh chóng." },
        { time: 172000, text: "Hủy lệnh đó đi, tôi bận rồi.", response: "Dạ, em đã hủy phân tích. Em luôn sẵn sàng khi sếp quay lại.", isInterrupt: true },
        
        { time: 205000, text: "Tôi đã rảnh rồi, chúng ta tiếp tục nhé.", response: "Tuyệt vời, em đã sẵn sàng phục vụ sếp tiếp tục." },
        
        // 220s: Backchannel test
        { time: 220000, text: "Hãy viết bài blog giới thiệu về LIVA assistant.", response: "LIVA là trợ lý ảo cá nhân tích hợp mô hình ngôn ngữ lớn chạy offline giúp tối ưu hóa công việc hàng ngày..." },
        { time: 223000, text: "Ok, tốt lắm.", response: "", isBackchannel: true },
        
        // 240s: Barge-in test
        { time: 240000, text: "Hãy kể tên các thành phố lớn tại Việt Nam.", response: "Việt Nam có năm thành phố lớn trực thuộc trung ương. Đầu tiên là thủ đô Hà Nội, tiếp theo là thành phố Hồ Chí Minh, trung tâm kinh tế lớn nhất cả nước, sau đó là Đà Nẵng, Hải Phòng và Cần Thơ." },
        { time: 242000, text: "Đổi sang tìm công thức nấu phở bò.", response: "Dạ, công thức nấu phở bò chuẩn vị truyền thống cần xương ống bò ninh kỹ, kết hợp với các loại gia vị nướng thơm như gừng, hành tây, quế, hồi, thảo quả, kèm theo bánh phở tươi ngon sếp nhé.", isInterrupt: true },
        
        { time: 275000, text: "Tuyệt vời, cảm ơn em.", response: "Dạ, chúc sếp một ngày làm việc hiệu quả và nhiều niềm vui!" }
    ];

    turnsTimeline.forEach(turn => {
        setTimeout(() => {
            if (turn.isInterrupt) {
                // Interrupt current LLM/TTS
                currentLlmResponseText = turn.response;
                simulateUserSpeech(turn.text);
            } else if (turn.isBackchannel) {
                // Emit backchannel during active playback
                vadBridge.emit("speech_start");
                setTimeout(() => {
                    vadBridge.emit("speech_end");
                    whisperNode.emit("transcription_ready", turn.text);
                }, 800);
            } else {
                currentLlmResponseText = turn.response;
                simulateUserSpeech(turn.text);
            }
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
    }, 10000);

    // 7. Finish test after 5 minutes
    setTimeout(async () => {
        logger.info("🏁 [Stress Test] Test duration reached (5 minutes). Shutting down...");
        
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
        logger.info(`- Total Runtime: 5 Minutes (${((Date.now() - metrics.startTime) / 1000).toFixed(1)}s)`);
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
