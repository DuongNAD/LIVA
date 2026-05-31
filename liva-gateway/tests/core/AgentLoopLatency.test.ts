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
        child: vi.fn().mockReturnThis(),
    },
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
    LlmCircuitBreaker: { getInstance: () => ({ canExecute: vi.fn().mockReturnValue(true), recordSuccess: vi.fn(), recordFailure: vi.fn() }) },
}));

vi.mock("../../src/core/config/ConfigManager", () => ({
    ConfigManager: {
        getInstance: () => ({
            isNativeMode: false, aiProvider: "local",
            env: { AI_PROVIDER: "local", LIVA_USE_NATIVE: false },
            getLivaConfig: vi.fn().mockResolvedValue({}), invalidateCache: vi.fn(),
        }),
    },
}));

describe("AgentLoop — Data Flow and Latency Diagnostics", () => {
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
        vi.restoreAllMocks();
    });

    it("should process user input, verify complete data flow and measure phase latencies", async () => {
        // We mock a stream with a simulated delay to measure latency accurately
        const FIRST_TOKEN_DELAY_MS = 120;
        const SECOND_TOKEN_DELAY_MS = 80;

        mockCreate.mockImplementation(() => {
            return {
                [Symbol.asyncIterator]: async function* () {
                    // Simulate processing delay for TTFT (Time to First Token)
                    await new Promise(r => setTimeout(r, FIRST_TOKEN_DELAY_MS));
                    yield { choices: [{ delta: { content: "Xin chào sếp, " } }] };
                    
                    // Simulate chunk streaming delay
                    await new Promise(r => setTimeout(r, SECOND_TOKEN_DELAY_MS));
                    yield { choices: [{ delta: { content: "Liva đã sẵn sàng!" }, finish_reason: "stop" }] };
                }
            };
        });

        // Let's track timestamps for each lifecycle hook
        const timestamps: Record<string, number> = {
            start: Date.now(),
            thinkingStart: 0,
            thinkingEnd: 0,
            streamStart: 0,
            streamChunkCount: 0,
            spokenResponse: 0,
        };

        loop.onThinkingStart = () => {
            timestamps.thinkingStart = Date.now();
        };

        loop.onThinkingEnd = () => {
            timestamps.thinkingEnd = Date.now();
        };

        loop.onStreamStart = () => {
            timestamps.streamStart = Date.now();
        };

        loop.onStreamChunk = (chunk) => {
            timestamps.streamChunkCount++;
        };

        const spokenResponsePromise = new Promise<string>((resolve) => {
            loop.onSpokenResponse = (text) => {
                timestamps.spokenResponse = Date.now();
                resolve(text);
            };
        });

        // Trigger input
        loop.handleUserInput("Xin chào!");

        // Wait for final response
        const finalResponseText = await spokenResponsePromise;

        // --- 1. Data Flow Verification ---
        expect(finalResponseText).toBe("Xin chào sếp, Liva đã sẵn sàng!");
        expect(memory.addMessage).toHaveBeenCalledWith("user", "Xin chào!");
        expect(memory.addMessage).toHaveBeenCalledWith("assistant", "Xin chào sếp, Liva đã sẵn sàng!");
        expect(timestamps.thinkingStart).toBeGreaterThan(0);
        expect(timestamps.streamStart).toBeGreaterThan(0);
        expect(timestamps.streamChunkCount).toBe(2);

        // --- 2. Latency Metrics Calculations ---
        const thinkingStartLatency = timestamps.thinkingStart - timestamps.start;
        const timeToFirstToken = timestamps.streamStart - timestamps.start;
        const totalDuration = timestamps.spokenResponse - timestamps.start;

        console.log("\n==========================================");
        console.log("⏱️  LIVA LATENCY & PERFORMANCE REPORT");
        console.log("==========================================");
        console.log(`- Thinking Start Latency : ${thinkingStartLatency}ms`);
        console.log(`- Time To First Token     : ${timeToFirstToken}ms`);
        console.log(`- Total Response Time     : ${totalDuration}ms`);
        console.log("==========================================\n");

        // Verify timing boundaries match simulated delays
        expect(thinkingStartLatency).toBeLessThan(50); // Thinking should start almost instantly
        expect(timeToFirstToken).toBeGreaterThanOrEqual(FIRST_TOKEN_DELAY_MS - 10);
        expect(totalDuration).toBeGreaterThanOrEqual(FIRST_TOKEN_DELAY_MS + SECOND_TOKEN_DELAY_MS - 20);
    });
});
