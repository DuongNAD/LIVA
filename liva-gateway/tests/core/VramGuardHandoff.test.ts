import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { AgentLoop } from "../../src/core/AgentLoop";
import { CoreKernelAuthority } from "../../src/core/CoreKernelAuthority";
import { AgentPhase } from "../../src/types/AgentTypes";
import { EmbeddingService } from "../../src/services/EmbeddingService";

// ============================================================
// Module-level Mocks
// ============================================================
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
        child: vi.fn().mockReturnThis(),
    },
}));

vi.mock("../../src/core/orchestrators/ToolExecutionOrchestrator", () => ({
    ToolExecutionOrchestrator: class {
        onExecApprovalRequired = null;
        executeWithReflection = vi.fn().mockResolvedValue({ valid: true, resultStr: "mock_result", rawObj: {} });
    },
}));

vi.mock("../../src/core/orchestrators/LTCOrchestrator", () => ({
    LTCOrchestrator: class {
        summarizeAndStore = vi.fn().mockResolvedValue(true);
    },
}));

vi.mock("../../src/memory/SemanticRouter", () => ({
    SemanticRouter: class {
        initialize = vi.fn().mockResolvedValue(true);
        route = vi.fn().mockResolvedValue({ activeKit: "general", route: "general" });
    },
}));

vi.mock("../../src/MemoryManager", () => ({
    MemoryManager: vi.fn().mockImplementation(() => ({
        getStructuredMemoryPrompt: vi.fn().mockReturnValue(""),
        getHybridContext: vi.fn().mockReturnValue([]),
        addMessage: vi.fn(),
        updateLongTermMemory: vi.fn(),
        getStructuredMemoryInstance: vi.fn().mockReturnValue({ insertTurnNode: vi.fn() }),
        reflectionDaemon: { queueTurn: vi.fn() },
        consolidationCron: { touch: vi.fn() },
        getPreviousSessionContextPrompt: vi.fn().mockResolvedValue(""),
        workingBuffer: { checkBudget: vi.fn().mockResolvedValue("") },
        getUserProfile: vi.fn().mockResolvedValue({}),
        getLongTermMarkdown: vi.fn().mockReturnValue(""),
        getSessionState: vi.fn().mockResolvedValue(""),
    })),
}));

export const mockOpenAICreate = vi.fn();
export const mockOpenAIConstructor = vi.fn();

vi.mock("openai", () => {
    const OpenAI = class {
        chat: any;
        constructor(config?: any) {
            mockOpenAIConstructor(config);
            this.chat = {
                completions: {
                    create: (...args: any[]) => mockOpenAICreate(...args),
                },
            };
        }
    };
    return {
        default: OpenAI,
        OpenAI: OpenAI,
    };
});

// Mock fs to bypass llama-server path validation
vi.mock("fs", () => ({
    existsSync: () => true,
}));

export const mockLlamaProcess = {
    kill: vi.fn(),
    stdout: { on: vi.fn() },
    stderr: { on: vi.fn() },
    on: vi.fn(),
};

// Mock child_process to capture spawned processes
vi.mock("child_process", () => ({
    spawn: () => mockLlamaProcess,
}));

describe("VramGuardHandoff — Preemption & Cloud Fallback Diagnostics", () => {
    let loop: AgentLoop;
    let memory: any;
    let registry: any;
    let savedEnv: typeof process.env;

    beforeEach(() => {
        vi.clearAllMocks();
        savedEnv = { ...process.env };
        process.env.LIVA_USE_NATIVE = "false";

        memory = {
            getStructuredMemoryPrompt: vi.fn().mockReturnValue(""),
            getHybridContext: vi.fn().mockReturnValue([]),
            addMessage: vi.fn(),
            updateLongTermMemory: vi.fn(),
            getStructuredMemoryInstance: vi.fn().mockReturnValue({ insertTurnNode: vi.fn() }),
            reflectionDaemon: { queueTurn: vi.fn() },
            consolidationCron: { touch: vi.fn() },
            getPreviousSessionContextPrompt: vi.fn().mockResolvedValue(""),
            workingBuffer: { checkBudget: vi.fn().mockResolvedValue("") },
            getUserProfile: vi.fn().mockResolvedValue({}),
            getLongTermMarkdown: vi.fn().mockReturnValue(""),
            getSessionState: vi.fn().mockResolvedValue(""),
        };

        registry = {
            executeSkill: vi.fn(),
            getSemanticTopK: vi.fn().mockResolvedValue([]),
            getAllSkills: vi.fn().mockReturnValue([]),
        };

        loop = new AgentLoop(memory, registry);
        vi.spyOn(CoreKernelAuthority.prototype, "verify").mockReturnValue(true);
        vi.spyOn(CoreKernelAuthority.prototype, "issueToken").mockReturnValue({ secret: "test", phase: AgentPhase.RUNNING } as any);

        // Async task loop dispatch mock
        (loop as any).dispatch = vi.fn().mockImplementation((task) => {
            Promise.resolve().then(async () => {
                try {
                    await task.execute({ secret: "test", phase: AgentPhase.RUNNING } as any);
                } catch (e) {
                    // Handled in test assertions
                }
            });
        });
    });

    afterEach(() => {
        process.env = savedEnv;
        vi.restoreAllMocks();
    });

    it("should trigger llama-server process kill on preemptive yield request", async () => {
        const orchestrator = loop.Orchestrator;
        
        // Spawn/start local server
        await orchestrator.startSingleExpert();

        expect(orchestrator.isReady()).toBe(true);

        // Call kill/yield
        await orchestrator.killLlamaServer();

        // Process SIGKILL must be sent
        expect(mockLlamaProcess.kill).toHaveBeenCalledWith("SIGKILL");
        expect(orchestrator.isReady()).toBe(false);
    });

    it("should notify EmbeddingService to yield VRAM on anomaly_detected event", async () => {
        const orchestrator = loop.Orchestrator;
        const spyEmbedYield = vi.spyOn(EmbeddingService.getInstance(), "setVramYielded");

        // Manually register bootstrap-style wiring
        orchestrator.on("anomaly_detected", () => {
            EmbeddingService.getInstance().setVramYielded(true);
        });

        // Trigger anomaly detected event
        orchestrator.emit("anomaly_detected");

        expect(spyEmbedYield).toHaveBeenCalledWith(true);
    });

    it("should fallback to Cloud API when local model is offline/yielded", async () => {
        // Setup fallback config
        process.env.FALLBACK_AI_BASE_URL = "https://api.fallback-cloud.com/v1";
        process.env.FALLBACK_AI_API_KEY = "sk-fallback-12345";
        process.env.FALLBACK_AI_MODEL = "fallback-gpt-model";

        // Mark local orchestrator as not ready
        vi.spyOn(loop.Orchestrator, "isReady").mockReturnValue(false);

        const mockStream = {
            [Symbol.asyncIterator]: async function* () {
                yield { choices: [{ delta: { content: "Chào sếp, em đang dùng đám mây." } }] };
            }
        };
        mockOpenAICreate.mockReturnValue(mockStream);

        let spokenText = "";
        loop.onSpokenResponse = (text) => {
            spokenText = text;
        };

        loop.handleUserInput("Hello");

        // Let the async task queue tick
        await new Promise(r => setTimeout(r, 100));

        // Check if OpenAI constructor was called with fallback config
        expect(mockOpenAIConstructor).toHaveBeenCalledWith(expect.objectContaining({
            baseURL: "https://api.fallback-cloud.com/v1",
            apiKey: "sk-fallback-12345",
        }));

        // Verify spoken response matches fallback stream
        expect(spokenText).toBe("Chào sếp, em đang dùng đám mây.");
    });

    it("should circuit-break gracefully if local is offline/yielded and NO fallback is configured", async () => {
        // Ensure no fallback configs
        delete process.env.FALLBACK_AI_BASE_URL;
        delete process.env.FALLBACK_AI_API_KEY;

        vi.spyOn(loop.Orchestrator, "isReady").mockReturnValue(false);

        let spokenText = "";
        loop.onSpokenResponse = (text) => {
            spokenText = text;
        };

        loop.handleUserInput("Hello");

        await new Promise(r => setTimeout(r, 100));

        // Bypassed reasoning, responded with system warning
        expect(spokenText).toContain("Hệ thống AI lõi đang bận xử lý ứng dụng nặng");
        expect(mockOpenAICreate).not.toHaveBeenCalled();
    });

    it("should handle mid-stream preemption exception gracefully (VRAM yielded error)", async () => {
        // Ensure local is ready initially so it passes initial entry checks
        vi.spyOn(loop.Orchestrator, "isReady").mockReturnValue(true);

        // Mock stream to throw VRAM yielded error mid-stream
        mockOpenAICreate.mockImplementation(() => {
            return {
                [Symbol.asyncIterator]: async function* () {
                    yield { choices: [{ delta: { content: "Đang xử lý... " } }] };
                    throw new Error("VRAM yielded during execution");
                }
            };
        });

        let spokenText = "";
        loop.onSpokenResponse = (text) => {
            spokenText = text;
        };

        loop.handleUserInput("Chạy tác vụ nặng");

        await new Promise(r => setTimeout(r, 100));

        // Verify that preemption message is spoken
        expect(spokenText).toContain("Anh ơi, em vừa nhường GPU cho game của anh rồi");
    });
});
