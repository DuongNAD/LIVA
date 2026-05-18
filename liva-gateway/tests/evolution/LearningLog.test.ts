/**
 * LearningLog.test.ts — Evolution Learning Memory
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { LearningLog } from '../../src/evolution/LearningLog';
import { EmbeddingService } from '../../src/services/EmbeddingService';
import { logger } from '../../src/utils/logger';

vi.mock('../../src/services/EmbeddingService');
vi.mock('../../src/utils/logger', () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn(),
        debug: vi.fn()
    }
}));

const mockStructuredMemory = vi.hoisted(() => ({
    vecReady: true,
    upsertVector: vi.fn(),
    searchSimilarVectors: vi.fn().mockReturnValue([])
}));

vi.mock('../../src/memory/StructuredMemory', () => ({
    StructuredMemory: {
        create: vi.fn().mockResolvedValue(mockStructuredMemory)
    }
}));

describe('LearningLog — Evolution Memory (v19)', () => {
    let learningLog: LearningLog;
    let mockEmbeddingService: any;

    beforeEach(() => {
        vi.clearAllMocks();

        mockEmbeddingService = {
            dimension: 384,
            embed: vi.fn().mockResolvedValue(new Array(384).fill(0.1))
        };
        (EmbeddingService.getInstance as any).mockReturnValue(mockEmbeddingService);

        learningLog = new LearningLog(mockEmbeddingService);
    });

    it('connect() should initialize StructuredMemory', async () => {
        await learningLog.connect();
        expect(learningLog['structuredMemory']).toBeDefined();
    });

    it('recordAttempt() should embed and upsert vector to StructuredMemory', async () => {
        await learningLog.recordAttempt('test.ts', 'read', 'context', true);
        expect(mockEmbeddingService.embed).toHaveBeenCalled();
        expect(mockStructuredMemory.upsertVector).toHaveBeenCalledWith(expect.objectContaining({
            type: 'SUCCESS',
            domain: 'test.ts'
        }));
    });

    it('getRelevantAxioms() should return formatted system memory block', async () => {
        mockStructuredMemory.searchSimilarVectors.mockReturnValueOnce([
            { type: 'SUCCESS', content: 'Success detail' },
            { type: 'DEAD-END', content: 'Failure detail' }
        ]);
        const result = await learningLog.getRelevantAxioms('test.ts', 'action');
        expect(result).toContain('<success>Success detail</success>');
        expect(result).toContain('<failure>Failure detail</failure>');
    });

    it('getRelevantAxioms() should return empty message when no results found', async () => {
        mockStructuredMemory.searchSimilarVectors.mockReturnValueOnce([]);
        const result = await learningLog.getRelevantAxioms('test.ts', 'action');
        expect(result).toContain('[No relevant evolution experiences found]');
    });

    it('should distill context correctly', () => {
        const distilled = (learningLog as any).distillContext('C:\\app\\src\\main.ts has error in /home/user/src/lib.ts');
        expect(distilled).toBe('src/main.ts has error in src/lib.ts');
        
        const longContext = 'a'.repeat(3000);
        const distilledLong = (learningLog as any).distillContext(longContext);
        expect(distilledLong.length).toBeLessThan(2020);
    });

    it('should handle empty/null context in distillContext', () => {
        const distilled = (learningLog as any).distillContext('');
        expect(distilled).toBe('Unknown error');
    });
});
