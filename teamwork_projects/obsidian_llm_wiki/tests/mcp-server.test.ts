import { promises as fsp } from 'fs';
import * as path from 'path';
import * as os from 'os';
import { fileURLToPath } from 'url';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { createMcpServer } from '../src/server.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

interface ToolTextContent {
  type: 'text';
  text: string;
}

interface CallToolSuccessResult {
  content: ToolTextContent[];
  isError?: boolean;
}

describe('LIVA-Obsidian MCP Server Integration Tests', () => {
  let tempVaultPath: string;
  let client: Client;
  let mcpServer: any;
  let clientTransport: InMemoryTransport;
  let serverTransport: InMemoryTransport;
  let cleanupServer: () => void;

  beforeEach(async () => {
    // Create a unique temporary directory for the mock vault
    tempVaultPath = await fsp.mkdtemp(path.join(os.tmpdir(), 'mcp-vault-test-'));
    
    // Copy sample vault contents to the temp vault directory
    const sourceVault = path.resolve(__dirname, '../vault');
    await fsp.cp(sourceVault, tempVaultPath, { recursive: true });

    // Instantiate MCP server configured with the temp vault path
    const serverResult = createMcpServer(tempVaultPath);
    mcpServer = serverResult.mcpServer;
    cleanupServer = serverResult.cleanup;

    // Create linked transports for in-memory communication
    [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();

    // Connect the server to its transport
    await mcpServer.connect(serverTransport);

    // Connect the client to its transport
    client = new Client(
      {
        name: 'test-client',
        version: '1.0.0'
      },
      {
        capabilities: {}
      }
    );
    await client.connect(clientTransport);
  });

  afterEach(async () => {
    if (client) {
      await client.close();
    }
    if (mcpServer) {
      await mcpServer.close();
    }
    if (cleanupServer) {
      cleanupServer();
    }
    await fsp.rm(tempVaultPath, { recursive: true, force: true });
  });

  test('The MCP server starts, connects successfully, and lists registered tools', async () => {
    const toolsResult = await client.listTools();
    expect(toolsResult.tools).toBeDefined();
    
    const toolNames = toolsResult.tools.map(t => t.name);
    expect(toolNames).toContain('read_markdown');
    expect(toolNames).toContain('write_markdown');
    expect(toolNames).toContain('search_vault');
  });

  test('Searching returns correct results from sample vault data', async () => {
    const rawResult = await client.callTool({
      name: 'search_vault',
      arguments: {
        query: 'architecture'
      }
    });

    const searchResult = rawResult as unknown as CallToolSuccessResult;
    expect(searchResult.isError).toBeFalsy();
    expect(searchResult.content).toBeDefined();
    expect(searchResult.content[0].type).toBe('text');
    
    const results = JSON.parse(searchResult.content[0].text);
    expect(results.length).toBeGreaterThan(0);
    
    const firstResult = results[0];
    expect(firstResult.path).toContain('liva_architecture.md');
    expect(firstResult.title).toBe('liva_architecture');
  });

  test('Creating and then reading files works correctly via the MCP server', async () => {
    const newFilePath = 'Knowledge/test_integration.md';
    const fileContent = `---
title: "test_integration"
tags:
  - liva/knowledge
author: "Test Suite"
last_update: "2026-06-21T08:00:00Z"
---

# Integration Test File
This is some content for testing.
`;

    // 1. Write the file
    const rawWriteResult = await client.callTool({
      name: 'write_markdown',
      arguments: {
        path: newFilePath,
        content: fileContent
      }
    });

    const writeResult = rawWriteResult as unknown as CallToolSuccessResult;
    expect(writeResult.isError).toBeFalsy();
    expect(writeResult.content[0].text).toContain('File written successfully');

    // 2. Read the file
    const rawReadResult = await client.callTool({
      name: 'read_markdown',
      arguments: {
        path: newFilePath
      }
    });

    const readResult = rawReadResult as unknown as CallToolSuccessResult;
    expect(readResult.isError).toBeFalsy();
    expect(readResult.content[0].text).toBe(fileContent);

    // 3. Search for the file content to verify indexing on write
    const rawSearchResult = await client.callTool({
      name: 'search_vault',
      arguments: {
        query: 'Integration Test File'
      }
    });

    const searchResult = rawSearchResult as unknown as CallToolSuccessResult;
    expect(searchResult.isError).toBeFalsy();
    const searchResults = JSON.parse(searchResult.content[0].text);
    expect(searchResults.length).toBeGreaterThan(0);
    expect(searchResults[0].path).toBe('Knowledge/test_integration.md');
  });

  test('The MCP server correctly rejects path traversal', async () => {
    const traversalPaths = [
      '../outside.md',
      'Knowledge/../../outside.md',
      'Skills/../../../outside.md'
    ];

    for (const badPath of traversalPaths) {
      const rawResult = await client.callTool({
        name: 'read_markdown',
        arguments: {
          path: badPath
        }
      });
      const readResult = rawResult as unknown as CallToolSuccessResult;
      expect(readResult.isError).toBe(true);
      expect(readResult.content[0].text).toContain('Access denied');
    }
  });

  test('The MCP server correctly rejects absolute paths outside root', async () => {
    const outsidePath = path.resolve(tempVaultPath, '../some_file.md');
    
    const rawResult = await client.callTool({
      name: 'read_markdown',
      arguments: {
        path: outsidePath
      }
    });

    const readResult = rawResult as unknown as CallToolSuccessResult;
    expect(readResult.isError).toBe(true);
    expect(readResult.content[0].text).toContain('Access denied');
  });

  test('The MCP server correctly rejects symlinks that resolve outside root', async () => {
    const tempOutsideFile = path.join(os.tmpdir(), `outside-file-${Date.now()}.md`);
    await fsp.writeFile(tempOutsideFile, 'Secret content outside vault', 'utf8');

    try {
      const symlinkPath = path.join(tempVaultPath, 'Skills', 'symlink_outside.md');
      
      let symlinkCreated = false;
      try {
        await fsp.symlink(tempOutsideFile, symlinkPath, 'file');
        symlinkCreated = true;
      } catch (err: any) {
        console.warn('Skipping symlink test assertion: symlink creation failed due to OS permissions:', err.message);
      }

      if (symlinkCreated) {
        const rawResult = await client.callTool({
          name: 'read_markdown',
          arguments: {
            path: 'Skills/symlink_outside.md'
          }
        });

        const readResult = rawResult as unknown as CallToolSuccessResult;
        expect(readResult.isError).toBe(true);
        expect(readResult.content[0].text).toContain('Access denied');
      }
    } finally {
      await fsp.rm(tempOutsideFile, { force: true });
    }
  });
});
