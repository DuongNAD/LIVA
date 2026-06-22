import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { SSEClientTransport } from "@modelcontextprotocol/sdk/client/sse.js";
import { logger } from "../utils/logger";
import { AgentSkill, SkillCategory } from "../skills/SkillMetadata";

export interface MCPHostClientConfig {
    id: string;
    name: string;
    transportType: "stdio" | "sse";
    stdioConfig?: {
        command: string;
        args: string[];
        env?: Record<string, string>;
    };
    sseConfig?: {
        url: string;
    };
}

export interface MCPToolInfo {
    name: string;
    description?: string;
    inputSchema: {
        type: "object";
        properties?: Record<string, unknown>;
        required?: string[];
        [key: string]: unknown;
    };
    [key: string]: unknown;
}

export class MCPHostClient {
    private client: Client | null = null;
    private transport: StdioClientTransport | SSEClientTransport | null = null;
    private readonly config: MCPHostClientConfig;

    constructor(config: MCPHostClientConfig) {
        this.config = config;
    }

    public async connect(): Promise<void> {
        if (this.client) {
            logger.warn(`[MCPHostClient] Client ${this.config.id} already connected.`);
            return;
        }

        logger.info(`[MCPHostClient] Connecting to server ${this.config.id} via ${this.config.transportType}...`);

        try {
            if (this.config.transportType === "stdio") {
                if (!this.config.stdioConfig?.command) {
                    throw new Error("Missing 'command' for Stdio transport");
                }
                const env = {
                    ...process.env,
                    ...(this.config.stdioConfig.env || {})
                } as Record<string, string>;

                this.transport = new StdioClientTransport({
                    command: this.config.stdioConfig.command,
                    args: this.config.stdioConfig.args || [],
                    env
                });
            } else if (this.config.transportType === "sse") {
                if (!this.config.sseConfig?.url) {
                    throw new Error("Missing 'url' for SSE transport");
                }
                this.transport = new SSEClientTransport(new URL(this.config.sseConfig.url));
            } else {
                throw new Error(`Unsupported transport type: ${this.config.transportType}`);
            }

            this.client = new Client(
                {
                    name: this.config.name,
                    version: "1.0.0"
                },
                {
                    capabilities: {}
                }
            );

            await this.client.connect(this.transport);
            logger.info(`[MCPHostClient] Connected successfully to ${this.config.id}`);
        } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
            logger.error(`[MCPHostClient] Connection failed for ${this.config.id}: ${errMsg}`);
            this.client = null;
            this.transport = null;
            throw error;
        }
    }

    public async listRemoteTools(): Promise<MCPToolInfo[]> {
        if (!this.client) {
            throw new Error(`[MCPHostClient] Client ${this.config.id} is not connected.`);
        }
        try {
            const response = await this.client.listTools();
            return (response.tools || []) as MCPToolInfo[];
        } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
            logger.error(`[MCPHostClient] Failed to list tools for ${this.config.id}: ${errMsg}`);
            throw error;
        }
    }

    public async executeRemoteTool(toolName: string, args: Record<string, unknown> | undefined): Promise<unknown> {
        if (!this.client) {
            throw new Error(`[MCPHostClient] Client ${this.config.id} is not connected.`);
        }
        try {
            const result = await this.client.callTool({
                name: toolName,
                arguments: args
            });
            return result;
        } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
            logger.error(`[MCPHostClient] Failed to execute tool ${toolName} on ${this.config.id}: ${errMsg}`);
            throw error;
        }
    }

    public async registerAsAgentSkills(): Promise<AgentSkill[]> {
        const tools = await this.listRemoteTools();
        return tools.map((tool) => {
            return {
                name: tool.name,
                description: tool.description || "",
                short_desc: tool.description ? tool.description.substring(0, 80) : "",
                category: "core" as SkillCategory,
                parameters: tool.inputSchema,
                execute: async (args: Record<string, unknown>) => {
                    return this.executeRemoteTool(tool.name, args);
                }
            };
        });
    }

    public async disconnect(): Promise<void> {
        logger.info(`[MCPHostClient] Disconnecting from server ${this.config.id}...`);
        if (this.client) {
            try {
                await this.client.close();
            } catch (error: unknown) {
                const errMsg = error instanceof Error ? error.message : String(error);
                logger.error(`[MCPHostClient] Error closing client for ${this.config.id}: ${errMsg}`);
            }
            this.client = null;
        }
        this.transport = null;
    }
}
