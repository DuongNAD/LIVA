import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { SSEClientTransport } from "@modelcontextprotocol/sdk/client/sse.js";
import { logger } from "../utils/logger";

export interface MCPServerConfig {
    id: string;
    type: "stdio" | "sse";
    command?: string; // for stdio
    args?: string[];  // for stdio
    env?: Record<string, string>; // for stdio
    url?: string;     // for sse
}

export class MCPClientManager {
    private static instance: MCPClientManager;
    private clients = new Map<string, Client>();
    private transports = new Map<string, StdioClientTransport | SSEClientTransport>();

    private constructor() {}

    public static getInstance(): MCPClientManager {
        if (!MCPClientManager.instance) {
            MCPClientManager.instance = new MCPClientManager();
        }
        return MCPClientManager.instance;
    }

    /**
     * Connect to an MCP server using either stdio or sse transport.
     */
    public async connectServer(config: MCPServerConfig): Promise<Client> {
        if (this.clients.has(config.id)) {
            logger.warn(`[MCPClientManager] Server ${config.id} đã kết nối, trả về client hiện tại.`);
            return this.clients.get(config.id)!;
        }

        logger.info(`[MCPClientManager] Đang khởi tạo kết nối MCP Server: ${config.id} qua ${config.type}...`);
        
        let transport: StdioClientTransport | SSEClientTransport;

        try {
            if (config.type === "stdio") {
                if (!config.command) throw new Error("Thiếu tham số 'command' cho Stdio transport");
                transport = new StdioClientTransport({
                    command: config.command,
                    args: config.args || [],
                    env: { ...(process.env as Record<string, string>), ...config.env }
                });
            } else if (config.type === "sse") {
                if (!config.url) throw new Error("Thiếu tham số 'url' cho SSE transport");
                transport = new SSEClientTransport(new URL(config.url));
            } else {
                throw new Error(`Loại kết nối không được hỗ trợ: ${config.type}`);
            }

            const client = new Client(
                {
                    name: "LIVA-Gateway",
                    version: "1.0.0"
                },
                {
                    capabilities: {}
                }
            );

            await client.connect(transport);
            logger.info(`[MCPClientManager] ✅ Kết nối thành công MCP Server: ${config.id}`);
            
            this.clients.set(config.id, client);
            this.transports.set(config.id, transport);
            
            return client;
        } catch (error: any) {
            logger.error(`[MCPClientManager] ❌ Lỗi kết nối MCP Server ${config.id}: ${error.message}`);
            throw error;
        }
    }

    /**
     * Retrieve all available tools from a specific connected server.
     */
    public async getServerTools(serverId: string) {
        const client = this.clients.get(serverId);
        if (!client) throw new Error(`Server ${serverId} chưa được kết nối.`);
        
        const response = await client.listTools();
        return response.tools;
    }

    /**
     * Retrieve all tools across all connected servers.
     */
    public async getAllConnectedTools() {
        let allTools: any[] = [];
        for (const [id, client] of this.clients.entries()) {
            try {
                const response = await client.listTools();
                const toolsWithServerId = response.tools.map(t => ({ ...t, _serverId: id }));
                allTools = allTools.concat(toolsWithServerId);
            } catch (e: any) {
                logger.warn(`[MCPClientManager] Không thể lấy listTools từ server ${id}: ${e.message}`);
            }
        }
        return allTools;
    }

    /**
     * Execute a tool on a specific server.
     */
    public async executeTool(serverId: string, toolName: string, args: any) {
        const client = this.clients.get(serverId);
        if (!client) throw new Error(`Server ${serverId} chưa được kết nối.`);
        
        const result = await client.callTool({
            name: toolName,
            arguments: args
        });
        
        return result;
    }

    public getClient(id: string): Client | undefined {
        return this.clients.get(id);
    }

    /**
     * Disconnect and cleanup all connections.
     */
    public async disconnectAll() {
        logger.info(`[MCPClientManager] Đang đóng toàn bộ kết nối MCP...`);
        for (const [id, client] of this.clients.entries()) {
            try {
                await client.close();
                logger.info(`[MCPClientManager] Đã đóng server ${id}`);
            } catch (e: any) {
                logger.error(`[MCPClientManager] Lỗi khi đóng server ${id}: ${e.message}`);
            }
        }
        this.clients.clear();
        this.transports.clear();
    }
}
