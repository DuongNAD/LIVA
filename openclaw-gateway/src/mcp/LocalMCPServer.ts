import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import {
    CallToolRequestSchema,
    ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import * as fs from 'node:fs/promises';
import * as path from "node:path";
import { logger } from "../utils/logger";
import { AgentSkill } from "../SkillRegistry";
import type { SkillCategory } from "../skills/SkillMetadata";
import { validateSkillMetadata } from "./SkillMetadataSchema";
import { z } from "zod";
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
const require = createRequire(import.meta.url);

// --- Dynamic JSON Schema to Zod Compiler ---
function compileZodSchema(parameters: any): z.ZodTypeAny {
    if (!parameters || !parameters.properties) return z.any();
    const shape: Record<string, z.ZodTypeAny> = {};
    for (const [key, prop] of Object.entries<any>(parameters.properties)) {
        let field: z.ZodTypeAny = z.any();
        
        if (prop.type === "string") {
            let strField = z.string();
            // Automatically apply min(1) for required strings to prevent empty bypass
            if (parameters.required?.includes(key)) {
                strField = strField.min(1, `${key} must not be empty`);
            }
            field = strField;
        } else if (prop.type === "number" || prop.type === "integer") {
            field = z.number();
        } else if (prop.type === "boolean") {
            field = z.boolean();
        } else if (prop.type === "array") {
            field = z.array(z.any());
        }

        if (prop.enum && Array.isArray(prop.enum) && prop.enum.length > 0) {
            field = z.enum(prop.enum as [string, ...string[]]);
        }

        if (!parameters.required?.includes(key)) {
            field = field.optional();
        }
        shape[key] = field;
    }
    return z.object(shape).strict(); // Prevent unknown arguments
}
// -------------------------------------------

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

    /** Expose skill metadata (search_keywords, isCoreSkill, kit, etc.) that MCP protocol strips out */
    public getSkillMetadata(): Map<string, AgentSkill> {
        return this.skillCache;
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

        const files = await fs.readdir(skillsDir, { recursive: true });
        for (const file of files) {
            if ((file.endsWith(".ts") || file.endsWith(".js")) && !file.endsWith("index.ts") && !file.endsWith("index.js")) {
                const skillPath = path.join(skillsDir, file);
                try {
                    // Try dynamic import using standard file URL conversion
                    const fileUrl = pathToFileURL(skillPath).href + `?v=${Date.now()}`;
                    const module = await import(fileUrl);
                    if (module.metadata && module.execute) {
                        // [Phase 4] Zod validation gate — reject malformed skills at load time
                        const validated = validateSkillMetadata(module.metadata, file);
                        if (!validated) {
                            logger.warn(`[MCPServer] Skill ${file} rejected: invalid metadata`);
                            continue;
                        }
                        this.skillCache.set(validated.name, {
                            name: validated.name,
                            description: validated.description,
                            parameters: validated.parameters,
                            search_keywords: validated.search_keywords,
                            isCoreSkill: validated.isCoreSkill || false,
                            category: validated.category as SkillCategory,
                            semantic_tags: validated.semantic_tags,
                            requires_hitl: validated.requires_hitl,
                            is_cpu_heavy: validated.is_cpu_heavy,
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
                            // [Phase 4] Zod validation gate (require fallback path)
                            const validated = validateSkillMetadata(module.metadata, file);
                            if (!validated) {
                                logger.warn(`[MCPServer] Skill ${file} rejected (require path): invalid metadata`);
                                continue;
                            }
                            this.skillCache.set(validated.name, {
                                name: validated.name,
                                description: validated.description,
                                parameters: validated.parameters,
                                search_keywords: validated.search_keywords,
                                isCoreSkill: validated.isCoreSkill || false,
                                category: validated.category as SkillCategory,
                                semantic_tags: validated.semantic_tags,
                                requires_hitl: validated.requires_hitl,
                                is_cpu_heavy: validated.is_cpu_heavy,
                                execute: module.execute,
                            });
                        }
                    } catch (err: unknown) {
                    const errMsg = err instanceof Error ? err.message : String(err);
                        logger.error(`[MCPServer] Failed to map skill ${file}: ${errMsg}`);
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
            const rawArgs = request.params.arguments || {};
            
            const skill = this.skillCache.get(toolName);
            if (!skill || !skill.execute) {
                throw new Error(`[MCPServer] Tool '${toolName}' not found or not executable.`);
            }

            try {
                // [AST Surgery] Strict Zod Validation Boundary
                logger.info(`[MCPServer] Validating arguments for tool: ${toolName}`);
                const compiledSchema = compileZodSchema(skill.parameters);
                const validatedArgs = compiledSchema.parse(rawArgs);

                logger.info(`[MCPServer] Executing tool: ${toolName}`);
                const result = await skill.execute(validatedArgs);
                
                const textContent = typeof result === "string" ? result : JSON.stringify(result, null, 2);
                return {
                    content: [{ type: "text", text: textContent }],
                    isError: false,
                };
            } catch (error: unknown) {
                const errMsg = error instanceof Error ? error.message : String(error);
                logger.error(`[MCPServer] Tool execution error for ${toolName}: ${errMsg}`);
                return {
                    content: [{ type: "text", text: `Error: ${errMsg}` }],
                    isError: true,
                };
            }
        });
    }
}
