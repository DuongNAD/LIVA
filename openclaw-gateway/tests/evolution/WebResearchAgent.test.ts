import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { researchGoal, researchErrors, fullResearch } from '../../src/evolution/WebResearchAgent';
import * as WebSearch from '@skills/web/WebSearch.js';
import { logger } from '../../src/utils/logger';

vi.mock('@skills/web/WebSearch.js', () => ({
    execute: vi.fn()
}));

vi.mock('../../src/utils/logger', () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn()
    }
}));

describe('WebResearchAgent', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    describe('researchGoal', () => {
        it('should build a technical query and return distilled results', async () => {
            (WebSearch.execute as any).mockResolvedValue('1. Do this\n2. Do that\n3. And this\n4. And this too');

            const promise = researchGoal('Fix typescript error in React');
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(WebSearch.execute).toHaveBeenCalledWith({ query: 'Fix typescript error in React' });
            expect(result).toBe('1. Do this\n2. Do that\n3. And this'); // Only up to 3 items
        });

        it('should handle short non-English goals', async () => {
            (WebSearch.execute as any).mockResolvedValue('1. Sửa lỗi');

            const promise = researchGoal('Lỗi JS');
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(WebSearch.execute).toHaveBeenCalledWith({ query: 'TypeScript Lỗi JS' });
            expect(result).toBe('1. Sửa lỗi');
        });

        it('should return empty string if result contains "Không tìm thấy"', async () => {
            (WebSearch.execute as any).mockResolvedValue('Không tìm thấy kết quả nào');

            const promise = researchGoal('Test Goal');
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(result).toBe('');
        });

        it('should handle timeouts', async () => {
            (WebSearch.execute as any).mockImplementation(() => new Promise(resolve => setTimeout(resolve, 20000)));

            const promise = researchGoal('Test Goal');
            
            // Fast forward time to trigger timeout (10_000 ms)
            await vi.advanceTimersByTimeAsync(11000);
            const result = await promise;

            expect(result).toBe('');
            expect(logger.warn).toHaveBeenCalledWith(expect.stringContaining('Goal research failed (non-fatal)'));
        });

        it('should handle search exceptions', async () => {
            (WebSearch.execute as any).mockRejectedValue(new Error('Search failed'));

            const promise = researchGoal('Test Goal');
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(result).toBe('');
            expect(logger.warn).toHaveBeenCalledWith(expect.stringContaining('Goal research failed (non-fatal)'));
        });
    });

    describe('researchErrors', () => {
        it('should extract TS errors and search them', async () => {
            const asiReport = `
                src/index.ts:10:5 - error TS2304: Cannot find name 'foo' in this scope.
                src/util.ts:20:1 - error TS1005: ',' expected at the end of the line.
            `;
            
            (WebSearch.execute as any).mockResolvedValue('1. Fix TS error');

            const promise = researchErrors(asiReport);
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(WebSearch.execute).toHaveBeenCalledTimes(2);
            expect(WebSearch.execute).toHaveBeenCalledWith({ query: expect.stringContaining('TS2304') });
            expect(WebSearch.execute).toHaveBeenCalledWith({ query: expect.stringContaining('TS1005') });
            
            expect(result).toContain('<error_research>');
            expect(result).toContain('1. Fix TS error');
        });

        it('should fallback to generic errors if no TS errors found', async () => {
            const asiReport = `
                Compilation Error: Unexpected token found in the file, please check.
            `;
            
            (WebSearch.execute as any).mockResolvedValue('1. Fix token');

            const promise = researchErrors(asiReport);
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(WebSearch.execute).toHaveBeenCalledWith({ query: expect.stringContaining('Unexpected token') });
            expect(result).toContain('1. Fix token');
        });

        it('should return empty string if no errors extracted', async () => {
            const promise = researchErrors('Everything is fine');
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(result).toBe('');
            expect(WebSearch.execute).not.toHaveBeenCalled();
        });

        it('should handle partial search failures', async () => {
            const asiReport = `
                error TS2304: Cannot find name 'foo' in this scope.
                error TS1005: ',' expected at the end of the line.
            `;
            
            // First succeeds, second fails
            (WebSearch.execute as any).mockResolvedValueOnce('1. Fix TS2304')
                                      .mockRejectedValueOnce(new Error('Network error'));

            const promise = researchErrors(asiReport);
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(result).toContain('1. Fix TS2304');
        });

        it('should handle timeouts in error research', async () => {
            const asiReport = `error TS2304: Cannot find name 'foo' in this scope.`;
            
            (WebSearch.execute as any).mockImplementation(() => new Promise(resolve => setTimeout(resolve, 20000)));

            const promise = researchErrors(asiReport);
            await vi.advanceTimersByTimeAsync(11000);
            const result = await promise;

            expect(result).toBe('');
        });

        it('should handle fatal errors gracefully', async () => {
            const promise = researchErrors(null as unknown as string);
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(result).toBe('');
            expect(logger.warn).toHaveBeenCalledWith(expect.stringContaining('Error research failed (non-fatal)'));
        });
    });

    describe('fullResearch', () => {
        beforeEach(() => {
            vi.resetAllMocks();
        });

        it('should perform full research with both goal and errors', async () => {
            // Mock researchGoal (first call to WebSearch)
            (WebSearch.execute as any).mockResolvedValueOnce('1. Goal insights')
                                      // Mock researchErrors (second call to WebSearch)
                                      .mockResolvedValueOnce('1. Error insights');

            const promise = fullResearch('Do something', 'error TS2304: Cannot find name "foo" in this file.');
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(result.goalInsights).toContain('1. Goal insights');
            expect(result.errorFixes).toContain('1. Error insights');
            expect(result.totalQueries).toBe(2);
            expect(result.totalResults).toBe(2);
        });

        it('should skip error research if no previous errors', async () => {
            (WebSearch.execute as any).mockResolvedValueOnce('1. Goal insights');

            const promise = fullResearch('Do something');
            await vi.runAllTimersAsync();
            const result = await promise;

            expect(result.goalInsights).toContain('1. Goal insights');
            expect(result.errorFixes).toBe('');
            expect(result.totalQueries).toBe(1);
            expect(result.totalResults).toBe(1);
        });
    });
});
