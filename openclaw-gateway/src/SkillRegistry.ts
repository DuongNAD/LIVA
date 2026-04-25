import { logger } from "./utils/logger";
import { MCPClientManager } from "./mcp/MCPClientManager";
import * as path from "node:path";
import * as fs from "node:fs";

export interface AgentSkill {
  name: string;
  description: string;
  parameters: any; 
  search_keywords?: string[];
  isCoreSkill?: boolean;
  requiresApproval?: boolean;
  execute?: (args: any) => Promise<any>;
}

export class SkillRegistry {
  private mcpManager: MCPClientManager;
  private mcpToolsList: any[] = [];
  private fallbackSkills: Map<string, AgentSkill> = new Map();

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
          parameters: tool.inputSchema,
          _serverId: tool._serverId
      } as any));
      return [...mcpSkills, ...Array.from(this.fallbackSkills.values())];
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
