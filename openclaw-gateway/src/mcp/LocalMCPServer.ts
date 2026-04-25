import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import {
    CallToolRequestSchema,
    ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import * as fs from 'node:fs/promises';
import * as path from "node:path";
import { logger } from "../utils/logger";
import { AgentSkill } from "../SkillRegistry";

/**
 * LocalMCPServer
 * ==============
 * Hosts the local skills via the Model Context Protocol.
 * Maps legacy `AgentSkill` (from src/skills) into the MCP `Tool` abstraction seamlessly.
 */
export class LocalMCPServer {
    private server: Server;
    private skillCache: Map<string, AgentSkill> = new Map();

    constructor() {
        this.server = new Server(
            {
                name: "liva-local-skills",
                version: "1.0.0",
            },
            {
                capabilities: {
                    tools: {},
                },
            }
        );

        this.setupHandlers();
    }

    public getServerInstance(): Server {
        return this.server;
    }

    /**
     * Loads the legacy skills into memory to back the MCP tools.
     * Uses require to synchronously build the map (or async via import).
     */
    public async loadSkills() {
        const skillsDir = path.join(process.cwd(), "src", "skills");
        try {
            await fs.access(skillsDir);
        } catch {
            logger.warn(`[MCPServer] Skills directory not found: ${skillsDir}`);
            return;
        }

        const files = await fs.readdir(skillsDir);
        for (const file of files) {
            if (file.endsWith(".ts") || file.endsWith(".js")) {
                const skillPath = path.join(skillsDir, file);
                try {
                    // Try dynamic import
                    const module = await import(
                        `file://${skillPath.replace(/\\/g, "/")}?v=${Date.now()}`
                    );
                    if (module.metadata && module.execute) {
                        this.skillCache.set(module.metadata.name, {
                            name: module.metadata.name,
                            description: module.metadata.description,
                            parameters: module.metadata.parameters,
                            search_keywords: module.metadata.search_keywords,
                            isCoreSkill: module.metadata.isCoreSkill || false,
                            execute: module.execute,
                        });
                    }
                } catch {
                    // Fallback to require
                    try {
                        const resolvedPath = require.resolve(skillPath);
                        if (require.cache[resolvedPath]) {
                            delete require.cache[resolvedPath];
                        }
                        const module = require(skillPath);
                        if (module.metadata && module.execute) {
                            this.skillCache.set(module.metadata.name, {
                                name: module.metadata.name,
                                description: module.metadata.description,
                                parameters: module.metadata.parameters,
                                search_keywords: module.metadata.search_keywords,
                                isCoreSkill: module.metadata.isCoreSkill || false,
                                execute: module.execute,
                            });
                        }
                    } catch (err: any) {
                        logger.error(`[MCPServer] Failed to map skill ${file}: ${err.message}`);
                    }
                }
            }
        }
        logger.info(`[MCPServer] Successfully wrapped ${this.skillCache.size} legacy tools into MCP schema.`);
    }

    private setupHandlers() {
        // List Tools Request
        this.server.setRequestHandler(ListToolsRequestSchema, async () => {
            const tools = Array.from(this.skillCache.values()).map(skill => {
                // Determine if there are properties to extract JSON Schema
                const props = skill.parameters?.properties || {};
                const req = skill.parameters?.required || [];
                
                return {
                    name: skill.name,
                    description: skill.description,
                    inputSchema: {
                        type: "object",
                        properties: props,
                        required: req
                    }
                };
            });

            return { tools };
        });

        // Call Tool Request
        this.server.setRequestHandler(CallToolRequestSchema, async (request) => {
            const toolName = request.params.name;
            const args = request.params.arguments || {};
            
            const skill = this.skillCache.get(toolName);
            if (!skill || !skill.execute) { // NOSONAR
                throw new Error(`[MCPServer] Tool '${toolName}' not found or not executable.`);
            }

            try {
                logger.info(`[MCPServer] Executing tool: ${toolName}`);
                const result = await skill.execute(args);
                
                // Trả về theo chuẩn CallToolResult của MCP (content là một array)
                const textContent = typeof result === "string" ? result : JSON.stringify(result, null, 2);
                return {
                    content: [{ type: "text", text: textContent }],
                    isError: false,
                };
            } catch (error: any) {
                logger.error(`[MCPServer] Tool execution error for ${toolName}: ${error.message}`);
                return {
                    content: [{ type: "text", text: `Error: ${error.message}` }],
                    isError: true,
                };
            }
        });
    }
}
