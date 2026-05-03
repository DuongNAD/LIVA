import { logger } from "./utils/logger";
import { MCPClientManager } from "./mcp/MCPClientManager";
import { EmbeddingService } from "./services/EmbeddingService";
import { cosineSimilarity } from "./utils/VectorMath";
import LRUCache from "lru-cache";
import * as path from "node:path";

export interface AgentSkill {
  name: string;
  description: string;
  short_desc?: string;           // Tool Attention: mô tả siêu ngắn cho Filtered Full Schema
  kit?: import("./memory/SemanticRouter").SkillKit; // [Dynamic Gating]
  parameters: any; 
  search_keywords?: string[];
  isCoreSkill?: boolean;
  requiresApproval?: boolean;
  execute?: (args: any) => Promise<any>;
}

/** DG-2: Dynamic Similarity Threshold — tools below this are excluded */
const SIMILARITY_THRESHOLD = 0.65;

export class SkillRegistry {
  private readonly mcpManager: MCPClientManager;
  private mcpToolsList: any[] = [];
  private fallbackSkills: Map<string, AgentSkill> = new Map();

  /**
   * LRU-cached description embeddings — prevents re-embedding
   * 33 skills × 384D vector on each user turn.
   * TTL 1h: descriptions don't change at runtime.
   */
  private readonly descEmbeddingCache = new LRUCache<string, number[]>({
      max: 100,
      ttl: 3600000, // 1 hour
  });

  constructor() {
      this.mcpManager = MCPClientManager.getInstance();
      this.registerBuiltInSkills();
  }

  public async registerLocalSkills() {
      try {
          // Gọi LocalAdapterServer qua stdio để bọc 29 skills thành chuẩn MCP
          const adapterScript = path.join(process.cwd(), "src", "mcp", "LocalAdapterServer.ts");
          await this.mcpManager.connectServer({
              id: "liva-legacy-adapter",
              type: "stdio",
              command: "npx",
              args: ["tsx", adapterScript]
          });
          
          this.mcpToolsList = await this.mcpManager.getAllConnectedTools();
          logger.info(`[SkillRegistry] MCP Client Manager initialized. Cached ${this.mcpToolsList.length} global tools via standard protocol.`);
      } catch (e: any) {
          logger.error(`[SkillRegistry] MCP Init Error: ${e.message}`);
      }
  }

  public registerSkill(skill: AgentSkill) {
      this.fallbackSkills.set(skill.name, skill);
  }

  public getAllSkills(): AgentSkill[] {
      const mcpSkills = this.mcpToolsList.map(tool => ({
          name: tool.name,
          description: tool.description,
          short_desc: tool.description?.substring(0, 80),
          parameters: tool.inputSchema,
          _serverId: tool._serverId
      } as any));
      return [...mcpSkills, ...Array.from(this.fallbackSkills.values())];
  }

  /**
   * Tool Attention — Semantic Top-K Filtering via EmbeddingService + Dynamic Gating
   * =============================================================
   * Pre-filters tools based on `activeKit` (Dynamic Gating) to prevent Prompt Bloat.
   * Then uses shared EmbeddingService singleton (all-MiniLM-L6-v2, 384D)
   * to compute cosine similarity between user query and remaining tool descriptions.
   *
   * DG-2: Dynamic Similarity Threshold (>0.65).
   *   If no tool exceeds threshold → returns [] (prevents hallucinated tool calls).
   *   CoreSkills always included regardless of threshold.
   *
   * @param userQuery  The user's raw text input
   * @param activeKit  The currently active kit detected by SemanticRouter
   * @param topK       Maximum non-core tools to return (default 3)
   * @returns          Filtered skills sorted by semantic relevance
   */
  public async getSemanticTopK(userQuery: string, activeKit?: import("./memory/SemanticRouter").SkillKit, topK: number = 3): Promise<AgentSkill[]> {
      let allSkills = this.getAllSkills();

      // [Dynamic Gating] Phase 1: Filter by activeKit
      if (activeKit) {
          allSkills = allSkills.filter(s => s.isCoreSkill || s.kit === activeKit || s.kit === "GENERAL_KIT" || !s.kit);
      }

      // Fast-exit: if no query text, return core skills only
      if (!userQuery || userQuery.trim().length === 0) {
          return allSkills.filter(s => s.isCoreSkill);
      }

      const embedSvc = EmbeddingService.getInstance();
      let queryVec: number[];
      try {
          queryVec = await embedSvc.embedWithTimeout(userQuery, 500);
      } catch {
          // Embedding failed — fallback to returning all skills (graceful degradation)
          logger.warn("[ToolAttention] Embedding failed, falling back to getAllSkills()");
          return allSkills;
      }

      const coreSkills: AgentSkill[] = [];
      const scored: Array<{ skill: AgentSkill; score: number }> = [];

      for (const skill of allSkills) {
          // CoreSkills always included (get_current_time, handoff_to_expert, etc.)
          if (skill.isCoreSkill) {
              coreSkills.push(skill);
              continue;
          }

          // Get or compute cached embedding for skill description
          let descVec = this.descEmbeddingCache.get(skill.name);
          if (!descVec) {
              const descText = skill.short_desc || skill.description.substring(0, 80);
              descVec = await embedSvc.embed(descText);
              this.descEmbeddingCache.set(skill.name, descVec);
          }

          const score = cosineSimilarity(queryVec, descVec);
          scored.push({ skill, score });
      }

      // DG-2: Dynamic threshold — filter out low-confidence tools
      const qualified = scored
          .filter(s => s.score >= SIMILARITY_THRESHOLD)
          .sort((a, b) => b.score - a.score)
          .slice(0, topK);

      if (qualified.length > 0) {
          logger.debug(
              `[ToolAttention] Filtered ${allSkills.length} → ${coreSkills.length + qualified.length} tools ` +
              `(top: ${qualified[0].skill.name}@${qualified[0].score.toFixed(3)})`
          );
      } else {
          logger.debug(`[ToolAttention] No tools above threshold ${SIMILARITY_THRESHOLD} — returning core only`);
      }

      return [...coreSkills, ...qualified.map(s => s.skill)];
  }

  public async executeSkill(name: string, args: any): Promise<any> {
      logger.info(`[SkillRegistry] Đang thực thi kỹ năng qua MCP: ${name}`);
      
      const fallback = this.fallbackSkills.get(name);
      if (fallback) {
          return await fallback.execute!(args);
      }

      const tool = this.mcpToolsList.find(t => t.name === name);
      if (!tool) {
          throw new Error(`MCP Tool '${name}' không tồn tại hoặc chưa kết nối!`);
      }

      try {
          const result = await this.mcpManager.executeTool(tool._serverId, name, args);
          const contentArray = result.content as { type: string, text: string }[] | undefined;
          
          if (result.isError) {
              const textContent = contentArray?.[0]?.text || "Unknown MCP Error";
              throw new Error(textContent);
          }
          return contentArray?.[0]?.text || "Success (No content)";
      } catch (e: any) {
          throw new Error(`MCP Tool '${name}' execution failed: ${e.message}`);
      }
  }

  private registerBuiltInSkills() {
    this.registerSkill({
      name: "get_current_time",
      description: "Lấy thời gian hiện tại của hệ thống.",
      isCoreSkill: true,
      parameters: {
        type: "object",
        properties: { timezone: { type: "string" } },
      },
      execute: async (args: any) => {
        const date = new Date();
        if (args.timezone) return date.toLocaleString("vi-VN", { timeZone: args.timezone });
        return date.toLocaleString("vi-VN", { timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone });
      },
    });

    this.registerSkill({
      name: "read_file",
      description: "Đọc nội dung của một tệp tin trên hệ thống (Local).",
      parameters: {
        type: "object",
        properties: { path: { type: "string" } },
        required: ["path"],
      },
      execute: async (args: any) => {
        try {
          const fsp = await import('fs/promises');
          return await fsp.readFile(args.path, "utf8");
        } catch (error: any) {
          return `Lỗi khi đọc tệp: ${(error as Error).message}`;
        }
      },
    });

    // --- GEMINI SURFER SKILL ---
    import('./skills/GeminiSurfer.js').then(geminiSurfer => {
      this.registerSkill({
        ...geminiSurfer.metadata,
        execute: geminiSurfer.execute
      });
    }).catch(e => logger.error(`[SkillRegistry] Lỗi nạp GeminiSurfer: ${e.message}`));
  }
}
