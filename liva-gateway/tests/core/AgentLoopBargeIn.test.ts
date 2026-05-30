import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { AgentLoop } from "../../src/core/AgentLoop";
import { MemoryManager } from "../../src/MemoryManager";
import { SkillRegistry } from "../../src/SkillRegistry";
import { CoreKernelAuthority } from "../../src/core/CoreKernelAuthority";
import { AgentPhase } from "../../src/types/AgentTypes";

// ============================================================
// Module-level Mocks
// ============================================================
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
        child: vi.fn().mockReturnThis()
    }
}));

vi.mock("../../src/core/ModelOrchestrator", () => ({
    ModelOrchestrator: class {
        routerPort = 8000;
        expertPort = 8001;
        startRouter = vi.fn();
        stopRouter = vi.fn();
        stopExpert = vi.fn();
        isReady = vi.fn().mockReturnValue(true);
        startAnomalyDetection = vi.fn();
        restartRouter = vi.fn().mockResolvedValue(true);
        startSingleExpert = vi.fn().mockResolvedValue(true);
        static getAuthorizedTokenFactory = () => ({
            issueToken: vi.fn().mockReturnValue({ secret: "test", phase: AgentPhase.INITIALIZING })
        });
    }
}));

vi.mock("../../src/memory/SemanticRouter", () => ({
    SemanticRouter: class {
        initialize = vi.fn().mockResolvedValue(true);
        route = vi.fn().mockResolvedValue({ activeKit: "general", route: "general" });
    }
}));

vi.mock("../../src/core/PromptBuilder", () => ({
    PromptBuilder: {
        prepareFullAiMessages: vi.fn().mockResolvedValue({
            aiMessages: [
                { role: "system", content: "You are LIVA" }
            ],
            dynamicContextBlock: "mock_dynamic_block"
        }),
        buildToolsPrompt: vi.fn().mockReturnValue(""),
        buildContextPrompt: vi.fn().mockResolvedValue(""),
    }
}));

vi.mock("../../src/core/orchestrators/ToolExecutionOrchestrator", () => ({
    ToolExecutionOrchestrator: class {
        onExecApprovalRequired = null;
        executeWithReflection = vi.fn().mockResolvedValue({ valid: true, resultStr: "mock_result", rawObj: {} });
    }
}));

vi.mock("../../src/core/orchestrators/LTCOrchestrator", () => ({
    LTCOrchestrator: class {
        summarizeAndStore = vi.fn().mockResolvedValue(true);
    }
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
    })),
}));

export const mockCreate = vi.fn();

vi.mock("openai", () => {
    const OpenAI = class {
        chat = {
            completions: {
                create: (...args: any[]) => mockCreate(...args)
            }
        };
    };
    return {
        default: OpenAI,
        OpenAI: OpenAI
    };
});

// [FIX] Mock Scheduler — pass syscall_infer directly to client
vi.mock("../../src/kernel/Scheduler", () => ({
    Scheduler: {
        getInstance: () => ({
            emitSyscall: vi.fn().mockImplementation(async (req: any) => {
                if (req.type === "syscall_infer") {
                    const { client, usingTarget, localMsgs, tempParam, maxTokensParam, topPParam } = req.payload;
                    return client.chat.completions.create({
                        model: usingTarget, messages: localMsgs,
                        temperature: tempParam, max_tokens: maxTokensParam, top_p: topPParam, stream: true,
                    });
                }
                return null;
            }),
            suspend: vi.fn(), resume: vi.fn(),
        }),
    },
}));

vi.mock("../../src/core/LlmCircuitBreaker", () => ({
    LlmCircuitBreaker: {
        getInstance: () => ({
            canExecute: vi.fn().mockReturnValue(true),
            recordSuccess: vi.fn(), recordFailure: vi.fn(),
        }),
    },
}));

vi.mock("../../src/core/config/ConfigManager", () => ({
    ConfigManager: {
        getInstance: () => ({
            isNativeMode: false, aiProvider: "local",
            env: { AI_PROVIDER: "local", LIVA_USE_NATIVE: false },
            getLivaConfig: vi.fn().mockResolvedValue({}),
            invalidateCache: vi.fn(),
        }),
    },
}));

describe("AgentLoop — Barge-in & Audio Interruption Diagnostics", () => {
    let loop: AgentLoop;
    let memory: any;
    let registry: any;

    beforeEach(() => {
        vi.clearAllMocks();
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

        // Mock task bus dispatch to execute asynchronously in the next tick
        (loop as any).dispatch = vi.fn().mockImplementation((task) => {
            Promise.resolve().then(async () => {
                try {
                    await task.execute({ secret: "test", phase: AgentPhase.RUNNING } as any);
                } catch (e) {
                    // Ignore test execution failures in dispatch
                }
            });
        });
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    it("should handle Stage 1: Audio ducking and speech start correctly in the FSM state changes", async () => {
        let streamResolve: any;
        const streamPromise = new Promise((resolve) => { streamResolve = resolve; });

        const mockStream = {
            [Symbol.asyncIterator]: async function* () {
                yield { choices: [{ delta: { content: "Chào " } }] };
                await streamPromise;
                yield { choices: [{ delta: { content: "bạn!" }, finish_reason: "stop" }] };
            }
        };
        mockCreate.mockReturnValue(mockStream);

        // Trigger user input
        loop.handleUserInput("Hello");

        // Give FSM time to transition to thinking/acting and start streaming
        await new Promise(r => setTimeout(r, 50));

        // State machine should be in thinking/acting/streaming
        expect(loop.isBusy).toBe(true);

        // Simulate Stage 1: SPEECH_START event received by loop.bargeIn('SPEECH_START')
        expect(() => loop.bargeIn('SPEECH_START')).not.toThrow();

        // Finish the stream
        streamResolve();
        await new Promise(r => setTimeout(r, 50));

        // The response should complete successfully since SPEECH_START did not abort it
        expect(loop.isBusy).toBe(false);
    });

    it("should handle Stage 2: Hard abort on real speech barge-in and XML-safe truncation", async () => {
        const originalAbortController = globalThis.AbortController;
        let activeSignal: AbortSignal | null = null;
        let onAborted: (() => void) | undefined;
        let resolveAbortSignalPromise: (() => void) | undefined;

        const mockAbortController = class extends originalAbortController {
            constructor() {
                super();
                activeSignal = this.signal;
                this.signal.addEventListener("abort", () => {
                    if (onAborted) onAborted();
                });
            }
        };
        vi.stubGlobal("AbortController", mockAbortController);

        const abortSignalPromise = new Promise<void>((resolve) => {
            resolveAbortSignalPromise = resolve;
            onAborted = resolve;
        });

        mockCreate.mockImplementation(() => {
            return {
                [Symbol.asyncIterator]: async function* () {
                    yield { choices: [{ delta: { content: "Tôi đang " } }] };
                    yield { choices: [{ delta: { content: "trả lời câu <tool_call>" } }] };
                    
                    if (activeSignal) {
                        if (!activeSignal.aborted) {
                            await new Promise<void>((resolvePromise) => {
                                onAborted = () => {
                                    resolvePromise();
                                    if (resolveAbortSignalPromise) resolveAbortSignalPromise();
                                };
                            });
                        }
                    }
                    return;
                }
            };
        });

        // Trigger user input to start generating response
        loop.handleUserInput("Hãy kể một câu chuyện");

        // Wait for the stream to start feeding chunks
        await new Promise(r => setTimeout(r, 100));

        // Now trigger the hard interrupt / real speech barge-in
        loop.bargeIn('BARGE_IN');

        // Verify the abort signal was triggered on the OpenAI API call
        await abortSignalPromise;

        // Give the event loop a few ticks to process truncation logic and save to memory
        await new Promise(r => setTimeout(r, 100));

        // Memory addMessage should have been called with the truncated assistant reply
        expect(memory.addMessage).toHaveBeenCalledWith("user", "Hãy kể một câu chuyện");
        expect(memory.addMessage).toHaveBeenCalledWith("assistant", "Tôi đang <interrupted>");

        // Clean up global stub
        vi.unstubAllGlobals();
    });
});
