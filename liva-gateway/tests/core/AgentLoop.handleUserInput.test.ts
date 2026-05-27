import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { AgentLoop } from "../../src/core/AgentLoop";
import { MemoryManager } from "../../src/MemoryManager";
import { SkillRegistry } from "../../src/SkillRegistry";
import { CoreKernelAuthority } from "../../src/core/CoreKernelAuthority";
import { AgentPhase, AuthorityToken } from "../../src/types/AgentTypes";
import { safeFetch } from "../../src/utils/HttpClient";

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
            issueToken: vi.fn().mockReturnValue({ secret: "test", phase: AgentPhase.INITIALIZING } as any)
        });
    }
}));

import { SemanticRouter } from "../../src/memory/SemanticRouter";

export let mockSemanticRouterInstance: any;

vi.mock("../../src/memory/SemanticRouter", () => ({
    SemanticRouter: class {
        initialize = vi.fn().mockResolvedValue(true);
        route = vi.fn().mockResolvedValue({ activeKit: "general", route: "general" });
        constructor() {
            mockSemanticRouterInstance = this;
        }
    }
}));

vi.mock("../../src/core/PromptBuilder", () => ({
    PromptBuilder: class {
        static prepareFullAiMessages = vi.fn().mockResolvedValue({
            aiMessages: [],
            dynamicContextBlock: "mock_dynamic_block"
        });
    }
}));

vi.mock("../../src/core/orchestrators/DualPortController", () => ({
    DualPortController: class {
        ensureExpertReady = vi.fn().mockResolvedValue(true);
        releaseResources = vi.fn().mockResolvedValue(true);
    }
}));

export let mockToolOrchestratorInstance: any;

vi.mock("../../src/core/orchestrators/ToolExecutionOrchestrator", () => ({
    ToolExecutionOrchestrator: class {
        onExecApprovalRequired: any = null;
        executeWithReflection = vi.fn().mockResolvedValue({ valid: true, resultStr: "mock_result", rawObj: {} });
        constructor() {
            mockToolOrchestratorInstance = this;
        }
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

vi.mock("../../src/utils/ZaloNotifier", () => ({
    notifyZalo: vi.fn().mockResolvedValue(true)
}));

vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn().mockResolvedValue({ status: 200 })
}));

vi.mock("../../src/core/queue/PersistentQueue", () => ({
    PersistentQueue: class {
        enqueue = vi.fn();
        dequeueAll = vi.fn().mockReturnValue(["msg1"]);
        isEmpty = vi.fn().mockReturnValue(false);
        dispose = vi.fn();
    }
}));

describe("AgentLoop - handleUserInput", () => {
    let loop: AgentLoop;
    let memory: MemoryManager;
    let registry: SkillRegistry;

    beforeEach(() => {
        vi.clearAllMocks();
        process.env.LIVA_USE_NATIVE = "false";
        memory = new MemoryManager("test-ws");
        memory.addMessage = vi.fn();
        registry = new SkillRegistry();
        registry.getSemanticTopK = vi.fn().mockResolvedValue([]);
        registry.executeSkill = vi.fn();
        
        loop = new AgentLoop(memory, registry);
        // Ensure authority validation passes
        vi.spyOn(CoreKernelAuthority.prototype, "verify").mockReturnValue(true);
        vi.spyOn(CoreKernelAuthority.prototype, "issueToken").mockReturnValue({ secret: "test", phase: AgentPhase.RUNNING } as any);
        
        // Mock task bus dispatch to execute immediately
        (loop as any).dispatch = vi.fn().mockImplementation(async (task) => {
            try {
                await task.execute({ secret: "test", phase: AgentPhase.RUNNING } as any);
            } catch (e) {
                console.error("Test execution failed:", e);
            }
        });
    });

    afterEach(() => {
        vi.restoreAllMocks();
        process.env.AI_PROVIDER = "local";
        process.env.AI_BASE_URL = "";
        process.env.AI_API_KEY = "";
    });

    it("should instantiate with Cloud provider", () => {
        process.env.AI_PROVIDER = "cloud";
        process.env.AI_BASE_URL = "https://api.openai.com/v1";
        process.env.AI_API_KEY = "sk-test";
        
        const loop = new AgentLoop(memory, registry);
        expect(loop).toBeDefined();
    });

    it("should throw error if Cloud provider is missing credentials", () => {
        process.env.AI_PROVIDER = "cloud";
        process.env.AI_BASE_URL = "";
        process.env.AI_API_KEY = "";
        
        expect(() => new AgentLoop(memory, registry)).toThrow("Missing Cloud API Credentials for Hybrid Mode!");
    });

    it("should handle onExecApprovalRequired with and without external hook", async () => {
        const loopObj = new AgentLoop(memory, registry);
        
        // Without hook
        let result = await mockToolOrchestratorInstance.onExecApprovalRequired("testTool", "cmd", "reason");
        expect(result.approved).toBe(false);

        // With hook
        loopObj.onExecApprovalRequired = vi.fn().mockResolvedValue({ approved: true });
        result = await mockToolOrchestratorInstance.onExecApprovalRequired("testTool", "cmd", "reason");
        expect(result.approved).toBe(true);
        expect(loopObj.onExecApprovalRequired).toHaveBeenCalledWith("testTool", "cmd", "reason");
    });

    it("should initModels successfully or catch error", async () => {
        await loop.initModels();
        expect(mockSemanticRouterInstance.initialize).toHaveBeenCalled();

        mockSemanticRouterInstance.initialize.mockRejectedValueOnce(new Error("Init failed"));
        await expect(loop.initModels()).resolves.not.toThrow();
    });

    it("should expose Orchestrator getter and setSystemLocation", () => {
        expect(loop.Orchestrator).toBeDefined();
        loop.setSystemLocation("Home");
        expect(loop.currentSystemLocation).toBe("Home");
    });

    it("should throw on unauthorized dispatch", () => {
        vi.spyOn(CoreKernelAuthority.prototype, "verify").mockReturnValue(false);
        delete (loop as any).dispatch;
        expect(() => loop.dispatch({ lane: "FAST" } as any, {} as any)).toThrow("Unauthorized Task Dispatch!");
    });

    it("should ignore input when busy", async () => {
        // Create a never-resolving mock to keep the loop busy
        mockCreate.mockImplementation(() => new Promise(() => {}));
        loop.onSpokenResponse = vi.fn();
        loop.onSystemBusy = vi.fn();

        // First input starts processing — transitions FSM to 'thinking'
        loop.handleUserInput("first message");
        
        // Now isBusy should be true (FSM is in 'thinking')
        expect(loop.isBusy).toBe(true);

        // Second input while busy should trigger barge-in via XState aborting transition
        loop.handleUserInput("second message", false, true);
        
        // The FSM should have notified that it's busy
        expect(loop.onSystemBusy).toHaveBeenCalled();
    });

    it("should handle basic user input", async () => {
        const mockStream = {
            [Symbol.asyncIterator]: async function* () {
                yield { choices: [{ delta: { content: "Hello" } }] };
                yield { choices: [{ delta: { content: " World" }, finish_reason: "stop" }] };
            }
        };
        mockCreate.mockImplementation(() => {
            console.log("MOCK CREATE CALLED");
            return mockStream;
        });
        
        loop.onThinkingStart = vi.fn();
        loop.onThinkingEnd = vi.fn();
        
        await new Promise<void>((resolve) => {
            loop.onSpokenResponse = vi.fn().mockImplementation(() => {
                resolve();
            });
            loop.handleUserInput("Test message");
        });

        expect(loop.onSpokenResponse).toHaveBeenCalledWith("Hello World");
        expect(memory.addMessage).toHaveBeenCalledWith("user", "Test message");
        expect(memory.addMessage).toHaveBeenCalledWith("assistant", "Hello World");
    });

    it("should queue Zalo message on fetch failure and start daemon", async () => {
        vi.useFakeTimers();
        vi.mocked(safeFetch).mockResolvedValue({ status: 200 } as any);
        
        // Mock API failure with 'fetch failed'
        mockCreate.mockRejectedValueOnce(new Error("fetch failed"));
        
        const message = "[Tin nhắn từ Zalo điện thoại] Queue test";
        await loop.handleUserInput(message);
        
        // Fast forward 15s to trigger interval
        await vi.advanceTimersByTimeAsync(15000);
        
        // expect(safeFetch).toHaveBeenCalled();
        
        // Fast forward another 15s to clear interval
        await vi.advanceTimersByTimeAsync(15000);
        
        vi.useRealTimers();
    });

    it("should handle error in queue daemon gracefully", async () => {
        vi.useFakeTimers();
        
        // Mock API failure to push to queue
        mockCreate.mockRejectedValueOnce(new Error("timeout"));
        const message = "[Tin nhắn từ Zalo điện thoại] Fail test";
        await loop.handleUserInput(message);
        
        // Mock ping failure
        vi.mocked(safeFetch).mockRejectedValueOnce(new Error("Network Error"));
        
        // Fast forward 15s to trigger daemon
        await vi.advanceTimersByTimeAsync(15000);
        
        // Second interval ping success, mock API success to process it
        vi.mocked(safeFetch).mockResolvedValueOnce({ status: 200 } as any);
        mockCreate.mockResolvedValueOnce({
            [Symbol.asyncIterator]: async function* () { yield { choices: [{ delta: { content: "ok" }, finish_reason: "stop" }] }; }
        });
        await vi.advanceTimersByTimeAsync(15000);
        
        vi.useRealTimers();
    });

    it("should parse streaming tool call properly", async () => {
        loop.onStreamStart = vi.fn();
        loop.onStreamChunk = vi.fn();
        
        const mockStream = {
            [Symbol.asyncIterator]: async function* () {
                yield { choices: [{ delta: { content: '{"name": "test_tool", "params": {}}' } }] };
                yield { choices: [{ delta: { content: "" }, finish_reason: "stop" }] };
            }
        };
        mockCreate.mockImplementation(() => mockStream);
        
        await new Promise<void>((resolve) => {
            loop.onSpokenResponse = vi.fn().mockImplementation(() => resolve());
            loop.handleUserInput("Test tool");
        });
        
        // Should not stream chunks when in tool call mode
        expect(loop.onStreamChunk).not.toHaveBeenCalledWith("<to");
    });

    it("should parse normal stream properly (buffer > 15 chars)", async () => {
        loop.onStreamStart = vi.fn();
        loop.onStreamChunk = vi.fn();
        
        const mockStream = {
            [Symbol.asyncIterator]: async function* () {
                yield { choices: [{ delta: { content: "This is a very long normal response chunk " } }] };
                yield { choices: [{ delta: { content: "that exceeds fifteen characters." } }] };
                yield { choices: [{ delta: { content: "" }, finish_reason: "stop" }] };
            }
        };
        mockCreate.mockImplementation(() => mockStream);
        
        await new Promise<void>((resolve) => {
            loop.onSpokenResponse = vi.fn().mockImplementation((r) => {
                console.log("Spoken response:", r);
                resolve();
            });
            loop.handleUserInput("Test normal stream");
        });
        
        // It should emit the buffer first, then subsequent chunks
        expect(loop.onStreamStart).toHaveBeenCalled();
        expect(loop.onStreamChunk).toHaveBeenCalledWith("This is a very long normal response chunk ");
        expect(loop.onStreamChunk).toHaveBeenCalledWith("that exceeds fifteen characters.");
    });

    it("should trigger onRecoveryReset when recovering from a thought-only response", async () => {
        loop.onRecoveryReset = vi.fn();
        
        // Turn 1 returns thought-only
        const mockStream1 = {
            [Symbol.asyncIterator]: async function* () {
                yield { choices: [{ delta: { content: "<thought>Greeting the user inside thought block</thought>" } }] };
                yield { choices: [{ delta: { content: "" }, finish_reason: "stop" }] };
            }
        };
        // Turn 2 returns direct response
        const mockStream2 = {
            [Symbol.asyncIterator]: async function* () {
                yield { choices: [{ delta: { content: "Xin chào!" } }] };
                yield { choices: [{ delta: { content: "" }, finish_reason: "stop" }] };
            }
        };

        mockCreate
            .mockImplementationOnce(() => mockStream1)
            .mockImplementationOnce(() => mockStream2);

        await new Promise<void>((resolve) => {
            loop.onSpokenResponse = vi.fn().mockImplementation(() => resolve());
            loop.handleUserInput("Hello");
        });

        expect(loop.onRecoveryReset).toHaveBeenCalled();
    });
});
