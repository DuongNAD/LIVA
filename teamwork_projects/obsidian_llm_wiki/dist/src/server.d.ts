import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
/**
 * Creates and configures the LIVA-Obsidian MCP Server with tools.
 *
 * @param vaultRoot Absolute path to the Obsidian Vault root.
 * @returns An configured McpServer instance.
 */
export declare function createMcpServer(vaultRoot: string): {
    mcpServer: McpServer;
    cleanup: () => void;
};
