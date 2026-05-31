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
        embedBatch: vi.fn().mockResolvedValue([[0.1, 0.2]]),
        ensureReady: vi.fn().mockResolvedValue(undefined),
        ready: true,
        isVramYielded: vi.fn().mockReturnValue(false)
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
const {
    mockLoadSkills,
    mockServerConnect,
    mockGetSkillMetadata,
    mockListTools,
    mockClientConnect,
    mockClientClose
} = vi.hoisted(() => ({
    mockLoadSkills: vi.fn().mockResolvedValue(undefined),
    mockServerConnect: vi.fn().mockResolvedValue(undefined),
    mockGetSkillMetadata: vi.fn().mockReturnValue(new Map()),
    mockListTools: vi.fn().mockResolvedValue({ tools: [] }),
    mockClientConnect: vi.fn().mockResolvedValue(undefined),
    mockClientClose: vi.fn().mockResolvedValue(undefined)
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
        close = mockClientClose;
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
        mockListTools.mockResolvedValue({ tools: [] });
        mockGetSkillMetadata.mockReturnValue(new Map());
    });

    describe('Nhóm 1 (P1): Đăng ký Kỹ năng & Quản lý Vòng đời MCP', () => {
        it('TC-01: should close the old local client connection when registering again to prevent leaks', async () => {
            const mockOldClient = {
                close: vi.fn().mockResolvedValue(undefined),
                connect: vi.fn(),
                listTools: vi.fn().mockResolvedValue({ tools: [] })
            };
            (registry as any).localMcpClient = mockOldClient;

            await registry.registerLocalSkills();

            expect(mockOldClient.close).toHaveBeenCalledTimes(1);
            expect((registry as any).localMcpClient).toBeDefined();
            expect((registry as any).localMcpClient).not.toBe(mockOldClient);
        });

        it('TC-02: should tolerate errors while closing old local client and continue', async () => {
            const mockOldClient = {
                close: vi.fn().mockRejectedValue(new Error('close error')),
                connect: vi.fn(),
                listTools: vi.fn().mockResolvedValue({ tools: [] })
            };
            (registry as any).localMcpClient = mockOldClient;

            await registry.registerLocalSkills();

            expect(mockOldClient.close).toHaveBeenCalledTimes(1);
            expect((registry as any).localMcpClient).toBeDefined();
            expect((registry as any).localMcpClient).not.toBe(mockOldClient);
        });

        it('TC-03: should merge local tools and external tools into mcpToolsList', async () => {
            mockListTools.mockResolvedValue({
                tools: [{ name: 'local_tool_1', description: 'Local 1', inputSchema: {} }]
            });
            mockMcpManager.getAllConnectedTools.mockResolvedValue([
                { name: 'external_tool_1', description: 'External 1', inputSchema: {}, _serverId: 'ext-server' }
            ]);

            await registry.registerLocalSkills();

            const allSkills = registry.getAllSkills();
            expect(allSkills).toContainEqual(expect.objectContaining({ name: 'local_tool_1', _serverId: 'liva-local-in-process' }));
            expect(allSkills).toContainEqual(expect.objectContaining({ name: 'external_tool_1', _serverId: 'ext-server' }));
        });

        it('TC-04: should enrich MCP tools with metadata from side-channel or fall back', async () => {
            mockListTools.mockResolvedValue({
                tools: [
                    { name: 'local_tool_1', description: 'Original Description', inputSchema: {} },
                    { name: 'local_tool_2', description: 'Original Description 2', inputSchema: {} }
                ]
            });
            const mockMetadata = new Map();
            mockMetadata.set('local_tool_1', {
                name: 'local_tool_1',
                short_desc: 'Short Description',
                search_keywords: ['key1', 'key2'],
                isCoreSkill: true,
                kit: 'DEVELOPER_KIT',
                requires_hitl: true,
                is_cpu_heavy: false
            });
            mockGetSkillMetadata.mockReturnValue(mockMetadata);

            await registry.registerLocalSkills();

            const allSkills = registry.getAllSkills();
            const skill1 = allSkills.find(s => s.name === 'local_tool_1');
            const skill2 = allSkills.find(s => s.name === 'local_tool_2');

            expect(skill1).toBeDefined();
            expect(skill1?.short_desc).toBe('Short Description');
            expect(skill1?.search_keywords).toEqual(['key1', 'key2']);
            expect(skill1?.isCoreSkill).toBe(true);
            expect(skill1?.kit).toBe('DEVELOPER_KIT');
            expect(skill1?.requires_hitl).toBe(true);

            expect(skill2).toBeDefined();
            expect(skill2?.isCoreSkill).toBe(false);
            expect(skill2?.short_desc).toBe('Original Description 2'.substring(0, 80));
        });
    });

    describe('Nhóm 2 (P2): Cơ chế Lọc Semantic Top-K & Gating', () => {
        it('TC-05: should filter skills by activeKit dynamically, keeping general and core skills', async () => {
            mockEmbedSvc.ready = true;
            registry.registerSkill({ name: 'dev_skill', description: 'dev desc', parameters: {}, kit: 'DEVELOPER_KIT' as any });
            registry.registerSkill({ name: 'social_skill', description: 'social desc', parameters: {}, kit: 'SOCIAL_KIT' as any });
            registry.registerSkill({ name: 'gen_skill', description: 'general desc', parameters: {}, kit: 'GENERAL_KIT' as any });
            registry.registerSkill({ name: 'no_kit_skill', description: 'no kit desc', parameters: {} });
            registry.registerSkill({ name: 'core_skill', description: 'core desc', parameters: {}, isCoreSkill: true });

            const results = await registry.getSemanticTopK('query', 'DEVELOPER_KIT' as any, 10);
            const names = results.map(s => s.name);

            expect(names).toContain('dev_skill');
            expect(names).toContain('gen_skill');
            expect(names).toContain('no_kit_skill');
            expect(names).toContain('core_skill');
            expect(names).not.toContain('social_skill');
        });

        it('TC-06: should fast-exit and return only core/CORE_TOOL_NAMES skills if query is empty', async () => {
            registry.registerSkill({ name: 'dev_skill', description: 'dev desc', parameters: {}, kit: 'DEVELOPER_KIT' as any });
            registry.registerSkill({ name: 'core_skill', description: 'core desc', parameters: {}, isCoreSkill: true });
            registry.registerSkill({ name: 'handoff_to_expert', description: 'handoff', parameters: {} });

            const results = await registry.getSemanticTopK('   ');
            const names = results.map(s => s.name);

            expect(names).toContain('core_skill');
            expect(names).toContain('handoff_to_expert');
            expect(names).not.toContain('dev_skill');
            expect(mockEmbedSvc.embedWithTimeout).not.toHaveBeenCalled();
        });

        it('TC-07: should fallback to keyword-only matching if EmbeddingService is not ready', async () => {
            mockEmbedSvc.ready = false;
            registry.registerSkill({ name: 'skill_keyword_1', description: 'desc', parameters: {}, search_keywords: ['banana', 'apple'] });
            registry.registerSkill({ name: 'skill_keyword_2', description: 'desc', parameters: {}, search_keywords: ['orange'] });
            registry.registerSkill({ name: 'core_skill', description: 'core desc', parameters: {}, isCoreSkill: true });

            const results = await registry.getSemanticTopK('I want a banana', undefined, 10);
            const names = results.map(s => s.name);

            expect(names).toContain('core_skill');
            expect(names).toContain('skill_keyword_1');
            expect(names).not.toContain('skill_keyword_2');
            expect(mockEmbedSvc.embedWithTimeout).not.toHaveBeenCalled();
        });

        it('TC-08: should handle embedding query failures gracefully by returning all healthy skills', async () => {
            mockEmbedSvc.ready = true;
            mockEmbedSvc.embedWithTimeout.mockRejectedValueOnce(new Error('Embedding timeout'));

            registry.registerSkill({ name: 'dynamic_skill_1', description: 'desc', parameters: {} });
            registry.registerSkill({ name: 'dynamic_skill_2', description: 'desc', parameters: {} });

            const results = await registry.getSemanticTopK('query', undefined, 10);
            const names = results.map(s => s.name);

            expect(names).toContain('dynamic_skill_1');
            expect(names).toContain('dynamic_skill_2');
        });

        it('TC-09: should filter out dynamic skills below similarity threshold', async () => {
            mockEmbedSvc.ready = true;
            const { cosineSimilarity } = await import('../../src/utils/VectorMath');
            
            registry.registerSkill({ name: 'tool_a', description: 'A', parameters: {} });
            registry.registerSkill({ name: 'tool_b', description: 'B', parameters: {} });

            (cosineSimilarity as any)
                .mockReturnValueOnce(0.8) // tool_a
                .mockReturnValueOnce(0.5); // tool_b

            const results = await registry.getSemanticTopK('query', undefined, 10);
            const names = results.map(s => s.name);

            expect(names).toContain('tool_a');
            expect(names).not.toContain('tool_b');
        });

        it('TC-10: should bypass similarity threshold for Core skills', async () => {
            mockEmbedSvc.ready = true;
            const { cosineSimilarity } = await import('../../src/utils/VectorMath');
            (cosineSimilarity as any).mockReturnValue(0.1);

            registry.registerSkill({ name: 'core_skill', description: 'core desc', parameters: {}, isCoreSkill: true });
            registry.registerSkill({ name: 'dynamic_skill', description: 'dyn desc', parameters: {} });

            const results = await registry.getSemanticTopK('query', undefined, 10);
            const names = results.map(s => s.name);

            expect(names).toContain('core_skill');
            expect(names).not.toContain('dynamic_skill');
        });

        it('TC-11: should boost skills matching search keywords below threshold, but respect hard floor', async () => {
            mockEmbedSvc.ready = true;
            const { cosineSimilarity } = await import('../../src/utils/VectorMath');
            
            registry.registerSkill({ name: 'tool_c', description: 'C', parameters: {}, search_keywords: ['banana'] });
            registry.registerSkill({ name: 'tool_d', description: 'D', parameters: {}, search_keywords: ['apple'] });
            registry.registerSkill({ name: 'tool_e', description: 'E', parameters: {}, search_keywords: ['orange'] });

            (cosineSimilarity as any)
                .mockReturnValueOnce(0.40)  // tool_c (below threshold, above hard floor, matches kw)
                .mockReturnValueOnce(0.25)  // tool_d (below hard floor, no kw match)
                .mockReturnValueOnce(0.15); // tool_e (below hard floor, matches kw)

            const results = await registry.getSemanticTopK('banana orange', undefined, 10);
            const names = results.map(s => s.name);

            expect(names).toContain('tool_c');
            expect(names).not.toContain('tool_d');
            expect(names).toContain('tool_e');
        });

        it('TC-12: should not duplicate tools that match both semantic threshold and keyword boost', async () => {
            mockEmbedSvc.ready = true;
            const { cosineSimilarity } = await import('../../src/utils/VectorMath');
            (cosineSimilarity as any).mockReturnValue(0.9);

            registry.registerSkill({ name: 'tool_f', description: 'F', parameters: {}, search_keywords: ['banana'] });

            const results = await registry.getSemanticTopK('banana query', undefined, 10);
            const occurrences = results.filter(s => s.name === 'tool_f').length;

            expect(occurrences).toBe(1);
        });

        it('TC-13: should tolerate description embedding failure and proceed with other tools', async () => {
            mockEmbedSvc.ready = true;
            
            registry.registerSkill({ name: 'tool_g', description: 'G', parameters: {} });
            registry.registerSkill({ name: 'tool_h', description: 'H', parameters: {} });

            mockEmbedSvc.embed
                .mockRejectedValueOnce(new Error('VRAMGuard Error'))
                .mockResolvedValueOnce([0.5, 0.6]);

            const { cosineSimilarity } = await import('../../src/utils/VectorMath');
            (cosineSimilarity as any).mockReturnValue(0.8);

            const results = await registry.getSemanticTopK('query', undefined, 10);
            const names = results.map(s => s.name);

            expect(names).not.toContain('tool_g');
            expect(names).toContain('tool_h');
        });
    });

    describe('Nhóm 3 (P3): Bộ lọc Sức khỏe & Ngăn chặn Lỗi', () => {
        it('TC-14: should prune skills with open circuit breaker from healthy skills list', async () => {
            registry.registerSkill({ name: 'tool_healthy', description: 'healthy', parameters: {} });
            registry.registerSkill({ name: 'tool_unhealthy', description: 'unhealthy', parameters: {} });

            for (let i = 0; i < 5; i++) {
                registry.circuitBreaker.recordFailure('tool_unhealthy', 'network error');
            }

            expect(registry.circuitBreaker.canExecute('tool_unhealthy')).toBe(false);

            const healthy = registry.getHealthySkills();
            const names = healthy.map(s => s.name);

            expect(names).toContain('tool_healthy');
            expect(names).not.toContain('tool_unhealthy');
        });

        it('TC-15: should prune whitelist-disabled skills from healthy skills list', async () => {
            registry.registerSkill({ name: 'tool_healthy', description: 'healthy', parameters: {} });
            registry.registerSkill({ name: 'tool_disabled', description: 'disabled', parameters: {} });

            vi.spyOn(registry.whitelist, 'getDisabledSkills').mockReturnValue(new Set(['tool_disabled']));

            const healthy = registry.getHealthySkills();
            const names = healthy.map(s => s.name);

            expect(names).toContain('tool_healthy');
            expect(names).not.toContain('tool_disabled');
        });

        it('TC-16: should combine both circuit breaker and whitelist filters', async () => {
            registry.registerSkill({ name: 'tool_healthy', description: 'healthy', parameters: {} });
            registry.registerSkill({ name: 'tool_disabled', description: 'disabled', parameters: {} });
            registry.registerSkill({ name: 'tool_unhealthy', description: 'unhealthy', parameters: {} });

            vi.spyOn(registry.whitelist, 'getDisabledSkills').mockReturnValue(new Set(['tool_disabled']));
            vi.spyOn(registry.circuitBreaker, 'getOpenCircuits').mockReturnValue(['tool_unhealthy']);

            const healthy = registry.getHealthySkills();
            const names = healthy.map(s => s.name);

            expect(names).toContain('tool_healthy');
            expect(names).not.toContain('tool_disabled');
            expect(names).not.toContain('tool_unhealthy');
        });
    });

    describe('Nhóm 4 (P4): Thực thi Kỹ năng & Phản ứng Lỗi', () => {
        it('TC-17: should reject execution immediately with a friendly error if circuit is open', async () => {
            registry.registerSkill({ name: 'tool_unhealthy', description: 'unhealthy', parameters: {} });
            vi.spyOn(registry.circuitBreaker, 'canExecute').mockReturnValue(false);

            await expect(registry.executeSkill('tool_unhealthy', {})).rejects.toThrow('Hệ thống mạng đang lỗi');
        });

        it('TC-18: should correctly route execution for Fallback, Local MCP, and External MCP tools', async () => {
            // 1. Fallback Skill
            const mockFallbackExec = vi.fn().mockResolvedValue('Fallback Result');
            registry.registerSkill({
                name: 'fallback_tool',
                description: 'fallback',
                parameters: {},
                execute: mockFallbackExec
            });

            const r1 = await registry.executeSkill('fallback_tool', { a: 1 });
            expect(mockFallbackExec).toHaveBeenCalledWith({ a: 1 });
            expect(r1).toBe('Fallback Result');

            // 2. Local MCP Tool
            await registry.registerLocalSkills();
            injectMcpTools(registry, [
                { name: 'local_tool', description: 'local', inputSchema: {}, _serverId: 'liva-local-in-process' }
            ]);
            const mockCallTool = vi.fn().mockResolvedValue({
                isError: false,
                content: [{ type: 'text', text: 'Local Result' }]
            });
            (registry as any).localMcpClient.callTool = mockCallTool;

            const r2 = await registry.executeSkill('local_tool', { b: 2 });
            expect(mockCallTool).toHaveBeenCalledWith({ name: 'local_tool', arguments: { b: 2 } });
            expect(r2).toBe('Local Result');

            // 3. External MCP Tool
            injectMcpTools(registry, [
                { name: 'external_tool', description: 'external', inputSchema: {}, _serverId: 'ext-server' }
            ]);
            mockMcpManager.executeTool.mockResolvedValue({
                isError: false,
                content: [{ type: 'text', text: 'External Result' }]
            });

            const r3 = await registry.executeSkill('external_tool', { c: 3 });
            expect(mockMcpManager.executeTool).toHaveBeenCalledWith('ext-server', 'external_tool', { c: 3 });
            expect(r3).toBe('External Result');
        });

        it('TC-19: should record success in circuit breaker on successful execution', async () => {
            const mockFallbackExec = vi.fn().mockResolvedValue('OK');
            registry.registerSkill({
                name: 'success_tool',
                description: 'desc',
                parameters: {},
                execute: mockFallbackExec
            });

            const recordSuccessSpy = vi.spyOn(registry.circuitBreaker, 'recordSuccess');

            await registry.executeSkill('success_tool', {});
            expect(recordSuccessSpy).toHaveBeenCalledWith('success_tool');
        });

        it('TC-20: should record failure and wrap errors, except for non-existent tool errors', async () => {
            const mockFallbackExec = vi.fn().mockRejectedValue(new Error('Database offline'));
            registry.registerSkill({
                name: 'fail_tool',
                description: 'desc',
                parameters: {},
                execute: mockFallbackExec
            });

            const recordFailureSpy = vi.spyOn(registry.circuitBreaker, 'recordFailure');

            await expect(registry.executeSkill('fail_tool', {})).rejects.toThrow("MCP Tool 'fail_tool' execution failed: Database offline");
            expect(recordFailureSpy).toHaveBeenCalledWith('fail_tool', 'Database offline');

            await expect(registry.executeSkill('non_existent_tool', {})).rejects.toThrow("không tồn tại");
            expect(recordFailureSpy).toHaveBeenCalledWith('non_existent_tool', expect.stringContaining('không tồn tại'));
        });
    });

    describe('Nhóm 5 (P5): Tự động Nạp Nóng Bộ nhớ đệm', () => {
        it('TC-21: should skip cache warming gracefully if EmbeddingService fails to ensure ready', async () => {
            mockEmbedSvc.ready = true;
            mockEmbedSvc.ensureReady.mockRejectedValueOnce(new Error('Model loading failed'));

            registry.registerSkill({ name: 'dyn_1', description: 'dyn desc 1', parameters: {} });

            await expect(registry.warmUpCache()).resolves.toBeUndefined();
        });

        it('TC-22: should batch embed and warm up cache for multiple dynamic skills', async () => {
            mockEmbedSvc.ready = true;
            mockEmbedSvc.ensureReady.mockResolvedValue(undefined);

            registry.registerSkill({ name: 'dyn_1', description: 'desc 1', parameters: {} });
            registry.registerSkill({ name: 'dyn_2', description: 'desc 2', parameters: {} });

            mockEmbedSvc.embedBatch.mockResolvedValueOnce([
                [0.1, 0.2],
                [0.3, 0.4]
            ]);

            await registry.warmUpCache();

            expect(mockEmbedSvc.embedBatch).toHaveBeenCalledWith([
                'dyn_1  desc 1',
                'dyn_2  desc 2'
            ]);
            expect((registry as any).descEmbeddingCache.get('dyn_1')).toEqual([0.1, 0.2]);
            expect((registry as any).descEmbeddingCache.get('dyn_2')).toEqual([0.3, 0.4]);
        });

        it('TC-23: should skip caching for core skills or already cached skills', async () => {
            mockEmbedSvc.ready = true;
            mockEmbedSvc.ensureReady.mockResolvedValue(undefined);

            registry.registerSkill({ name: 'core_skill', description: 'core desc', parameters: {}, isCoreSkill: true });
            registry.registerSkill({ name: 'cached_skill', description: 'cached desc', parameters: {} });

            (registry as any).descEmbeddingCache.set('cached_skill', [0.9, 0.9]);
            registry.registerSkill({ name: 'uncached_skill', description: 'uncached desc', parameters: {} });

            mockEmbedSvc.embedBatch.mockResolvedValueOnce([
                [0.5, 0.5]
            ]);

            await registry.warmUpCache();

            expect(mockEmbedSvc.embedBatch).toHaveBeenCalledWith([
                'uncached_skill  uncached desc'
            ]);
        });

        it('TC-24: should handle batch embedding failures gracefully and not crash', async () => {
            mockEmbedSvc.ready = true;
            mockEmbedSvc.ensureReady.mockResolvedValue(undefined);

            registry.registerSkill({ name: 'dyn_1', description: 'desc 1', parameters: {} });
            mockEmbedSvc.embedBatch.mockRejectedValueOnce(new Error('Network error'));

            await expect(registry.warmUpCache()).resolves.toBeUndefined();
        });
    });

    describe('Nhóm 6 (P6): An toàn Bất đồng bộ, Bảo mật Siêu dữ liệu & Hiệu năng', () => {
        it('TC-25: should evict stale cache entries when re-registering tools', async () => {
            mockEmbedSvc.ready = true;
            mockEmbedSvc.ensureReady.mockResolvedValue(undefined);

            registry.registerSkill({ name: 'tool_a', description: 'desc A', parameters: {} });
            mockEmbedSvc.embedBatch.mockResolvedValueOnce([[0.1, 0.2]]);
            await registry.warmUpCache();
            expect((registry as any).descEmbeddingCache.has('tool_a')).toBe(true);

            mockListTools.mockResolvedValue({ tools: [] });
            await registry.registerLocalSkills();

            expect((registry as any).descEmbeddingCache.has('tool_a')).toBe(false);
        });

        it('TC-26: should prevent race conditions or deadlock when query runs during warm-up', async () => {
            mockEmbedSvc.ready = true;
            mockEmbedSvc.ensureReady.mockResolvedValue(undefined);

            registry.registerSkill({ name: 'tool_a', description: 'desc A', parameters: {} });

            mockEmbedSvc.embedBatch.mockImplementation(() => {
                return new Promise(resolve => setTimeout(() => resolve([[0.1, 0.2]]), 20));
            });

            const warmUpPromise = registry.warmUpCache();
            const queryPromise = registry.getSemanticTopK('query', undefined, 10);

            await expect(Promise.all([warmUpPromise, queryPromise])).resolves.toBeDefined();
        });

        it('TC-27: should sanitize and truncate malicious long description metadata to 1000 characters', async () => {
            mockEmbedSvc.ready = true;
            
            const longDesc = 'a'.repeat(50000);
            registry.registerSkill({ 
                name: 'malicious_tool', 
                description: 'normal desc', 
                parameters: {},
                short_desc: longDesc 
            });

            mockEmbedSvc.embed.mockResolvedValue([0.1, 0.2]);

            const { cosineSimilarity } = await import('../../src/utils/VectorMath');
            (cosineSimilarity as any).mockReturnValue(0.9);

            await registry.getSemanticTopK('query', undefined, 10);

            const maliciousCall = mockEmbedSvc.embed.mock.calls.find((call: any) => call[0].startsWith('malicious_tool'));
            expect(maliciousCall).toBeDefined();
            expect(maliciousCall![0].length).toBeLessThanOrEqual(1000);
        });

        it('TC-28: should execute Cosine Similarity check for 1000 skills below 15ms', async () => {
            mockEmbedSvc.ready = true;
            
            const dummyVector = Array.from({ length: 384 }, () => Math.random());
            for (let i = 0; i < 1000; i++) {
                const name = `dummy_tool_${i}`;
                registry.registerSkill({ name, description: 'dummy', parameters: {} });
                (registry as any).descEmbeddingCache.set(name, dummyVector);
            }

            const { cosineSimilarity } = await import('../../src/utils/VectorMath');
            (cosineSimilarity as any).mockImplementation((a: number[], b: number[]) => 0.5);

            const start = performance.now();
            await registry.getSemanticTopK('query', undefined, 10);
            const duration = performance.now() - start;
            
            expect(duration).toBeLessThan(15);
        });
    });

    describe('BuiltIn Skills', () => {
        it('get_current_time should execute correctly', async () => {
            const result = await registry.executeSkill('get_current_time', { timezone: 'UTC' });
            expect(result).toBeDefined();
            expect(typeof result).toBe('string');
        });
    });
});
