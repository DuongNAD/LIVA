import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SkillRegistry } from '../../src/SkillRegistry';
import { MCPClientManager } from '../../src/mcp/MCPClientManager';
import { EmbeddingService } from '../../src/services/EmbeddingService';

vi.mock('../../src/utils/logger', () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn()
    }
}));

vi.mock('../../src/mcp/MCPClientManager', () => {
    const mockMcpManager = {
        connectServer: vi.fn(),
        getAllConnectedTools: vi.fn().mockResolvedValue([]),
        executeTool: vi.fn()
    };
    return {
        MCPClientManager: {
            getInstance: vi.fn().mockReturnValue(mockMcpManager)
        }
    };
});

vi.mock('../../src/services/EmbeddingService', () => {
    const mockEmbedSvc = {
        embedWithTimeout: vi.fn().mockResolvedValue([0.1, 0.2]),
        embed: vi.fn().mockResolvedValue([0.1, 0.2]),
        ready: true
    };
    return {
        EmbeddingService: {
            getInstance: vi.fn().mockReturnValue(mockEmbedSvc)
        }
    };
});

vi.mock('../../src/utils/VectorMath', () => ({
    cosineSimilarity: vi.fn().mockReturnValue(0.9)
}));

vi.mock('fs/promises', () => ({
    readFile: vi.fn()
}));

// ── Mock the entire in-process MCP chain ──
// vi.hoisted ensures these are available when vi.mock is hoisted to the top
const {
    mockLoadSkills,
    mockServerConnect,
    mockGetSkillMetadata,
    mockListTools,
    mockClientConnect
} = vi.hoisted(() => ({
    mockLoadSkills: vi.fn().mockResolvedValue(undefined),
    mockServerConnect: vi.fn().mockResolvedValue(undefined),
    mockGetSkillMetadata: vi.fn().mockReturnValue(new Map()),
    mockListTools: vi.fn().mockResolvedValue({ tools: [] }),
    mockClientConnect: vi.fn().mockResolvedValue(undefined)
}));

vi.mock('../../src/mcp/LocalMCPServer', () => ({
    LocalMCPServer: class MockLocalMCPServer {
        loadSkills = mockLoadSkills;
        getServerInstance() { return { connect: mockServerConnect }; }
        getSkillMetadata = mockGetSkillMetadata;
    }
}));

vi.mock('@modelcontextprotocol/sdk/client/index.js', () => ({
    Client: class MockClient {
        connect = mockClientConnect;
        listTools = mockListTools;
    }
}));

vi.mock('@modelcontextprotocol/sdk/inMemory.js', () => ({
    InMemoryTransport: {
        createLinkedPair: vi.fn().mockReturnValue([{}, {}])
    }
}));

vi.mock('../../src/skills/GeminiSurfer.js', () => ({
    metadata: { name: 'gemini_surfer', description: 'Surfer' },
    execute: vi.fn()
}));

// Helper: inject MCP tools directly into registry (bypassing mock chain complexity)
function injectMcpTools(registry: SkillRegistry, tools: any[]) {
    (registry as any).mcpToolsList = tools.map(t => ({ ...t, _serverId: t._serverId || 'external-test-server' }));
}

describe('SkillRegistry', () => {
    let registry: SkillRegistry;
    let mockMcpManager: any;
    let mockEmbedSvc: any;

    beforeEach(() => {
        vi.clearAllMocks();
        registry = new SkillRegistry();
        mockMcpManager = MCPClientManager.getInstance();
        mockEmbedSvc = EmbeddingService.getInstance();
        mockMcpManager.executeTool.mockResolvedValue({});
    });

    describe('registerLocalSkills', () => {
        it('should connect to local adapter and fetch tools', async () => {
            mockListTools.mockResolvedValue({
                tools: [{ name: 'mcp_tool', description: 'desc', inputSchema: {} }]
            });

            await registry.registerLocalSkills();

            const allSkills = registry.getAllSkills();
            expect(allSkills).toContainEqual(expect.objectContaining({ name: 'mcp_tool' }));
        });

        it('should catch and log error on init failure', async () => {
            mockLoadSkills.mockRejectedValueOnce(new Error('MCP Init failed'));
            
            await registry.registerLocalSkills();
            
            const { logger } = await import('../../src/utils/logger');
            expect(logger.error).toHaveBeenCalledWith(expect.stringContaining('MCP Init failed'));
        });
    });

    describe('executeSkill', () => {
        it('should execute fallback skill if available', async () => {
            const mockExecute = vi.fn().mockResolvedValue('Result');
            registry.registerSkill({
                name: 'test_skill',
                description: 'test',
                parameters: {},
                execute: mockExecute
            });

            const result = await registry.executeSkill('test_skill', { arg: 1 });
            expect(mockExecute).toHaveBeenCalledWith({ arg: 1 });
            expect(result).toBe('Result');
        });

        it('should execute MCP tool if not fallback', async () => {
            injectMcpTools(registry, [{ name: 'mcp_tool', description: 'desc', inputSchema: {} }]);

            mockMcpManager.executeTool.mockResolvedValue({
                isError: false,
                content: [{ type: 'text', text: 'MCP Result' }]
            });

            const result = await registry.executeSkill('mcp_tool', { arg: 1 });
            expect(mockMcpManager.executeTool).toHaveBeenCalledWith('external-test-server', 'mcp_tool', { arg: 1 });
            expect(result).toBe('MCP Result');
        });

        it('should return default success if no content array', async () => {
            injectMcpTools(registry, [{ name: 'mcp_tool', description: 'desc', inputSchema: {} }]);
            mockMcpManager.executeTool.mockResolvedValue({ isError: false });

            const result = await registry.executeSkill('mcp_tool', { arg: 1 });
            expect(result).toBe('Success (No content)');
        });

        it('should throw error if MCP tool returns isError', async () => {
            injectMcpTools(registry, [{ name: 'mcp_tool', description: 'desc', inputSchema: {} }]);

            mockMcpManager.executeTool.mockResolvedValue({
                isError: true,
                content: [{ type: 'text', text: 'MCP Error Occurred' }]
            });

            await expect(registry.executeSkill('mcp_tool', {})).rejects.toThrow('MCP Error Occurred');
        });

        it('should throw error if MCP tool execution fails abruptly', async () => {
            injectMcpTools(registry, [{ name: 'mcp_tool', description: 'desc', inputSchema: {} }]);
            mockMcpManager.executeTool.mockRejectedValue(new Error('Execution abruptly failed'));

            await expect(registry.executeSkill('mcp_tool', {})).rejects.toThrow("MCP Tool 'mcp_tool' execution failed: Execution abruptly failed");
        });

        it('should throw error if tool not found', async () => {
            await expect(registry.executeSkill('unknown_tool', {})).rejects.toThrow("không tồn tại");
        });
    });

    describe('BuiltIn Skills', () => {
        it('get_current_time should execute correctly', async () => {
            const result = await registry.executeSkill('get_current_time', { timezone: 'UTC' });
            expect(result).toBeDefined();
            expect(typeof result).toBe('string');
        });
    });

    describe('getSemanticTopK', () => {
        it('should filter by activeKit and return scored skills', async () => {
            mockEmbedSvc.ready = true;
            registry.registerSkill({ name: 'skill1', description: 'test1', parameters: {}, kit: 'DEVELOPER_KIT' as any });
            registry.registerSkill({ name: 'skill2', description: 'test2', parameters: {}, kit: 'SOCIAL_KIT' as any });

            const top = await registry.getSemanticTopK('query', 'DEVELOPER_KIT' as any, 10);
            expect(top.find(s => s.name === 'skill1')).toBeDefined();
            expect(top.find(s => s.name === 'skill2')).toBeUndefined();
        });

        it('should use embedding cache for descriptions', async () => {
            mockEmbedSvc.ready = true;
            registry.registerSkill({ name: 'skill1', description: 'test1', parameters: {} });

            await registry.getSemanticTopK('query');
            expect(mockEmbedSvc.embedWithTimeout).toHaveBeenCalled();

            mockEmbedSvc.embed.mockClear();
            await registry.getSemanticTopK('query2');
            expect(mockEmbedSvc.embed).not.toHaveBeenCalled(); // Desc cached
        });

        it('should fast-exit and return core skills if query is empty', async () => {
            const top = await registry.getSemanticTopK('   ');
            expect(top).toBeDefined();
            expect(top.every(s => s.isCoreSkill)).toBe(true);
        });

        it('should return all skills if embedding fails', async () => {
            mockEmbedSvc.ready = true;
            mockEmbedSvc.embedWithTimeout.mockRejectedValueOnce(new Error('Timeout'));
            const top = await registry.getSemanticTopK('query');
            expect(top).toBeDefined();
            const { logger } = await import('../../src/utils/logger');
            expect(logger.warn).toHaveBeenCalledWith(expect.stringContaining('Embedding failed'));
        });

        it('should return only core skills if no tools above threshold', async () => {
            mockEmbedSvc.ready = true;
            const { cosineSimilarity } = await import('../../src/utils/VectorMath');
            (cosineSimilarity as any).mockReturnValue(0.1);
            
            registry.registerSkill({ name: 'skill_low', description: 'test', parameters: {} });
            const top = await registry.getSemanticTopK('query');
            expect(top.find(s => s.name === 'skill_low')).toBeUndefined();
            
            const { logger } = await import('../../src/utils/logger');
            expect(logger.debug).toHaveBeenCalledWith(expect.stringContaining('No tools above threshold'));
            (cosineSimilarity as any).mockReturnValue(0.9);
        });
    });
});
