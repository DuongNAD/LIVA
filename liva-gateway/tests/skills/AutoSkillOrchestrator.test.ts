import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { promises as fsp } from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { AutoSkillOrchestrator } from "../../src/skills/AutoSkillOrchestrator";
import { SkillsLockDataSchema } from "../../src/skills/autoskills-types";
import type { StackDetector } from "../../src/skills/StackDetector";

vi.mock("../../src/utils/logger", () => ({
  logger: {
    info: vi.fn(),
    debug: vi.fn(),
  },
}));

describe("AutoSkillOrchestrator", () => {
  let workspaceDir: string;
  let detectMock: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    workspaceDir = await fsp.mkdtemp(path.join(os.tmpdir(), "liva-autoskills-"));
    detectMock = vi.fn().mockResolvedValue(["nodejs", "typescript"]);
  });

  afterEach(async () => {
    vi.restoreAllMocks();
    await fsp.rm(workspaceDir, { recursive: true, force: true });
  });

  function createOrchestrator(): AutoSkillOrchestrator {
    return new AutoSkillOrchestrator({ detect: detectMock } as unknown as StackDetector);
  }

  it("should detect stack, persist lock file, and cache subsequent calls", async () => {
    const orchestrator = createOrchestrator();

    const first = await orchestrator.onboardWorkspace(workspaceDir);
    const second = await orchestrator.onboardWorkspace(workspaceDir);

    expect(detectMock).toHaveBeenCalledTimes(1);
    expect(second).toEqual(first);
    expect(first.map(tool => tool.toolName)).toEqual(expect.arrayContaining([
      "read_local_file",
      "execute_command",
      "gitnexus_query",
    ]));

    const raw = await fsp.readFile(path.join(workspaceDir, ".liva", "skills.lock"), "utf-8");
    const parsed = SkillsLockDataSchema.parse(JSON.parse(raw));
    expect(parsed.detectedStack).toEqual(["nodejs", "typescript"]);
  });

  it("should ignore corrupted lock file and rebuild", async () => {
    await fsp.mkdir(path.join(workspaceDir, ".liva"), { recursive: true });
    await fsp.writeFile(path.join(workspaceDir, ".liva", "skills.lock"), "not json", "utf-8");
    detectMock.mockResolvedValueOnce(["python"]);

    const tools = await createOrchestrator().onboardWorkspace(workspaceDir);

    expect(detectMock).toHaveBeenCalledOnce();
    expect(tools.map(tool => tool.toolName)).toContain("execute_command");
  });

  it("should clean temporary lock file when atomic rename fails", async () => {
    vi.spyOn(fsp, "rename").mockRejectedValueOnce(new Error("EPERM"));

    await expect(createOrchestrator().onboardWorkspace(workspaceDir)).rejects.toThrow("EPERM");

    const entries = await fsp.readdir(path.join(workspaceDir, ".liva"));
    expect(entries.some(entry => entry.endsWith(".tmp"))).toBe(false);
  });

  it("should coalesce concurrent onboarding requests", async () => {
    const orchestrator = createOrchestrator();
    detectMock.mockImplementationOnce(async () => {
      await new Promise(resolve => setTimeout(resolve, 20));
      return ["docker"];
    });

    const [first, second] = await Promise.all([
      orchestrator.onboardWorkspace(workspaceDir),
      orchestrator.onboardWorkspace(workspaceDir),
    ]);

    expect(detectMock).toHaveBeenCalledTimes(1);
    expect(first).toEqual(second);
    expect(first.map(tool => tool.toolName)).toContain("docker_sandbox_manager");
  });

  it("should invalidate cache and reload when forceRevalidate is called", async () => {
    const orchestrator = createOrchestrator();
    
    await orchestrator.onboardWorkspace(workspaceDir);
    expect(detectMock).toHaveBeenCalledTimes(1);

    orchestrator.forceRevalidate(workspaceDir);
    await fsp.rm(path.join(workspaceDir, ".liva", "skills.lock"), { force: true });
    
    await orchestrator.onboardWorkspace(workspaceDir);
    expect(detectMock).toHaveBeenCalledTimes(2); // Should have re-run detector due to cache clear
  });
});
