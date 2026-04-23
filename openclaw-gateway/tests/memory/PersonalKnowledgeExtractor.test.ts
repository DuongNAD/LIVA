import { describe, it, expect, vi, beforeEach } from 'vitest';
import { PersonalKnowledgeExtractor } from '../../src/memory/PersonalKnowledgeExtractor';
import { StructuredMemory } from '../../src/memory/StructuredMemory';
import OpenAI from 'openai';

vi.mock('../../src/memory/StructuredMemory');
vi.mock('openai');

describe('PersonalKnowledgeExtractor', () => {
    let memory: StructuredMemory;
    let aiClient: OpenAI;
    let extractor: PersonalKnowledgeExtractor;

    beforeEach(() => {
        vi.clearAllMocks();
        memory = new StructuredMemory("test_core");
        
        // Mock OpenAI Client
        aiClient = new OpenAI({ apiKey: 'test' });
        aiClient.chat = {
            completions: {
                create: vi.fn(),
            }
        } as any;

        extractor = new PersonalKnowledgeExtractor(memory, aiClient);
    });

    it('should ignore short or trivial messages', async () => {
        await extractor.extractAndStore('hello', 'Hi there!');
        expect(aiClient.chat.completions.create).not.toHaveBeenCalled();
        
        await extractor.extractAndStore('ok dạ', 'Tôi hiểu rồi.');
        expect(aiClient.chat.completions.create).not.toHaveBeenCalled();
    });

    it('should extract and store personal facts correctly', async () => {
        // Mock AI returning a valid JSON extraction
        (aiClient.chat.completions.create as any).mockResolvedValue({
            choices: [{
                message: {
                    content: `[
                        {"key": "so_thich", "value": "Thích cà phê đen lạnh", "category": "Sở thích"},
                        {"key": "nguoi_than", "value": "Có em gái tên Linh", "category": "Người thân"}
                    ]`
                }
            }]
        });

        await extractor.extractAndStore(
            'Mua cho tôi ly cà phê đen lạnh nhé, lát tôi phải đi đón em gái tôi là Linh.', 
            'Được rồi, tôi đã ghi chú lại.'
        );

        expect(aiClient.chat.completions.create).toHaveBeenCalledTimes(1);
        
        // Should have called setFact twice
        expect(memory.setFact).toHaveBeenCalledTimes(2);
        
        expect(memory.setFact).toHaveBeenNthCalledWith(1, 'so_thich', 'Thích cà phê đen lạnh', expect.objectContaining({ category: 'Sở thích' }));
        expect(memory.setFact).toHaveBeenNthCalledWith(2, 'nguoi_than', 'Có em gái tên Linh', expect.objectContaining({ category: 'Người thân' }));
        
        expect(extractor.totalExtracted).toBe(2);
    });

    it('should handle AI returning empty array', async () => {
        (aiClient.chat.completions.create as any).mockResolvedValue({
            choices: [{ message: { content: "[]" } }]
        });

        await extractor.extractAndStore(
            'Hôm nay trời mưa to quá, tắc đường suốt.', 
            'Trời mưa bạn đi đường cẩn thận nhé.'
        );

        expect(aiClient.chat.completions.create).toHaveBeenCalledTimes(1);
        expect(memory.setFact).not.toHaveBeenCalled();
    });

    it('should handle malformed JSON safely without crashing', async () => {
        (aiClient.chat.completions.create as any).mockResolvedValue({
            choices: [{ message: { content: "Lỗi kết nối AI" } }]
        });

        // Execution should not throw
        await expect(extractor.extractAndStore('Mai tôi đi làm', 'Ok')).resolves.not.toThrow();
        expect(memory.setFact).not.toHaveBeenCalled();
    });
});
