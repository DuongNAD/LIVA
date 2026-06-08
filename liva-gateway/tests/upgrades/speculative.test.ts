import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import path from "node:path";
import fs from "node:fs";
import cp from "node:child_process";
import { AgentLoop } from "../../src/core/AgentLoop";

// Mocking dependencies for AgentLoop and ModelOrchestrator
vi.mock("fs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("fs")>();
  return {
    ...actual,
    existsSync: vi.fn().mockImplementation((p: string) => {
      if (p.includes("llama-server.exe") || p.includes("gemma") || p.includes("draft")) {
        return true;
      }
      return false;
    }),
  };
});

vi.mock("child_process", () => {
  const mockCp = {
    spawn: vi.fn().mockReturnValue({
      pid: 9999,
      stdout: { on: vi.fn() },
      stderr: { on: vi.fn() },
      on: vi.fn(),
      kill: vi.fn(),
    }),
    execSync: vi.fn().mockReturnValue(Buffer.from("")),
  };
  return {
    ...mockCp,
    default: mockCp,
  };
});

const mockOpenAICreate = vi.fn().mockResolvedValue({
  [Symbol.asyncIterator]: async function* () {
    yield { choices: [{ delta: { content: "Trời hôm nay đẹp." } }] };
  }
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

vi.mock("../../src/utils/logger", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
    child: vi.fn().mockReturnThis(),
  },
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

vi.mock("../../src/memory/SemanticRouter", () => {
  return {
    SemanticRouter: class {
      initialize = vi.fn();
      route = vi.fn().mockResolvedValue({ route: "chitchat", confidence: 0.9, activeKit: "general" });
    }
  };
});

describe("Speculative Decoding Tests", () => {
  const originalEnv = { ...process.env };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  afterEach(() => {
    process.env = { ...originalEnv };
  });

  it("should parse and validate speculative configuration via ConfigManager", async () => {
    process.env.LIVA_ENABLE_SPECULATIVE = "true";
    process.env.LIVA_DRAFT_MODEL_NAME = "gemma-draft-model.gguf";

    const { ConfigManager } = await import("../../src/core/config/ConfigManager");
    const config = ConfigManager.getInstance().get();

    expect(config.LIVA_ENABLE_SPECULATIVE).toBe(true);
    expect(config.LIVA_DRAFT_MODEL_NAME).toBe("gemma-draft-model.gguf");
  });

  it("should map draft args correctly in ModelOrchestrator when speculative is enabled", async () => {
    process.env.LIVA_USE_NATIVE = "false";
    process.env.LIVA_ENABLE_SPECULATIVE = "true";
    process.env.LIVA_DRAFT_MODEL_NAME = "gemma-draft-model.gguf";
    process.env.AI_MODELS_DIR = "E:\\AI_Models";
    process.env.EXPERT_MODEL_NAME = "gemma-expert.gguf";

    const { ModelOrchestrator } = await import("../../src/core/ModelOrchestrator");
    const orchestrator = new ModelOrchestrator();

    await orchestrator.startSingleExpert();

    const expectedDraftModelPath = path.join("E:\\AI_Models", "gemma-draft-model.gguf");

    expect(cp.spawn).toHaveBeenCalled();
    const spawnCall = vi.mocked(cp.spawn).mock.calls[0];
    const args = spawnCall[1];
    expect(args).toContain("-md");
    expect(args).toContain(expectedDraftModelPath);
    expect(args).toContain("--draft");
    expect(args).toContain("5");

    await orchestrator.dispose();
  });

  it("should not append draft args in ModelOrchestrator when speculative is disabled", async () => {
    process.env.LIVA_USE_NATIVE = "false";
    process.env.LIVA_ENABLE_SPECULATIVE = "false";
    process.env.LIVA_DRAFT_MODEL_NAME = "gemma-draft-model.gguf";
    process.env.AI_MODELS_DIR = "E:\\AI_Models";
    process.env.EXPERT_MODEL_NAME = "gemma-expert.gguf";

    const { ModelOrchestrator } = await import("../../src/core/ModelOrchestrator");
    const orchestrator = new ModelOrchestrator();

    await orchestrator.startSingleExpert();

    expect(cp.spawn).toHaveBeenCalled();
    const spawnCall = vi.mocked(cp.spawn).mock.calls[0];
    const args = spawnCall[1];
    expect(args).not.toContain("-md");
    expect(args).not.toContain("--draft");

    await orchestrator.dispose();
  });

  it("should warm the speculative cache in AgentLoop and verify it is consumed", async () => {
    const memoryMock = {
      getStructuredMemoryPrompt: vi.fn().mockReturnValue(""),
      getHybridContext: vi.fn().mockReturnValue([]),
      addMessage: vi.fn(),
      updateLongTermMemory: vi.fn(),
      routeQuery: vi.fn().mockResolvedValue({ route: "chitchat", confidence: 0.9 }),
      getUserProfile: vi.fn().mockResolvedValue({}),
      getLongTermMarkdown: vi.fn().mockReturnValue(""),
      getSessionState: vi.fn().mockResolvedValue(""),
      workingBuffer: { checkBudget: vi.fn().mockResolvedValue("") },
      getStructuredMemoryInstance: vi.fn().mockReturnValue({ insertTurnNode: vi.fn() }),
      reflectionDaemon: { queueTurn: vi.fn() },
      consolidationCron: { touch: vi.fn() },
      getPreviousSessionContextPrompt: vi.fn().mockResolvedValue(""),
      getShortTermHistory: vi.fn().mockResolvedValue([]),
    };

    const registryMock = {
      executeSkill: vi.fn(),
      getSemanticTopK: vi.fn().mockResolvedValue([]),
      getAllSkills: vi.fn().mockReturnValue([]),
    };

    const loop = new AgentLoop(memoryMock as any, registryMock as any);
    vi.spyOn(loop.Orchestrator, "isReady").mockReturnValue(true);
    const { logger } = await import("../../src/utils/logger");

    // Pre-warm the cache
    await loop.speculativeWarm("Thời tiết");

    expect(logger.debug).toHaveBeenCalledWith(
      expect.stringContaining("[v26.1 Speculative] 🔮 Cache hydrated")
    );

    // Call handleUserInput to consume cache
    loop.handleUserInput("Thời tiết Hà Nội thế nào");

    await new Promise((r) => setTimeout(r, 100));

    expect(logger.info).toHaveBeenCalledWith(
      expect.stringContaining("[v23 Speculative] ⚡ Using pre-warmed route")
    );

    await loop.shutdown();
  });

  it("should bypass speculative cache on key mismatch and route dynamically", async () => {
    const memoryMock = {
      getStructuredMemoryPrompt: vi.fn().mockReturnValue(""),
      getHybridContext: vi.fn().mockReturnValue([]),
      addMessage: vi.fn(),
      updateLongTermMemory: vi.fn(),
      routeQuery: vi.fn().mockResolvedValue({ route: "chitchat", confidence: 0.9 }),
      getUserProfile: vi.fn().mockResolvedValue({}),
      getLongTermMarkdown: vi.fn().mockReturnValue(""),
      getSessionState: vi.fn().mockResolvedValue(""),
      workingBuffer: { checkBudget: vi.fn().mockResolvedValue("") },
      getStructuredMemoryInstance: vi.fn().mockReturnValue({ insertTurnNode: vi.fn() }),
      reflectionDaemon: { queueTurn: vi.fn() },
      consolidationCron: { touch: vi.fn() },
      getPreviousSessionContextPrompt: vi.fn().mockResolvedValue(""),
      getShortTermHistory: vi.fn().mockResolvedValue([]),
    };

    const registryMock = {
      executeSkill: vi.fn(),
      getSemanticTopK: vi.fn().mockResolvedValue([]),
      getAllSkills: vi.fn().mockReturnValue([]),
    };

    const loop = new AgentLoop(memoryMock as any, registryMock as any);
    vi.spyOn(loop.Orchestrator, "isReady").mockReturnValue(true);
    const { logger } = await import("../../src/utils/logger");

    // Pre-warm the cache with "Thời tiết"
    await loop.speculativeWarm("Thời tiết");

    expect(logger.debug).toHaveBeenCalledWith(
      expect.stringContaining("[v26.1 Speculative] 🔮 Cache hydrated")
    );

    // Call handleUserInput with mismatched text "Gửi zalo cho Dương"
    loop.handleUserInput("Gửi zalo cho Dương");

    await new Promise((r) => setTimeout(r, 100));

    // Logger.info with "Using pre-warmed route" should NOT be called
    const hasPreWarmedLog = vi.mocked(logger.info).mock.calls.some(call => 
      typeof call[0] === "string" && call[0].includes("[v23 Speculative] ⚡ Using pre-warmed route")
    );
    expect(hasPreWarmedLog).toBe(false);

    await loop.shutdown();
  });

  it("should warn and fallback when draft model does not exist", async () => {
    process.env.LIVA_USE_NATIVE = "false";
    process.env.LIVA_ENABLE_SPECULATIVE = "true";
    process.env.LIVA_DRAFT_MODEL_NAME = "gemma-draft-model.gguf";
    process.env.AI_MODELS_DIR = "E:\\AI_Models";
    process.env.EXPERT_MODEL_NAME = "gemma-expert.gguf";

    const { ModelOrchestrator } = await import("../../src/core/ModelOrchestrator");
    const { logger } = await import("../../src/utils/logger");

    // Mock existsSync: return true for expert and exe but false for draft model
    vi.mocked(fs.existsSync).mockImplementation((p: string) => {
      if (p.includes("llama-server.exe") || p.includes("gemma-expert.gguf")) {
        return true;
      }
      return false;
    });

    const orchestrator = new ModelOrchestrator();
    await orchestrator.startSingleExpert();

    expect(cp.spawn).toHaveBeenCalled();
    const spawnCall = vi.mocked(cp.spawn).mock.calls[0];
    const args = spawnCall[1];
    expect(args).not.toContain("-md");
    expect(args).not.toContain("--draft");

    expect(logger.warn).toHaveBeenCalledWith(
      expect.stringContaining("Draft model not found at")
    );

    await orchestrator.dispose();
  });
});
