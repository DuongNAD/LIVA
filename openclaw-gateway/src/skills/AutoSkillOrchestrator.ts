import { promises as fsp } from "node:fs";
import * as path from "node:path";
import * as crypto from "node:crypto";
import LRUCache from "lru-cache";
import { logger } from "../utils/logger";
import { StackDetector } from "./StackDetector";
import { SkillsLockDataSchema, ToolMappingSchema } from "./autoskills-types";
import type { SkillsLockData, TechStack, ToolMapping } from "./autoskills-types";

export class AutoSkillOrchestrator {
  #detector: StackDetector;
  #workspaceCache: LRUCache<string, SkillsLockData>;
  #pendingTasks = new Map<string, Promise<ToolMapping[]>>();

  constructor(detector = new StackDetector()) {
    this.#detector = detector;
    this.#workspaceCache = new LRUCache<string, SkillsLockData>({
      max: 5,
      ttl: 3_600_000,
    });
  }

  public async onboardWorkspace(workspacePath: string, forceRevalidate = false): Promise<ToolMapping[]> {
    const workspaceRoot = path.resolve(workspacePath);
    const workspaceHash = this.#generateHash(workspaceRoot);

    if (forceRevalidate) {
      this.#workspaceCache.delete(workspaceHash);
    } else {
      const cached = this.#workspaceCache.get(workspaceHash);
      if (cached) return cached.activeTools;
    }

    const pending = this.#pendingTasks.get(workspaceHash);
    if (pending) return pending;

    const task = this.#executeOnboarding(workspaceRoot, workspaceHash).finally(() => {
      this.#pendingTasks.delete(workspaceHash);
    });
    this.#pendingTasks.set(workspaceHash, task);
    return task;
  }

  async #executeOnboarding(workspacePath: string, workspaceHash: string): Promise<ToolMapping[]> {
    logger.info({ context: "AutoSkillOrchestrator", workspacePath }, "Beginning AutoSkills onboarding");

    const lockPath = path.join(workspacePath, ".liva", "skills.lock");
    try {
      const raw = await fsp.readFile(lockPath, "utf-8");
      const parsed = SkillsLockDataSchema.parse(JSON.parse(raw));
      if (parsed.workspaceHash === workspaceHash) {
        this.#workspaceCache.set(workspaceHash, parsed);
        return parsed.activeTools;
      }
    } catch (error: unknown) {
      logger.debug({ context: "AutoSkillOrchestrator", err: error, lockPath }, "Skills lock unavailable or stale");
    }

    const detectedStack = await this.#detector.detect(workspacePath);
    const activeTools = this.#mapStackToSkills(detectedStack);
    const lockData: SkillsLockData = {
      workspaceHash,
      detectedStack,
      activeTools,
      lastUpdated: new Date().toISOString(),
      schemaVersion: "1.0.0",
    };

    await this.#persistLockFile(workspacePath, lockData);
    this.#workspaceCache.set(workspaceHash, lockData);
    return activeTools;
  }

  public forceRevalidate(workspacePath: string): void {
    const workspaceHash = this.#generateHash(path.resolve(workspacePath));
    this.#workspaceCache.delete(workspaceHash);
    logger.info({ context: "AutoSkillOrchestrator", workspacePath }, "Workspace AutoSkills cache invalidated");
  }

  #mapStackToSkills(stack: TechStack[]): ToolMapping[] {
    const tools = new Map<string, ToolMapping>();
    const add = (tool: ToolMapping) => {
      tools.set(tool.toolName, ToolMappingSchema.parse(tool));
    };

    add({ toolName: "read_local_file", version: "latest", source: "local", description: "[AUTO_RUN] Read local project files", kit: "GENERAL_KIT" });
    add({ toolName: "write_local_file", version: "latest", source: "local", description: "Write local project files", kit: "GENERAL_KIT" });
    add({ toolName: "list_directory", version: "latest", source: "local", description: "List project directories", kit: "GENERAL_KIT" });
    add({ toolName: "execute_command", version: "latest", source: "local", description: "Run approved local commands", kit: "GENERAL_KIT" });
    add({ toolName: "web_search", version: "latest", source: "local", description: "Search the web for current context", kit: "GENERAL_KIT" });

    if (stack.some((item) => ["nodejs", "typescript", "go", "rust", "php"].includes(item))) {
      add({ toolName: "gitnexus_query", version: "latest", source: "local", description: "Semantic codebase search", kit: "DEVOPS_KIT" });
      add({ toolName: "git_operator", version: "latest", source: "local", description: "Inspect Git project state", kit: "DEVOPS_KIT" });
      add({ toolName: "log_analyzer", version: "latest", source: "local", description: "Analyze development logs", kit: "DEVOPS_KIT" });
    }

    if (stack.some((item) => ["react", "nextjs", "vue", "tailwind"].includes(item))) {
      add({ toolName: "browser_harness", version: "latest", source: "local", description: "Inspect browser UI flows", kit: "DEVOPS_KIT" });
    }

    if (stack.includes("docker")) {
      add({ toolName: "docker_sandbox_manager", version: "latest", source: "local", description: "Manage Docker sandbox tasks", kit: "DEVOPS_KIT" });
      add({ toolName: "get_system_info", version: "latest", source: "local", description: "Inspect local resource usage", kit: "DEVOPS_KIT" });
    }

    if (stack.includes("python")) {
      add({ toolName: "log_analyzer", version: "latest", source: "local", description: "Analyze Python/runtime logs", kit: "DEVOPS_KIT" });
    }

    return Array.from(tools.values());
  }

  async #persistLockFile(workspacePath: string, data: SkillsLockData): Promise<void> {
    const livaDir = path.join(workspacePath, ".liva");
    const lockPath = path.join(livaDir, "skills.lock");
    const tmpPath = `${lockPath}.${crypto.randomUUID()}.tmp`;

    try {
      await fsp.mkdir(livaDir, { recursive: true });
      await fsp.writeFile(tmpPath, JSON.stringify(data, null, 2), "utf-8");
      await fsp.rename(tmpPath, lockPath);
    } catch (error: unknown) {
      try {
        await fsp.unlink(tmpPath);
      } catch {
        // Best-effort cleanup only.
      }
      throw error;
    }
  }

  #generateHash(input: string): string {
    return crypto.createHash("sha256").update(input).digest("hex").substring(0, 16);
  }

  public dispose(): void {
    this.#workspaceCache.clear();
    this.#pendingTasks.clear();
  }
}
