/**
 * ToolExecutionOrchestrator.test.ts — Tool execution with Zero-Trust tests
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
    child: vi.fn().mockReturnThis(),
  },
}));

vi.mock("../../src/security/ZMAS_Guard", () => ({
  ZMAS_Guard: class MockGuard {
    executeAutoRemediation = vi.fn().mockImplementation((str: string) => str);
  },
}));

import { ToolExecutionOrchestrator } from "../../src/core/orchestrators/ToolExecutionOrchestrator";

describe("ToolExecutionOrchestrator", () => {
  let orchestrator: ToolExecutionOrchestrator;
  let mockRegistry: any;
  let mockAI: any;

  beforeEach(() => {
    vi.clearAllMocks();
    mockRegistry = {
      executeSkill: vi.fn().mockResolvedValue("Skill executed successfully"),
    };
    mockAI = {
      chat: {
        completions: {
          create: vi.fn().mockResolvedValue({
            choices: [{ message: { content: "Summarized result" } }],
          }),
        },
      },
    };
    orchestrator = new ToolExecutionOrchestrator(mockRegistry, mockAI);
  });

  describe("executeWithReflection", () => {
    it("should execute a safe skill and return valid result", async () => {
      const result = await orchestrator.executeWithReflection("get_weather", {
        location: "Hà Nội",
      });
      expect(result.valid).toBe(true);
      expect(result.resultStr).toBe("Skill executed successfully");
    });

    it("should mark error results as invalid", async () => {
      mockRegistry.executeSkill.mockResolvedValue("error: spawn ENOENT");
      const result = await orchestrator.executeWithReflection(
        "run_shell_command",
        { command: "bad" },
      );
      expect(result.valid).toBe(false);
    });

    it("should mark traceback results as invalid", async () => {
      mockRegistry.executeSkill.mockResolvedValue(
        "Traceback (most recent call last): ...",
      );
      const result = await orchestrator.executeWithReflection(
        "run_python_script",
        {},
      );
      expect(result.valid).toBe(false);
    });

    it("should mark ECONNREFUSED as invalid", async () => {
      mockRegistry.executeSkill.mockResolvedValue(
        "ECONNREFUSED localhost:8000",
      );
      const result = await orchestrator.executeWithReflection("web_search", {});
      expect(result.valid).toBe(false);
    });

    it("should mark short results (<=5 chars) as invalid", async () => {
      mockRegistry.executeSkill.mockResolvedValue("err");
      const result = await orchestrator.executeWithReflection("test_skill", {});
      expect(result.valid).toBe(false);
    });

    it("should handle tool runtime errors gracefully", async () => {
      mockRegistry.executeSkill.mockRejectedValue(new Error("Skill crashed"));
      const result = await orchestrator.executeWithReflection(
        "broken_skill",
        {},
      );
      expect(result.valid).toBe(false);
      expect(result.resultStr).toContain("Skill crashed");
    });

    it("should sanitize long results (>2000 chars) via AI", async () => {
      const longResult = "A".repeat(3000);
      mockRegistry.executeSkill.mockResolvedValue(longResult);
      const result = await orchestrator.executeWithReflection(
        "data_heavy_skill",
        {},
      );
      // Should call AI for sanitization
      expect(mockAI.chat.completions.create).toHaveBeenCalled();
      expect(result.resultStr).toBe("Summarized result");
    });

    it("should fallback to truncation if sanitizer AI fails", async () => {
      const longResult = "B".repeat(3000);
      mockRegistry.executeSkill.mockResolvedValue(longResult);
      mockAI.chat.completions.create.mockRejectedValue(new Error("AI down"));
      const result = await orchestrator.executeWithReflection(
        "data_heavy_skill",
        {},
      );
      expect(result.resultStr).toContain("[System: Data too large");
    });

    it("should strictly limit fallback truncation to 1500 chars + suffix", async () => {
      const exact1500 = "B".repeat(1500);
      const extra = " EXTRA".repeat(300); // make length > 2000
      mockRegistry.executeSkill.mockResolvedValue(exact1500 + extra);
      mockAI.chat.completions.create.mockRejectedValue(new Error("AI down"));
      const result = await orchestrator.executeWithReflection(
        "data_heavy_skill",
        {},
      );
      // 1500 chars + "\n\n[System: Data too large, safely trimmed]" length (42) = 1542
      expect(result.resultStr.length).toBe(1542);
      expect(result.resultStr.substring(0, 1500)).toBe(exact1500);
    });

    it("should stringify object results (Line 21)", async () => {
      mockRegistry.executeSkill.mockResolvedValue({ success: true, data: "test" });
      const result = await orchestrator.executeWithReflection("test_skill", {});
      expect(result.valid).toBe(true);
      expect(result.resultStr).toBe('{"success":true,"data":"test"}');
    });

    it("should mark JSON containing error as invalid", async () => {
      mockRegistry.executeSkill.mockResolvedValue({ error: "Something went wrong" });
      const result = await orchestrator.executeWithReflection("test_skill", {});
      expect(result.valid).toBe(false);
    });

    it("should fallback if AI sanitizer returns empty content", async () => {
      const longResult = "C".repeat(3000);
      mockRegistry.executeSkill.mockResolvedValue(longResult);
      mockAI.chat.completions.create.mockResolvedValue({ choices: [{ message: { content: "" } }] });
      const result = await orchestrator.executeWithReflection("data_heavy_skill", {});
      expect(result.resultStr).toBe("C".repeat(1500));
    });
  });
});
