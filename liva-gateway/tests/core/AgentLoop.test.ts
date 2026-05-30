import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "events";
import {
    CoreKernelAuthority,
    AuthorityToken,
    AgentPhase,
    TaskLane,
    TaskState,
    TaskLaneWorker,
    ToolExecutionOrchestrator,
    LTCOrchestrator,
    AgentLoop,
    type MessageTask,
} from "../../src/core";

// ============================================================
// Mocks
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

export let capturedOrchestrator: any = null;

vi.mock("../../src/core/orchestrators/ToolExecutionOrchestrator", () => {
    return {
        ToolExecutionOrchestrator: class {
            onExecApprovalRequired: any = null;
            constructor() {
                capturedOrchestrator = this;
            }
        }
    };
});

vi.mock("../../src/security/ZMAS_Guard", () => ({
    ZMAS_Guard: class {
        executeAutoRemediation = vi.fn((output: string) => output);
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
    })),
}));

vi.mock("../../src/SkillRegistry", () => ({
    SkillRegistry: vi.fn().mockImplementation(() => ({
        executeSkill: vi.fn(),
        getSemanticTopK: vi.fn().mockResolvedValue([]),
        getAllSkills: vi.fn().mockReturnValue([]),
    })),
}));

vi.mock("../../src/mcp/MCPClientManager", () => ({
    MCPClientManager: {
        getInstance: vi.fn().mockReturnValue({
            connectServer: vi.fn(),
            getAllConnectedTools: vi.fn().mockResolvedValue([]),
        }),
    },
}));

vi.mock("../../src/memory/SemanticRouter", () => {
    return {
        SemanticRouter: class {
            initialize = vi.fn();
            route = vi.fn().mockResolvedValue({ route: "deep_reasoning", confidence: 0.9, activeKit: "general" });
        }
    };
});

vi.mock("../../src/services/SmartTurnVAD", () => ({
    SmartTurnVAD: vi.fn()
}));

vi.mock("../../src/core/PromptBuilder", () => ({
    PromptBuilder: {
        prepareFullAiMessages: vi.fn().mockResolvedValue({
            aiMessages: [
                { role: "system", content: "You are LIVA" },
                { role: "user", content: "test" }
            ],
            dynamicContextBlock: "mock_dynamic_block"
        }),
        buildToolsPrompt: vi.fn().mockReturnValue(""),
        buildContextPrompt: vi.fn().mockResolvedValue(""),
    },
}));

vi.mock("node:sqlite", () => ({
    DatabaseSync: vi.fn().mockImplementation(() => ({
        exec: vi.fn(),
        prepare: vi.fn().mockReturnValue({
            run: vi.fn(),
            all: vi.fn().mockReturnValue([]),
            get: vi.fn(),
        }),
        close: vi.fn(),
    })),
}));

// ============================================================
// Mocks
// ============================================================
export const mockOpenAICreate = vi.fn().mockResolvedValue({
    choices: [{ message: { content: "Default response" } }]
});

vi.mock("openai", () => ({
    default: class OpenAI {
        chat = {
            completions: {
                create: mockOpenAICreate
            }
        }
    }
}));

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
describe("CoreKernelAuthority", () => {
    it("should be a singleton", () => {
        const a = CoreKernelAuthority.getInstance();
        const b = CoreKernelAuthority.getInstance();
        expect(a).toBe(b);
    });

    it("should issue a valid AuthorityToken for a given phase", () => {
        const authority = CoreKernelAuthority.getInstance();
        const token = authority.issueToken(AgentPhase.RUNNING);
        expect(token).toBeDefined();
        expect(token.phase).toBe(AgentPhase.RUNNING);
    });

    it("should verify a valid token against its phase", () => {
        const authority = CoreKernelAuthority.getInstance();
        const token = authority.issueToken(AgentPhase.RUNNING);
        expect(authority.verify(token, AgentPhase.RUNNING)).toBe(true);
    });

    it("should reject verification against wrong phase", () => {
        const authority = CoreKernelAuthority.getInstance();
        const token = authority.issueToken(AgentPhase.RUNNING);
        // Token was issued for RUNNING, but we verify against PAUSING
        expect(authority.verify(token, AgentPhase.PAUSING)).toBe(false);
    });
});

// ============================================================
// TEST GROUP 2: AuthorityToken
// ============================================================
describe("AuthorityToken", () => {
    it("should store the phase", () => {
        const token = CoreKernelAuthority.getInstance().issueToken(AgentPhase.INITIALIZING);
        expect(token.phase).toBe(AgentPhase.INITIALIZING);
    });

    it("should validate with correct phase and secret", () => {
        const token = CoreKernelAuthority.getInstance().issueToken(AgentPhase.RUNNING);
        expect(CoreKernelAuthority.getInstance().verify(token, AgentPhase.RUNNING)).toBe(true);
    });

    it("should reject validation with wrong phase", () => {
        const token = CoreKernelAuthority.getInstance().issueToken(AgentPhase.RUNNING);
        expect(CoreKernelAuthority.getInstance().verify(token, AgentPhase.PAUSING)).toBe(false);
    });

    it("secret should not be accessible externally", () => {
        const token = CoreKernelAuthority.getInstance().issueToken(AgentPhase.RUNNING);
        // Private class member #secret should not be enumerable
        const keys = Object.keys(token);
        expect(keys).not.toContain("#secret");
        expect(keys).not.toContain("secret");
    });
});

// ============================================================
// TEST GROUP 3: AgentPhase & TaskLane Branded Types
// ============================================================
describe("Branded Types", () => {
    it("AgentPhase constants should be defined", () => {
        expect(AgentPhase.INITIALIZING).toBeDefined();
        expect(AgentPhase.RUNNING).toBeDefined();
        expect(AgentPhase.PAUSING).toBeDefined();
        expect(AgentPhase.TERMINATING).toBeDefined();
    });

    it("TaskLane constants should be defined", () => {
        expect(TaskLane.UI_INTERACTION).toBeDefined();
        expect(TaskLane.LLM_REASONING).toBeDefined();
        expect(TaskLane.BACKGROUND_JOB).toBeDefined();
    });

    it("AgentPhase values should be unique strings", () => {
        const phases = [
            AgentPhase.INITIALIZING,
            AgentPhase.RUNNING,
            AgentPhase.PAUSING,
            AgentPhase.TERMINATING,
        ];
        const unique = new Set(phases);
        expect(unique.size).toBe(4);
    });
});

// ============================================================
// TEST GROUP 4: TaskLaneWorker (Queue Processing)
// ============================================================
// ============================================================

describe("AgentLoop", () => {
    let loop: AgentLoop;
    let mockUI: any;
    let mockZalo: any;

    beforeEach(async () => {
        vi.clearAllMocks();
        mockUI = { on: vi.fn(), emit: vi.fn() };
        mockZalo = { on: vi.fn(), emit: vi.fn() };
        
        // Ensure logger mock is completely clean
        const { logger } = await import("../../src/utils/logger");
        vi.mocked(logger.warn).mockClear();
        vi.mocked(logger.info).mockClear();

        process.env.LIVA_USE_NATIVE = "false";
        process.env.PORT_ROUTER = "8000";
        process.env.PORT_EXPERT = "8001";
        
        const { ModelOrchestrator } = await import("../../src/core/ModelOrchestrator");
        vi.spyOn(ModelOrchestrator.prototype, "isReady").mockReturnValue(true);
        vi.spyOn(ModelOrchestrator.prototype, "restartRouter").mockResolvedValue(undefined as any);
        vi.spyOn(ModelOrchestrator.prototype, "startAnomalyDetection").mockReturnValue(undefined as any);

        // Create an instance of AgentLoop with mocked dependencies
        loop = new AgentLoop(
            {
                getStructuredMemoryPrompt: vi.fn().mockReturnValue(""),
                getHybridContext: vi.fn().mockReturnValue([]),
                addMessage: vi.fn(),
                updateLongTermMemory: vi.fn(),
                routeQuery: vi.fn().mockResolvedValue({ route: "deep_reasoning", confidence: 0.9 }),
                getUserProfile: vi.fn().mockResolvedValue({}),
                getLongTermMarkdown: vi.fn().mockReturnValue(""),
                getSessionState: vi.fn().mockResolvedValue(""),
                workingBuffer: { checkBudget: vi.fn().mockResolvedValue("") },
                getStructuredMemoryInstance: vi.fn().mockReturnValue({ insertTurnNode: vi.fn() }),
                reflectionDaemon: { queueTurn: vi.fn() },
                consolidationCron: { touch: vi.fn() },
            } as any,
            {
                executeSkill: vi.fn(),
                getSemanticTopK: vi.fn().mockResolvedValue([]),
                getAllSkills: vi.fn().mockReturnValue([]),
            } as any
        );
    });

    it("Zalo Suspend Queue (DEV GUARD B): should push message to queue on ECONNREFUSED network error", async () => {
        // Mock fetch with nested cause property
        const fetchError = new Error('fetch failed', { cause: new Error('ECONNREFUSED') });
        mockOpenAICreate.mockRejectedValueOnce(fetchError);

        const { TraceContext } = await import("../../src/utils/TraceContext");
        loop.channelRouter = {
            getAdapter: () => ({
                sendText: vi.fn().mockResolvedValue(undefined),
                sendApprovalCard: vi.fn().mockResolvedValue(undefined),
                sendScreenshot: vi.fn().mockResolvedValue(undefined)
            })
        } as any;

        // We can just call the public handleUserInput directly
        // Note: handleUserInput dispatches to the event bus asynchronously
        try {
            TraceContext.runWithContext(() => {
                loop.handleUserInput("[Tin nhắn từ Zalo điện thoại] Test message");
            }, { channel: "zalo", userId: "test_user_123" });
        } catch (e) {}

        // Wait for the TaskLaneWorker to process the dispatched task
        await new Promise(r => setTimeout(r, 300));

        const { logger } = await import("../../src/utils/logger");

        // Verify that the message was pushed to the queue via logger
        expect(logger.warn).toHaveBeenCalledWith(
            expect.stringContaining("Sếp chờ chút nha! Server AI đang tiến hóa")
        );
        expect(logger.warn).toHaveBeenCalledWith(
            expect.stringContaining("Test message")
        );
        
        vi.unstubAllGlobals();
    });

    it('should correctly handle onExecApprovalRequired callback (Zero Trust)', async () => {
        // Access the captured instance from the mock
        expect(capturedOrchestrator).toBeDefined();
        expect(capturedOrchestrator.onExecApprovalRequired).toBeDefined();

        // Trigger the fallback branch (when AgentLoop.onExecApprovalRequired is NOT set)
        const resultFallback = await capturedOrchestrator.onExecApprovalRequired('rm', 'rm -rf /', 'dangerous');
        expect(resultFallback.approved).toBe(false);

        // Trigger the happy branch (when AgentLoop.onExecApprovalRequired IS set)
        const mockCustomApproval = vi.fn().mockResolvedValue({ approved: true, editedCommand: 'echo safe' });
        loop.onExecApprovalRequired = mockCustomApproval;
        
        const resultCustom = await capturedOrchestrator.onExecApprovalRequired('echo', 'echo safe', 'safe');
        expect(mockCustomApproval).toHaveBeenCalledWith('echo', 'echo safe', 'safe');
        expect(resultCustom.approved).toBe(true);
        expect(resultCustom.editedCommand).toBe('echo safe');
    });

    it('should pre-warm the speculative cache and consume it during handleUserInput', async () => {
        const { logger } = await import("../../src/utils/logger");
        
        // 1. Run speculative warm
        await loop.speculativeWarm("Thời tiết");
        
        // Verify logger notes hydration
        expect(logger.debug).toHaveBeenCalledWith(
            expect.stringContaining("[v26.1 Speculative] 🔮 Cache hydrated")
        );
        
        // Mock LLM create to yield response
        mockOpenAICreate.mockResolvedValueOnce({
            [Symbol.asyncIterator]: async function* () {
                yield { choices: [{ delta: { content: "Trời hôm nay đẹp." } }] };
            }
        } as any);

        // 2. Trigger user input
        loop.handleUserInput("Thời tiết Hà Nội thế nào");
        
        // Wait for the async task loop to run
        await new Promise(r => setTimeout(r, 100));

        // Verify that the pre-warmed route was used (avoiding recalculating SemanticRouter and PromptBuilder)
        expect(logger.info).toHaveBeenCalledWith(
            expect.stringContaining("[v23 Speculative] ⚡ Using pre-warmed route")
        );
    });

    it('should clear speculative cache when clearSpeculativeCache is called', async () => {
        const { logger } = await import("../../src/utils/logger");
        
        await loop.speculativeWarm("Thời tiết");
        loop.clearSpeculativeCache();
        
        expect(logger.debug).toHaveBeenCalledWith(
            expect.stringContaining("[v26.1 Speculative] 🔮 Cache cleared")
        );
        
        // Trigger user input
        mockOpenAICreate.mockResolvedValueOnce({
            [Symbol.asyncIterator]: async function* () {
                yield { choices: [{ delta: { content: "Trời hôm nay đẹp." } }] };
            }
        } as any);
        loop.handleUserInput("Thời tiết Hà Nội thế nào");
        
        await new Promise(r => setTimeout(r, 100));

        // Verify that speculative cache was NOT used
        expect(logger.info).not.toHaveBeenCalledWith(
            expect.stringContaining("[v23 Speculative] ⚡ Using pre-warmed route")
        );
    });
});
