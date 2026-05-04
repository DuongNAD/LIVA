import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SkillRegistry } from '../../src/SkillRegistry';
import { MCPClientManager } from '../../src/mcp/MCPClientManager';
import { EmbeddingService } from '../../src/services/EmbeddingService';
import * as fsp from 'fs/promises';

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
        getAllConnectedTools: vi.fn(),
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
        embed: vi.fn().mockResolvedValue([0.1, 0.2])
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

// We need to mock GeminiSurfer import
vi.mock('../../src/skills/GeminiSurfer.js', () => ({
    metadata: { name: 'gemini_surfer', description: 'Surfer' },
    execute: vi.fn()
}));

describe('SkillRegistry', () => {
    let registry: SkillRegistry;
    let mockMcpManager: any;
    let mockEmbedSvc: any;

    beforeEach(() => {
        vi.clearAllMocks();
        registry = new SkillRegistry();
        mockMcpManager = MCPClientManager.getInstance();
        mockEmbedSvc = EmbeddingService.getInstance();

        // Reset implementations to default success state
        mockMcpManager.connectServer.mockResolvedValue(undefined);
        mockMcpManager.getAllConnectedTools.mockResolvedValue([]);
        mockMcpManager.executeTool.mockResolvedValue({});
    });

    describe('registerLocalSkills', () => {
        it('should connect to local adapter and fetch tools', async () => {
            mockMcpManager.getAllConnectedTools.mockResolvedValue([
                { name: 'mcp_tool', description: 'desc', inputSchema: {} }
            ]);

            await registry.registerLocalSkills();

            expect(mockMcpManager.connectServer).toHaveBeenCalledWith(expect.objectContaining({ id: 'liva-legacy-adapter' }));
            const allSkills = registry.getAllSkills();
            expect(allSkills).toContainEqual(expect.objectContaining({ name: 'mcp_tool' }));
        });

        it('should catch and log error on init failure', async () => {
            mockMcpManager.connectServer.mockRejectedValueOnce(new Error('MCP Init failed'));
            
            await registry.registerLocalSkills();
            
            // Should not throw, but just log error
            const { logger } = await import('../../src/utils/logger');
            expect(logger.error).toHaveBeenCalledWith(expect.stringContaining('MCP Init Error: MCP Init failed'));
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
            mockMcpManager.getAllConnectedTools.mockResolvedValue([
                { name: 'mcp_tool', description: 'desc', _serverId: 'server1', inputSchema: {} }
            ]);
            await registry.registerLocalSkills();

            mockMcpManager.executeTool.mockResolvedValue({
                isError: false,
                content: [{ type: 'text', text: 'MCP Result' }]
            });

            const result = await registry.executeSkill('mcp_tool', { arg: 1 });
            
            expect(mockMcpManager.executeTool).toHaveBeenCalledWith('server1', 'mcp_tool', { arg: 1 });
            expect(result).toBe('MCP Result');
        });

        it('should return default success if no content array', async () => {
            mockMcpManager.getAllConnectedTools.mockResolvedValue([
                { name: 'mcp_tool', description: 'desc', _serverId: 'server1', inputSchema: {} }
            ]);
            await registry.registerLocalSkills();

            mockMcpManager.executeTool.mockResolvedValue({
                isError: false,
            });

            const result = await registry.executeSkill('mcp_tool', { arg: 1 });
            expect(result).toBe('Success (No content)');
        });

        it('should throw error if MCP tool returns isError', async () => {
            mockMcpManager.getAllConnectedTools.mockResolvedValue([
                { name: 'mcp_tool', description: 'desc', _serverId: 'server1', inputSchema: {} }
            ]);
            await registry.registerLocalSkills();

            mockMcpManager.executeTool.mockResolvedValue({
                isError: true,
                content: [{ type: 'text', text: 'MCP Error Occurred' }]
            });

            await expect(registry.executeSkill('mcp_tool', {})).rejects.toThrow('MCP Error Occurred');
        });

        it('should throw error if MCP tool execution fails abruptly', async () => {
            mockMcpManager.getAllConnectedTools.mockResolvedValue([
                { name: 'mcp_tool', description: 'desc', _serverId: 'server1', inputSchema: {} }
            ]);
            await registry.registerLocalSkills();

            mockMcpManager.executeTool.mockRejectedValue(new Error('Execution abruptly failed'));

            await expect(registry.executeSkill('mcp_tool', {})).rejects.toThrow("MCP Tool 'mcp_tool' execution failed: Execution abruptly failed");
        });

        it('should throw error if tool not found', async () => {
            await expect(registry.executeSkill('unknown_tool', {})).rejects.toThrow("MCP Tool 'unknown_tool' không tồn tại");
        });
    });

    describe('BuiltIn Skills', () => {
        it('get_current_time should execute correctly', async () => {
            const result = await registry.executeSkill('get_current_time', { timezone: 'UTC' });
            expect(result).toBeDefined();
            expect(typeof result).toBe('string');
        });

        it('get_current_time should use local timezone if not provided', async () => {
            const result = await registry.executeSkill('get_current_time', {});
            expect(result).toBeDefined();
            expect(typeof result).toBe('string');
        });

        it('read_file should execute correctly', async () => {
            (fsp.readFile as any).mockResolvedValue('file content');

            const result = await registry.executeSkill('read_file', { path: 'test.txt' });
            expect(result).toBe('file content');
            expect(fsp.readFile).toHaveBeenCalledWith('test.txt', 'utf8');
        });

        it('read_file should return error string if fails', async () => {
            (fsp.readFile as any).mockRejectedValue(new Error('ENOENT'));

            const result = await registry.executeSkill('read_file', { path: 'missing.txt' });
            expect(result).toBe('Lỗi khi đọc tệp: ENOENT');
        });
    });

    describe('getSemanticTopK', () => {
        it('should filter by activeKit and return scored skills', async () => {
            registry.registerSkill({
                name: 'skill1',
                description: 'test1',
                parameters: {},
                kit: 'DEVELOPER_KIT' as any
            });

            registry.registerSkill({
                name: 'skill2',
                description: 'test2',
                parameters: {},
                kit: 'SOCIAL_KIT' as any
            });

            const top = await registry.getSemanticTopK('query', 'DEVELOPER_KIT' as any, 10);
            
            expect(top.find(s => s.name === 'skill1')).toBeDefined();
            expect(top.find(s => s.name === 'skill2')).toBeUndefined();
        });

        it('should use embedding cache for descriptions', async () => {
            registry.registerSkill({
                name: 'skill1',
                description: 'test1',
                parameters: {}
            });

            await registry.getSemanticTopK('query');
            expect(mockEmbedSvc.embed).toHaveBeenCalled();

            mockEmbedSvc.embed.mockClear();
            await registry.getSemanticTopK('query');
            expect(mockEmbedSvc.embed).not.toHaveBeenCalled(); // Cached
        });

        it('should fast-exit and return core skills if query is empty', async () => {
            const top = await registry.getSemanticTopK('   ');
            expect(top).toBeDefined();
            // All returned skills should be core skills
            expect(top.every(s => s.isCoreSkill)).toBe(true);
        });

        it('should return all skills if embedding fails', async () => {
            mockEmbedSvc.embedWithTimeout.mockRejectedValueOnce(new Error('Timeout'));
            const top = await registry.getSemanticTopK('query');
            expect(top).toBeDefined();
            const { logger } = await import('../../src/utils/logger');
            expect(logger.warn).toHaveBeenCalledWith(expect.stringContaining('Embedding failed'));
        });

        it('should return only core skills if no tools above threshold', async () => {
            // Mock cosineSimilarity to return 0.1 (below 0.65)
            const { cosineSimilarity } = await import('../../src/utils/VectorMath');
            (cosineSimilarity as any).mockReturnValue(0.1);
            
            registry.registerSkill({
                name: 'skill_low_score',
                description: 'test_low',
                parameters: {}
            });
            
            const top = await registry.getSemanticTopK('query');
            expect(top.find(s => s.name === 'skill_low_score')).toBeUndefined();
            const { logger } = await import('../../src/utils/logger');
            expect(logger.debug).toHaveBeenCalledWith(expect.stringContaining('No tools above threshold'));
            
            (cosineSimilarity as any).mockReturnValue(0.9); // Restore for other tests
        });
    });
});
