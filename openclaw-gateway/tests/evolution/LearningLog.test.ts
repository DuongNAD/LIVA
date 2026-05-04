import { describe, it, expect, vi, beforeEach } from 'vitest';
import { LearningLog } from '../../src/evolution/LearningLog';
import { EmbeddingService } from '../../src/services/EmbeddingService';
import * as lancedb from '@lancedb/lancedb';
import { logger } from '../../src/utils/logger';

vi.mock('@lancedb/lancedb');
vi.mock('../../src/services/EmbeddingService');
vi.mock('../../src/utils/logger', () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn(),
        debug: vi.fn()
    }
}));

describe('LearningLog', () => {
    let learningLog: LearningLog;
    let mockEmbeddingService: any;
    let mockDb: any;
    let mockTable: any;

    beforeEach(() => {
        vi.clearAllMocks();

        mockEmbeddingService = {
            dimension: 384,
            embed: vi.fn().mockResolvedValue(new Array(384).fill(0.1))
        };
        (EmbeddingService.getInstance as any).mockReturnValue(mockEmbeddingService);

        mockTable = {
            search: vi.fn().mockReturnThis(),
            limit: vi.fn().mockReturnThis(),
            toArray: vi.fn().mockResolvedValue([]),
            add: vi.fn().mockResolvedValue(undefined),
            delete: vi.fn().mockResolvedValue(undefined)
        };

        mockDb = {
            tableNames: vi.fn().mockResolvedValue([]),
            createTable: vi.fn().mockResolvedValue(undefined),
            openTable: vi.fn().mockResolvedValue(mockTable)
        };

        (lancedb.connect as any).mockResolvedValue(mockDb);

        learningLog = new LearningLog(mockEmbeddingService);
    });

    it('should connect and create table if not exists', async () => {
        await learningLog.connect();
        
        expect(lancedb.connect).toHaveBeenCalled();
        expect(mockDb.createTable).toHaveBeenCalled();
        expect(logger.info).toHaveBeenCalledWith(expect.stringContaining('Đã khởi tạo'));
    });

    it('should catch error on connect failure', async () => {
        (lancedb.connect as any).mockRejectedValueOnce(new Error('Connection Failed'));
        
        await learningLog.connect();
        expect(logger.error).toHaveBeenCalledWith(expect.stringContaining('Kết nối LanceDB thất bại'));
    });

    it('should record attempt - new log entry', async () => {
        mockDb.tableNames.mockResolvedValue(['evolution_logs_v2']);
        
        await learningLog.recordAttempt('test.ts', 'read', 'C:\\user\\src\\test.ts', true);
        
        expect(mockEmbeddingService.embed).toHaveBeenCalledWith('File: test.ts. Action: read. Context: src/test.ts');
        expect(mockTable.search).toHaveBeenCalled();
        expect(mockTable.add).toHaveBeenCalled();
    });

    it('should record attempt - update existing log entry', async () => {
        mockDb.tableNames.mockResolvedValue(['evolution_logs_v2']);
        mockTable.toArray.mockResolvedValue([{
            id: 'existing_id',
            _distance: 0.01,
            occurrence_count: 1
        }]);

        await learningLog.recordAttempt('test.ts', 'read', 'test context', false);

        expect(mockTable.delete).toHaveBeenCalledWith("id = 'existing_id'");
        expect(mockTable.add).toHaveBeenCalled();
        
        // Ensure that occurrence_count is incremented
        const addArg = mockTable.add.mock.calls[0][0][0];
        expect(addArg.occurrence_count).toBe(2);
        expect(addArg.success).toBe(false);
    });

    it('should distill context correctly', async () => {
        const distilled = (learningLog as any).distillContext('C:\\app\\src\\main.ts has error in /home/user/src/lib.ts');
        expect(distilled).toBe('src/main.ts has error in src/lib.ts');
        
        const longContext = 'a'.repeat(3000);
        const distilledLong = (learningLog as any).distillContext(longContext);
        expect(distilledLong.length).toBeLessThan(2020); // 2000 + length of "... (distilled)"
    });

    it('should get relevant axioms - return empty if table not exists', async () => {
        mockDb.tableNames.mockResolvedValue([]);
        const result = await learningLog.getRelevantAxioms('test.ts', 'action');
        expect(result).toContain('Không có ký ức');
    });

    it('should get relevant axioms - filter old memories and separate best practices and anti patterns', async () => {
        mockDb.tableNames.mockResolvedValue(['evolution_logs_v2']);
        
        mockTable.toArray.mockResolvedValue([
            { id: 'init_id' },
            { id: 'old', timestamp: Date.now() - (40 * 24 * 60 * 60 * 1000) }, // 40 days old
            { id: '1', success: true, occurrence_count: 2, action: 'act1', targetFile: 'f1', asiContext: 'ctx1', timestamp: Date.now() },
            { id: '2', success: false, occurrence_count: 6, action: 'act2', targetFile: 'f2', asiContext: 'ctx2', timestamp: Date.now() }
        ]);

        const result = await learningLog.getRelevantAxioms('test.ts', 'action', 5);
        
        expect(result).toContain('<best_practices>');
        expect(result).toContain('act1');
        expect(result).toContain('2_times');
        
        expect(result).toContain('<anti_patterns>');
        expect(result).toContain('act2');
        expect(result).toContain('CRITICAL');
    });

    it('should get relevant axioms - handle errors', async () => {
        (lancedb.connect as any).mockRejectedValueOnce(new Error('Lance Error'));
        const result = await learningLog.getRelevantAxioms('test.ts', 'action');
        expect(result).toContain('Lỗi truy xuất ký ức');
    });
});
