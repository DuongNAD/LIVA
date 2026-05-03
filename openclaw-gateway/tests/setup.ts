// tests/setup.ts
import { afterEach, vi } from 'vitest';

afterEach(() => {
    vi.restoreAllMocks();      // Xóa toàn bộ spy lịch sử để giải phóng RAM
    vi.clearAllTimers();       // Dọn sạch setTimeout() bị kẹt lại của Phase 3
});
