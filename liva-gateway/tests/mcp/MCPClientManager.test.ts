import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================================
// Mock MCP SDK to prevent real connections
// ============================================================
vi.mock("@modelcontextprotocol/sdk/client/index.js", () => ({
    Client: vi.fn().mockImplementation(() => ({
        connect: vi.fn().mockResolvedValue(undefined),
        close: vi.fn().mockResolvedValue(undefined),
        listTools: vi.fn().mockResolvedValue({ tools: [] }),
        callTool: vi.fn().mockResolvedValue({ content: [{ type: "text", text: "result" }] }),
    })),
}));

vi.mock("@modelcontextprotocol/sdk/client/stdio.js", () => ({
    StdioClientTransport: vi.fn().mockImplementation(() => ({})),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { MCPClientManager } from "../../src/mcp/MCPClientManager";

// ============================================================
// Tests
// ============================================================
describe("MCPClientManager", () => {
    afterEach(() => {
        // Reset singleton state between tests
        // @ts-ignore — accessing private static for test cleanup
        MCPClientManager.instance = undefined;
    });

    describe("Singleton Pattern", () => {
        it("should return the same instance on multiple calls", () => {
            const a = MCPClientManager.getInstance();
            const b = MCPClientManager.getInstance();
            expect(a).toBe(b);
        });
    });

    describe("Instance Methods", () => {
        it("should be constructable and have expected methods", () => {
            const manager = MCPClientManager.getInstance();
            expect(manager).toBeDefined();
            expect(typeof manager.connectServer).toBe("function");
            expect(typeof manager.getServerTools).toBe("function");
            expect(typeof manager.executeTool).toBe("function");
            expect(typeof manager.getAllConnectedTools).toBe("function");
            expect(typeof manager.disconnectAll).toBe("function");
            expect(typeof manager.getClient).toBe("function");
        });
    });
});
