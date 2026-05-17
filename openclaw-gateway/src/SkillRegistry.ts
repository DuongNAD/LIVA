import { logger } from "./utils/logger";
import { MCPClientManager } from "./mcp/MCPClientManager";
import { EmbeddingService } from "./services/EmbeddingService";
import { cosineSimilarity } from "./utils/VectorMath";
import { SkillCircuitBreaker } from "./core/SkillCircuitBreaker";
import { SkillWhitelist } from "./core/SkillWhitelist";
import LRUCache from "lru-cache";
import * as path from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { LocalMCPServer } from "./mcp/LocalMCPServer";

import { SkillCategory } from "./skills/SkillMetadata";

export interface AgentSkill {
  name: string;
  description: string;
  short_desc?: string;           // Tool Attention: mô tả siêu ngắn cho Filtered Full Schema
  category?: SkillCategory;      // BẮT BUỘC dùng enum này theo chuẩn v19
  semantic_tags?: string[];      // Từ khóa vector cho sqlite-vec
  kit?: import("./memory/SemanticRouter").SkillKit; // [Dynamic Gating]
  parameters: any; 
  search_keywords?: string[];
  isCoreSkill?: boolean;
  requiresApproval?: boolean;
  requires_hitl?: boolean;       // Cờ bảo mật - Bắt buộc người dùng UI duyệt
  is_cpu_heavy?: boolean;        // Cờ hiệu năng - Cảnh báo khóa Event Loop
  execute?: (args: any) => Promise<any>;
}

/** DG-2: Dynamic Similarity Threshold — tools below this are excluded */
const SIMILARITY_THRESHOLD = 0.65;

export class SkillRegistry {
  private readonly mcpManager: MCPClientManager;
  private mcpToolsList: any[] = [];
  private fallbackSkills: Map<string, AgentSkill> = new Map();
  private localMcpClient?: Client;
  /** Skill metadata from LocalMCPServer (search_keywords, isCoreSkill, kit, etc.) */
  private localSkillMeta: Map<string, AgentSkill> = new Map();

  /**
   * LRU-cached description embeddings — prevents re-embedding
   * 33 skills × 384D vector on each user turn.
   * TTL 1h: descriptions don't change at runtime.
   */
  private readonly descEmbeddingCache = new LRUCache<string, number[]>({
      max: 100,
      ttl: 3600000, // 1 hour
  });

  // [v25] Passive Circuit Breaker — tracks per-skill failures, prunes dead tools from context
  public readonly circuitBreaker = new SkillCircuitBreaker();

  // [v25] Skill Whitelist — user-controlled enablement (persisted to disk)
  public readonly whitelist = new SkillWhitelist();

  constructor() {
      this.mcpManager = MCPClientManager.getInstance();
      this.registerBuiltInSkills();
  }

  public async registerLocalSkills() {
      try {
          const localMCPServer = new LocalMCPServer();
          await localMCPServer.loadSkills();
          
          const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
          
          await localMCPServer.getServerInstance().connect(serverTransport);
          
          this.localMcpClient = new Client(
              { name: "LIVA-Gateway-InProcess", version: "1.0.0" },
              { capabilities: {} }
          );
          
          await this.localMcpClient.connect(clientTransport);
          
          const response = await this.localMcpClient.listTools();
          // Add local tools with no _serverId or a specific local ID
          const localTools = response.tools.map(t => ({ ...t, _serverId: "liva-local-in-process" }));
          
          // Combine with any other tools from mcpManager (if there are external ones)
          const externalTools = await this.mcpManager.getAllConnectedTools();
          this.mcpToolsList = [...localTools, ...externalTools];

          // [CRITICAL] Store skill metadata (search_keywords, isCoreSkill, kit, etc.)
          // MCP protocol strips custom fields, so we preserve them via side-channel
          this.localSkillMeta = localMCPServer.getSkillMetadata();

          logger.info(`[SkillRegistry] In-process MCP Server initialized. Cached ${localTools.length} local tools + ${externalTools.length} external tools.`);
      } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          logger.error(`[SkillRegistry] In-process MCP Init Error: ${errMsg}`);
      }
  }

  public registerSkill(skill: AgentSkill) {
      this.fallbackSkills.set(skill.name, skill);
  }

  public getAllSkills(): AgentSkill[] {
      const mcpSkills = this.mcpToolsList.map(tool => {
          // Enrich MCP tools with metadata that MCP protocol strips out
          const meta = this.localSkillMeta.get(tool.name);
          return {
              name: tool.name,
              description: tool.description,
              short_desc: meta?.short_desc || tool.description?.substring(0, 80),
              parameters: tool.inputSchema,
              search_keywords: meta?.search_keywords,
              isCoreSkill: meta?.isCoreSkill || false,
              category: meta?.category,
              semantic_tags: meta?.semantic_tags,
              kit: meta?.kit,
              requires_hitl: meta?.requires_hitl,
              is_cpu_heavy: meta?.is_cpu_heavy,
              _serverId: tool._serverId
          } as AgentSkill & { _serverId: string };
      });
      return [...mcpSkills, ...Array.from(this.fallbackSkills.values())];
  }

  /**
   * [v25] Get all skills EXCLUDING those with open circuit breakers.
   * Used by PromptBuilder to prevent injecting dead tools into LLM context.
   */
  public getHealthySkills(): AgentSkill[] {
      const openCircuits = new Set(this.circuitBreaker.getOpenCircuits());
      const disabledSkills = this.whitelist.getDisabledSkills();
      
      if (openCircuits.size === 0 && disabledSkills.size === 0) return this.getAllSkills();

      const healthy = this.getAllSkills().filter(s => 
          !openCircuits.has(s.name) && !disabledSkills.has(s.name)
      );

      if (openCircuits.size > 0) {
          logger.debug(`[v25 CircuitBreaker] Pruned ${openCircuits.size} dead skills: ${[...openCircuits].join(", ")}`);
      }
      if (disabledSkills.size > 0) {
          logger.debug(`[v25 Whitelist] Pruned ${disabledSkills.size} disabled skills: ${[...disabledSkills].join(", ")}`);
      }
      return healthy;
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
      // [v25] Use healthy skills only — exclude open circuit breakers
      let allSkills = this.getHealthySkills();

      // [Dynamic Gating] Phase 1: Filter by activeKit
      if (activeKit) {
          allSkills = allSkills.filter(s => s.isCoreSkill || s.kit === activeKit || s.kit === "GENERAL_KIT" || !s.kit);
      }

      // Fast-exit: if no query text, return core skills only
      if (!userQuery || userQuery.trim().length === 0) {
          return allSkills.filter(s => s.isCoreSkill);
      }

      const embedSvc = EmbeddingService.getInstance();

      // [CRITICAL] When EmbeddingService is using dummy vectors, cosine similarity
      // is meaningless (all skills get identical scores). Fall back to keyword-only matching.
      if (!embedSvc.ready) {
          logger.debug("[ToolAttention] EmbeddingService not ready — using keyword-only matching");
          const queryLower = userQuery.toLowerCase();
          const coreSkills: AgentSkill[] = [];
          const keywordMatched: AgentSkill[] = [];

          for (const skill of allSkills) {
              if (skill.isCoreSkill) {
                  coreSkills.push(skill);
                  continue;
              }
              const keywords = skill.search_keywords || [];
              const hasKeywordHit = keywords.some(kw =>
                  kw.length >= 2 && queryLower.includes(kw.toLowerCase())
              );
              if (hasKeywordHit) {
                  keywordMatched.push(skill);
              }
          }

          if (keywordMatched.length > 0) {
              logger.debug(`[ToolAttention] Keyword-only matched ${keywordMatched.length} tools: ${keywordMatched.map(s => s.name).join(", ")}`);
          }

          return [...coreSkills, ...keywordMatched.slice(0, topK)];
      }

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
              const descText = `${skill.name} ${skill.search_keywords?.join(" ") || ""} ${skill.short_desc || skill.description.substring(0, 80)}`;
              try {
                  descVec = await embedSvc.embed(descText);
              } catch {
                  // [v25 FIX] embed() may throw mid-loop if VRAMGuard fires during processing.
                  // Skip this skill's semantic scoring — it won't appear in top-K results
                  // but coreSkills are still always included.
                  continue;
              }
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

      // [KEYWORD BOOST] Fallback for cross-lingual queries (Vietnamese → English embedding model)
      // If a skill has a search_keyword that exactly matches a substring in the user query,
      // include it even if cosine similarity is below threshold.
      const queryLower = userQuery.toLowerCase();
      const qualifiedNames = new Set(qualified.map(q => q.skill.name));
      const keywordBoosted: AgentSkill[] = [];

      for (const { skill, score } of scored) {
          if (qualifiedNames.has(skill.name)) continue; // Already qualified via embedding
          if (score < 0.30) continue; // Hard floor — skip completely irrelevant tools

          const keywords = skill.search_keywords || [];
          const hasKeywordHit = keywords.some(kw =>
              kw.length >= 2 && queryLower.includes(kw.toLowerCase())
          );
          if (hasKeywordHit) {
              keywordBoosted.push(skill);
              logger.debug(`[ToolAttention/KeywordBoost] "${skill.name}" matched via keyword in query (cosine=${score.toFixed(3)})`);
          }
      }

      if (qualified.length > 0 || keywordBoosted.length > 0) {
          logger.debug(
              `[ToolAttention] Filtered ${allSkills.length} → ${coreSkills.length + qualified.length + keywordBoosted.length} tools ` +
              `(embedding: ${qualified.length}, keyword-boost: ${keywordBoosted.length})` +
              (qualified.length > 0 ? ` (top: ${qualified[0].skill.name}@${qualified[0].score.toFixed(3)})` : ``)
          );
      } else {
          logger.debug(`[ToolAttention] No tools above threshold ${SIMILARITY_THRESHOLD} — returning core only`);
      }

      return [...coreSkills, ...qualified.map(s => s.skill), ...keywordBoosted];
  }

  public async executeSkill(name: string, args: any): Promise<any> {
      logger.info(`[SkillRegistry] Đang thực thi kỹ năng qua MCP: ${name}`);

      if (!this.circuitBreaker.canExecute(name)) {
          logger.warn(`[CircuitBreaker] 🔌 Skill '${name}' đang bị NGẮT MẠCH (OPEN). Bỏ qua request.`);
          throw new Error(`Hệ thống mạng đang lỗi, hãy thử lại sau. (Skill: ${name})`);
      }

      try {
          const fallback = this.fallbackSkills.get(name);
          let rawResult;
          if (fallback) {
              rawResult = await fallback.execute!(args);
          } else {
              const tool = this.mcpToolsList.find(t => t.name === name);
              if (!tool) {
                  throw new Error(`MCP Tool '${name}' không tồn tại hoặc chưa kết nối!`);
              }

              let result;
              if (tool._serverId === "liva-local-in-process") {
                  if (!this.localMcpClient) throw new Error("Local MCP Client not initialized");
                  result = await this.localMcpClient.callTool({ name, arguments: args });
              } else {
                  result = await this.mcpManager.executeTool(tool._serverId, name, args);
              }
              
              const contentArray = result.content as { type: string, text: string }[] | undefined;
              
              if (result.isError) {
                  const textContent = contentArray?.[0]?.text || "Unknown MCP Error";
                  throw new Error(textContent);
              }
              
              rawResult = contentArray?.[0]?.text || "Success (No content)";
          }

          this.circuitBreaker.recordSuccess(name);
          return rawResult;
      } catch (error: any) {
          const errMsg = error.message || String(error);
          this.circuitBreaker.recordFailure(name, errMsg);
          if (error.message && error.message.includes("không tồn tại")) {
              throw error;
          }
          throw new Error(`MCP Tool '${name}' execution failed: ${errMsg}`);
      }
  }

  private registerBuiltInSkills() {
    this.registerSkill({
      name: "get_current_time",
      description: "[AUTO_RUN] Get current system date and time.",
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

    // --- GEMINI SURFER SKILL ---
    import('./skills/web/GeminiSurfer').then(geminiSurfer => {
      this.registerSkill({
        ...geminiSurfer.metadata,
        execute: geminiSurfer.execute
      });
    }).catch(e => logger.error(`[SkillRegistry] Lỗi nạp GeminiSurfer: ${e.message}`));
  }
}
