import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest';

// 1. MOCK TRƯỚC KHI IMPORT (Chặn chạy logic thật, VRAM, GPU, Database...)
vi.mock('../src/core/CoreKernel', () => ({
    CoreKernel: vi.fn().mockImplementation(() => ({
        fetchSystemLocation: vi.fn().mockResolvedValue(true),
        bootstrap: vi.fn().mockResolvedValue(true),
        shutdown: vi.fn().mockResolvedValue(true)
    }))
}));

// Giả sử có AutoGPUSetup
vi.mock('../src/scripts/AutoGPUSetup', () => ({
    AutoGPUSetup: { runAutoSetupIfNeeded: vi.fn().mockResolvedValue(true) }
}));

describe('Gateway Entry Point', () => {
    let exitSpy: ReturnType<typeof vi.spyOn>;
    let errorSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
        vi.clearAllMocks();
        // 2. CRITICAL: Chặn app tắt làm sập Vitest
        exitSpy = vi.spyOn(process, 'exit').mockImplementation((() => {}) as any);
        errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {}); 
        vi.spyOn(console, 'warn').mockImplementation(() => {}); 
    });

    afterEach(() => { 
        vi.restoreAllMocks(); 
    });

    it('should bootstrap successfully without exiting process', async () => {
        // 3. Import động để test chạy ngay lúc đó thay vì load sẵn
        const Gateway = await import('../src/Gateway'); 
        
        // 4. Đảm bảo Kernel được gọi và app không bị crash
        const { CoreKernel } = await import('../src/core/CoreKernel');
        
        // Wait a bit for the async start() to finish since it's not exported
        await new Promise(resolve => setTimeout(resolve, 50));
        
        expect(CoreKernel).toHaveBeenCalled();
        expect(exitSpy).not.toHaveBeenCalled();
    });

    it('should handle EOF event from stdin and gracefully shutdown', async () => {
        await import('../src/Gateway');
        const { CoreKernel } = await import('../src/core/CoreKernel');
        
        // Simulate stdin 'end'
        process.stdin.emit('end');

        // Check if shutdown was called on the instance
        const mockKernelInstance = vi.mocked(CoreKernel).mock.results[0]?.value;
        if (mockKernelInstance) {
            expect(mockKernelInstance.shutdown).toHaveBeenCalled();
        }
        expect(exitSpy).toHaveBeenCalledWith(0);
    });
});
