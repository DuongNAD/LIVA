// tests/setup.ts
import { afterEach, vi, beforeAll, afterAll } from 'vitest';

let exitSpy: any;

beforeAll(() => {
    // Prevent ANY test from calling process.exit and crashing the test runner silently
    exitSpy = vi.spyOn(process, 'exit').mockImplementation((code?: number | string | null | undefined) => {
        console.error(`[VITEST WARNING] Intercepted process.exit(${code})`);
        throw new Error(`process.exit(${code}) was called in a test environment!`);
    });
});

afterAll(() => {
    if (exitSpy) {
        exitSpy.mockRestore();
    }
});

afterEach(() => {
    vi.restoreAllMocks();      // Xóa toàn bộ spy lịch sử để giải phóng RAM
    vi.clearAllTimers();       // Dọn sạch setTimeout() bị kẹt lại của Phase 3
});
