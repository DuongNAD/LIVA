import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as UpdateMemory from '../../src/skills/core/UpdateMemory';
import { StructuredMemory } from '../../src/memory/StructuredMemory';

vi.mock('../../src/memory/StructuredMemory');

describe('UpdateMemory Skill', () => {
    let memory: StructuredMemory;

    beforeEach(() => {
        vi.clearAllMocks();
        memory = new StructuredMemory("test_core.sqlite");
        UpdateMemory.setMemoryInstance(memory);
    });

    it('should store facts with correct category and return success message', async () => {
        const result = await UpdateMemory.execute({
            key: 'sinh_nhat_vo',
            value: '20/5, likes roses',
            category: 'Events'
        });

        expect(memory.setFact).toHaveBeenCalledWith(
            'sinh_nhat_vo',
            '20/5, likes roses',
            expect.objectContaining({
                category: 'Events',
                source: 'ai_tool',
                ttlDays: 30 // Sự kiện has 30 days TTL
            })
        );

        expect(result).toContain('Memory saved');
        expect(result).toContain('sinh_nhat_vo');
    });

    it('should fallback to Chung category if invalid', async () => {
        const result = await UpdateMemory.execute({
            key: 'ban_phim',
            value: 'Cơ',
            category: 'InvalidCategory'
        });

        expect(memory.setFact).toHaveBeenCalledWith(
            'ban_phim',
            'Cơ',
            expect.objectContaining({ category: 'General' })
        );
    });

    it('should apply 7 days TTL for Cảm xúc', async () => {
        await UpdateMemory.execute({
            key: 'tam_trang_hien_tai',
            value: 'Đang stress',
            category: 'Emotions'
        });

        expect(memory.setFact).toHaveBeenCalledWith(
            'tam_trang_hien_tai',
            'Đang stress',
            expect.objectContaining({ ttlDays: 7 })
        );
    });
});
