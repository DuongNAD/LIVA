import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { LocalMCPServer } from "./LocalMCPServer";
import { logger } from "../utils/logger";

/**
 * MCPHost
 * =======
 * Connects to the LocalMCPServer via InMemoryTransport.
 * Eliminates stdio/http overhead, providing lightning-fast tool discovery and execution.
 */
export class MCPHost {
    private readonly client: Client;
    private readonly localServer: LocalMCPServer;
    private readonly clientTransport: any;
    private readonly serverTransport: any;
    private isConnected = false;

    constructor() {
        this.localServer = new LocalMCPServer();
        
        // Initialize InMemoryTransport pair
        // The first element is the client transport, the second is the server transport.
        const [clientT, serverT] = InMemoryTransport.createLinkedPair();
        this.clientTransport = clientT;
        this.serverTransport = serverT;

        this.client = new Client({
            name: "liva-gateway-host",
            version: "1.0.0",
        }, {
            capabilities: {}
        });
    }

    /**
     * Bootstraps the internal server, connects the transports, and initializes the client.
     */
    public async initialize() {
        if (this.isConnected) return;

        // 1. Load legacy skills into the MCP server
        await this.localServer.loadSkills();

        // 2. Connect the server to its transport
        await this.localServer.getServerInstance().connect(this.serverTransport);

        // 3. Connect the client to its transport and initialize MCP session
        await this.client.connect(this.clientTransport);
        this.isConnected = true;

        logger.info("[MCPHost] In-Memory Transport connected successfully.");
    }

    /**
     * Lists all available tools from the MCP Server schema.
     */
    public async listTools() {
        if (!this.isConnected) {
            throw new Error("[MCPHost] Client is not connected. Call initialize() first.");
        }
        
        const response = await this.client.listTools();
        return response.tools;
    }

    /**
     * Executes a tool via the MCP protocol.
     */
    public async callTool(toolName: string, args: Record<string, any>): Promise<any> {
        if (!this.isConnected) {
            throw new Error("[MCPHost] Client is not connected. Call initialize() first.");
        }

        logger.info(`[MCPHost] Dispatching CallToolRequest for: ${toolName}`);
        
        const response = await this.client.callTool({
            name: toolName,
            arguments: args
        });

        if (response.isError) {
            const errorMsg = (response.content as any[])?.[0] as { type: string, text: string };
            logger.error(`[MCPHost] Tool '${toolName}' failed: ${errorMsg?.text}`);
            throw new Error(errorMsg?.text || `Tool ${toolName} failed with unknown error.`);
        }

        // Return the raw text content format expected by legacy integrations
        const content = (response.content as any[])?.[0] as { type: string, text: string };
        return content?.text || "Success (No content)";
    }
}
