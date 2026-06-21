import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { createMcpServer } from './server.js';
import * as path from 'path';
import { fileURLToPath } from 'url';
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
// 1. Intercept console logs to stderr so they do not corrupt the stdio JSON-RPC transport stream.
const originalLog = console.log;
const originalInfo = console.info;
const originalWarn = console.warn;
console.log = (...args) => {
    console.error(...args);
};
console.info = (...args) => {
    console.error(...args);
};
console.warn = (...args) => {
    console.error(...args);
};
async function main() {
    // 2. Handle process args / environment variables, with fallback to default vault path
    const vaultPathArg = process.argv[2];
    const vaultPathEnv = process.env.OBSIDIAN_VAULT_PATH;
    const defaultVaultPath = path.resolve(__dirname, '../vault');
    const vaultRoot = vaultPathArg || vaultPathEnv || defaultVaultPath;
    console.error(`Initializing LIVA-Obsidian MCP Server...`);
    console.error(`Vault Root: ${vaultRoot}`);
    // 3. Instantiate the server and get the cleanup method
    const { mcpServer, cleanup } = createMcpServer(vaultRoot);
    // 4. Configure Stdio transport
    const transport = new StdioServerTransport();
    // 5. Manage clean shutdown
    let isShuttingDown = false;
    const shutdown = async () => {
        if (isShuttingDown)
            return;
        isShuttingDown = true;
        console.error('Shutting down LIVA-Obsidian MCP Server gracefully...');
        try {
            cleanup();
            await mcpServer.close();
            console.error('Shutdown complete.');
        }
        catch (err) {
            console.error('Error during shutdown:', err);
        }
        finally {
            process.exit(0);
        }
    };
    process.on('SIGINT', () => {
        console.error('Received SIGINT');
        shutdown();
    });
    process.on('SIGTERM', () => {
        console.error('Received SIGTERM');
        shutdown();
    });
    process.stdin.on('close', () => {
        console.error('stdin closed');
        shutdown();
    });
    // 6. Connect and run
    await mcpServer.connect(transport);
    console.error('LIVA-Obsidian MCP Server running on stdio transport.');
}
main().catch((error) => {
    console.error('Fatal server error:', error);
    process.exit(1);
});
//# sourceMappingURL=index.js.map