import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { z } from 'zod';
import { VaultManager } from './vault.js';

/**
 * Creates and configures the LIVA-Obsidian MCP Server with tools.
 * 
 * @param vaultRoot Absolute path to the Obsidian Vault root.
 * @returns An configured McpServer instance.
 */
export function createMcpServer(
  vaultRoot: string
): { mcpServer: McpServer; cleanup: () => void } {
  const vaultManager = new VaultManager(vaultRoot);

  const mcpServer = new McpServer({
    name: 'liva-obsidian-mcp-server',
    version: '1.0.0'
  });

  // 1. read_markdown tool
  mcpServer.registerTool(
    'read_markdown',
    {
      description: 'Read the markdown file content within the vault. The path must be relative to the vault root.',
      inputSchema: {
        path: z.string().min(1).describe('Relative path from vault root to the markdown file (e.g., "Skills/web_search.md")')
      }
    },
    async ({ path }) => {
      try {
        const content = vaultManager.readMarkdown(path);
        return {
          content: [
            {
              type: 'text',
              text: content
            }
          ]
        };
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : String(err);
        throw new Error(message);
      }
    }
  );

  // 2. write_markdown tool
  mcpServer.registerTool(
    'write_markdown',
    {
      description: 'Create or update a markdown file with content (including YAML frontmatter) in the vault.',
      inputSchema: {
        path: z.string().min(1).describe('Relative path from vault root to save the markdown file (e.g., "Knowledge/new_topic.md")'),
        content: z.string().describe('Complete file content containing markdown and standard frontmatter')
      }
    },
    async ({ path, content }) => {
      try {
        const success = vaultManager.writeMarkdown(path, content);
        return {
          content: [
            {
              type: 'text',
              text: success ? 'File written successfully' : 'Failed to write file'
            }
          ]
        };
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : String(err);
        throw new Error(message);
      }
    }
  );

  // 3. search_vault tool
  mcpServer.registerTool(
    'search_vault',
    {
      description: 'Search for markdown files inside the vault by keywords and metadata fields.',
      inputSchema: {
        query: z.string().describe('Search query, e.g. "architecture" or "author:explorer tags:liva/rule"')
      }
    },
    async ({ query }) => {
      try {
        const results = vaultManager.searchVault(query);
        return {
          content: [
            {
              type: 'text',
              text: JSON.stringify(results, null, 2)
            }
          ]
        };
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : String(err);
        throw new Error(message);
      }
    }
  );

  return {
    mcpServer,
    cleanup: () => {
      vaultManager.close();
    }
  };
}
