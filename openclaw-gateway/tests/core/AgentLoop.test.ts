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
    DualPortController,
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
    },
}));

vi.mock("../../src/security/ZMAS_Guard", () => ({
    ZMAS_Guard: class {
        executeAutoRemediation = vi.fn((output: string) => output);
    },
}));

vi.mock("../../src/MemoryManager", () => ({
    MemoryManager: vi.fn(),
}));

vi.mock("../../src/SkillRegistry", () => ({
    SkillRegistry: vi.fn().mockImplementation(() => ({
        executeSkill: vi.fn(),
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

// ============================================================
// TEST GROUP 1: CoreKernelAuthority (Singleton + Token System)
// ============================================================
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
        const token = new AuthorityToken(AgentPhase.INITIALIZING, "secret123");
        expect(token.phase).toBe(AgentPhase.INITIALIZING);
    });

    it("should validate with correct phase and secret", () => {
        const token = new AuthorityToken(AgentPhase.RUNNING, "my_secret");
        expect(token.isValid(AgentPhase.RUNNING, "my_secret")).toBe(true);
    });

    it("should reject validation with wrong secret", () => {
        const token = new AuthorityToken(AgentPhase.RUNNING, "my_secret");
        expect(token.isValid(AgentPhase.RUNNING, "wrong_secret")).toBe(false);
    });

    it("should reject validation with wrong phase", () => {
        const token = new AuthorityToken(AgentPhase.RUNNING, "my_secret");
        expect(token.isValid(AgentPhase.PAUSING, "my_secret")).toBe(false);
    });

    it("secret should not be accessible externally", () => {
        const token = new AuthorityToken(AgentPhase.RUNNING, "top_secret");
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
describe("TaskLaneWorker", () => {
    let taskBus: EventEmitter;
    let worker: TaskLaneWorker;
    let authority: CoreKernelAuthority;
    let token: AuthorityToken<typeof AgentPhase.RUNNING>;

    beforeEach(() => {
        taskBus = new EventEmitter();
        authority = CoreKernelAuthority.getInstance();
        token = authority.issueToken(AgentPhase.RUNNING);
        worker = new TaskLaneWorker(TaskLane.BACKGROUND_JOB, taskBus);
    });

    it("should process tasks emitted to the bus", async () => {
        const executeFn = vi.fn().mockResolvedValue(undefined);
        const task: MessageTask = {
            id: "test-1",
            lane: TaskLane.BACKGROUND_JOB,
            data: {},
            execute: executeFn,
        };

        taskBus.emit(TaskLane.BACKGROUND_JOB as string, task, token);
        // Wait for async processing
        await new Promise(r => setTimeout(r, 200));

        expect(executeFn).toHaveBeenCalledTimes(1);
    });

    it("should set task state to COMPLETED on success", async () => {
        const task: MessageTask = {
            id: "test-2",
            lane: TaskLane.BACKGROUND_JOB,
            data: {},
            execute: vi.fn().mockResolvedValue(undefined),
        };

        taskBus.emit(TaskLane.BACKGROUND_JOB as string, task, token);
        await new Promise(r => setTimeout(r, 200));

        expect(task.state).toBe(TaskState.COMPLETED);
    });

    it("should set task state to FAILED on error", async () => {
        const task: MessageTask = {
            id: "test-3",
            lane: TaskLane.BACKGROUND_JOB,
            data: {},
            execute: vi.fn().mockRejectedValue(new Error("Boom")),
        };

        taskBus.emit(TaskLane.BACKGROUND_JOB as string, task, token);
        await new Promise(r => setTimeout(r, 200));

        expect(task.state).toBe(TaskState.FAILED);
    });

    it("should handle multiple sequential task dispatches", async () => {
        // Dispatch 3 tasks one-at-a-time, each waiting for the previous to complete
        for (let i = 0; i < 3; i++) {
            const executeFn = vi.fn().mockResolvedValue(undefined);
            const task: MessageTask = {
                id: `seq-${i}`,
                lane: TaskLane.BACKGROUND_JOB,
                data: {},
                execute: executeFn,
            };

            taskBus.emit(TaskLane.BACKGROUND_JOB as string, task, token);
            await new Promise(r => setTimeout(r, 250));

            expect(executeFn).toHaveBeenCalledTimes(1);
            expect(task.state).toBe(TaskState.COMPLETED);
        }
    });
});

// ============================================================
// TEST GROUP 5: ToolExecutionOrchestrator (Reflection Layer)
// ============================================================
describe("ToolExecutionOrchestrator", () => {
    let orchestrator: ToolExecutionOrchestrator;
    let mockRegistry: any;
    let mockAIClient: any;

    beforeEach(() => {
        mockRegistry = {
            executeSkill: vi.fn(),
        };
        mockAIClient = {
            chat: {
                completions: {
                    create: vi.fn(),
                },
            },
        };
        orchestrator = new ToolExecutionOrchestrator(mockRegistry, mockAIClient);
    });

    it("should return valid=true for clean tool output", async () => {
        mockRegistry.executeSkill.mockResolvedValue("Kết quả tìm kiếm: Hà Nội 35°C");

        const result = await orchestrator.executeWithReflection("search_web", { query: "thời tiết" });

        expect(result.valid).toBe(true);
        expect(result.resultStr).toContain("Hà Nội");
    });

    it("should return valid=false for error output (traceback)", async () => {
        mockRegistry.executeSkill.mockResolvedValue(
            "Traceback (most recent call last):\n  File test.py line 10\nNameError: x not defined"
        );

        const result = await orchestrator.executeWithReflection("run_code", { code: "x" });
        expect(result.valid).toBe(false);
    });

    it("should return valid=false for spawn error", async () => {
        mockRegistry.executeSkill.mockResolvedValue("Error: spawn ENOENT");

        const result = await orchestrator.executeWithReflection("run_cmd", {});
        expect(result.valid).toBe(false);
    });

    it("should return valid=false for ECONNREFUSED", async () => {
        mockRegistry.executeSkill.mockResolvedValue("connect ECONNREFUSED 127.0.0.1:8000");

        const result = await orchestrator.executeWithReflection("api_call", {});
        expect(result.valid).toBe(false);
    });

    it("should return valid=false for JSON error responses", async () => {
        mockRegistry.executeSkill.mockResolvedValue('{"error": "Not found", "code": 404}');

        const result = await orchestrator.executeWithReflection("api_call", {});
        expect(result.valid).toBe(false);
    });

    it("should return valid=false for very short output (≤5 chars)", async () => {
        mockRegistry.executeSkill.mockResolvedValue("err");

        const result = await orchestrator.executeWithReflection("tool", {});
        expect(result.valid).toBe(false);
    });

    it("should handle tool runtime exceptions gracefully", async () => {
        mockRegistry.executeSkill.mockRejectedValue(new Error("Connection timeout"));

        const result = await orchestrator.executeWithReflection("broken_tool", {});

        expect(result.valid).toBe(false);
        expect(result.resultStr).toContain("Tool runtime error");
        expect(result.rawObj).toBeNull();
    });

    it("should sanitize long outputs (>2000 chars) via sub-agent", async () => {
        const longOutput = "A".repeat(3000);
        mockRegistry.executeSkill.mockResolvedValue(longOutput);
        mockAIClient.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "Summary of long data" } }],
        });

        const result = await orchestrator.executeWithReflection("data_tool", {});

        expect(mockAIClient.chat.completions.create).toHaveBeenCalled();
        expect(result.resultStr).toBe("Summary of long data");
    });

    it("should fallback to truncation if sanitizer AI fails", async () => {
        const longOutput = "B".repeat(3000);
        mockRegistry.executeSkill.mockResolvedValue(longOutput);
        mockAIClient.chat.completions.create.mockRejectedValue(new Error("AI down"));

        const result = await orchestrator.executeWithReflection("data_tool", {});

        expect(result.resultStr.length).toBeLessThanOrEqual(1600); // 1500 + suffix
        expect(result.resultStr).toContain("[System: Data too large");
    });
});

// ============================================================
// TEST GROUP 6: LTCOrchestrator (Long-Term Concept Engine)
// ============================================================
describe("LTCOrchestrator", () => {
    let ltc: LTCOrchestrator;
    let mockMemory: any;
    let mockAI: any;

    beforeEach(() => {
        mockMemory = {
            updateLongTermMemory: vi.fn().mockResolvedValue(undefined),
        };
        mockAI = {
            chat: {
                completions: {
                    create: vi.fn(),
                },
            },
        };
        ltc = new LTCOrchestrator(mockMemory, mockAI);
    });

    it("should extract and store a valid concept", async () => {
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "User prefers dark mode UI" } }],
        });

        await ltc.summarizeAndStore("Tôi thích giao diện tối", "Đã ghi nhận!");

        expect(mockMemory.updateLongTermMemory).toHaveBeenCalledWith(
            "Working Concepts",
            expect.arrayContaining([expect.stringContaining("dark mode")])
        );
    });

    it("should skip storing when AI returns NONE", async () => {
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "NONE" } }],
        });

        await ltc.summarizeAndStore("Xin chào", "Chào bạn!");

        expect(mockMemory.updateLongTermMemory).not.toHaveBeenCalled();
    });

    it("should skip storing empty/short responses", async () => {
        mockAI.chat.completions.create.mockResolvedValue({
            choices: [{ message: { content: "ok" } }],
        });

        await ltc.summarizeAndStore("test", "test");

        expect(mockMemory.updateLongTermMemory).not.toHaveBeenCalled();
    });

    it("should not crash when AI call fails", async () => {
        mockAI.chat.completions.create.mockRejectedValue(new Error("timeout"));

        await expect(ltc.summarizeAndStore("test", "test")).resolves.not.toThrow();
    });
});

// ============================================================
// TEST GROUP 7: DualPortController (Circuit Breaker)
// ============================================================
vi.mock("../../src/core/ModelOrchestrator", () => ({
    ModelOrchestrator: {
        getAuthorizedTokenFactory: () => ({
            issueToken: (state: string) => state as any,
        }),
    },
}));

describe("DualPortController", () => {
    let mockOrchestrator: any;
    let controller: DualPortController;

    beforeEach(() => {
        mockOrchestrator = {
            stopRouter: vi.fn().mockResolvedValue(undefined),
            startRouter: vi.fn().mockResolvedValue(undefined),
            startExpert: vi.fn().mockResolvedValue(undefined),
            stopExpert: vi.fn().mockResolvedValue(undefined),
        };

        controller = new DualPortController(mockOrchestrator);
    });

    it("should start expert and mark as awake", async () => {
        const result = await controller.ensureExpertReady();
        expect(result).toBe(true);
        expect(controller.isExpertAwake).toBe(true);
    });

    it("should return true immediately if expert already awake", async () => {
        await controller.ensureExpertReady();
        mockOrchestrator.stopRouter.mockClear();

        const result = await controller.ensureExpertReady();
        expect(result).toBe(true);
        // Should not call stopRouter again
        expect(mockOrchestrator.stopRouter).not.toHaveBeenCalled();
    });

    it("should fallback to router on expert failure", async () => {
        mockOrchestrator.startExpert.mockRejectedValue(new Error("VRAM full"));

        const result = await controller.ensureExpertReady();
        expect(result).toBe(false);
        expect(controller.isExpertAwake).toBe(false);
        expect(mockOrchestrator.startRouter).toHaveBeenCalled();
    });

    it("should release expert resources", async () => {
        await controller.ensureExpertReady();
        await controller.releaseResources();

        expect(mockOrchestrator.stopExpert).toHaveBeenCalled();
        expect(controller.isExpertAwake).toBe(false);
    });

    it("should do nothing on release if expert not awake", async () => {
        await controller.releaseResources();
        expect(mockOrchestrator.stopExpert).not.toHaveBeenCalled();
    });
});
