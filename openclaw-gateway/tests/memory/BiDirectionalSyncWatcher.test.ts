import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { BiDirectionalSyncWatcher } from "../../src/memory/BiDirectionalSyncWatcher";
import { promises as fsp } from "node:fs";

vi.mock("node:fs", () => ({
    promises: {
        watch: vi.fn(),
        readFile: vi.fn()
    }
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn()
    }
}));

import { EventEmitter } from "node:events";

describe("BiDirectionalSyncWatcher", () => {
    let watcher: BiDirectionalSyncWatcher;

    beforeEach(() => {
        vi.useFakeTimers();
        watcher = new BiDirectionalSyncWatcher("/mock/vault");
        vi.clearAllMocks();
    });

    afterEach(() => {
        watcher.stopWatching();
        vi.useRealTimers();
    });

    it("should hash and debounce file changes, ignoring duplicates", async () => {
        const emitter = new EventEmitter();
        vi.mocked(fsp.watch).mockReturnValue({
            [Symbol.asyncIterator]() {
                return {
                    async next() {
                        return new Promise((resolve) => {
                            emitter.once('data', (data) => resolve({ value: data, done: false }));
                        });
                    }
                };
            }
        } as any);
        
        // Mock nội dung file
        vi.mocked(fsp.readFile).mockResolvedValue("Hello World");
        
        const promise = watcher.startWatching();
        
        emitter.emit('data', { filename: "note1.md" });
        emitter.emit('data', { filename: "note1.md" });
        
        // Advance timer để kích hoạt debounce (2000ms)
        await vi.advanceTimersByTimeAsync(2500);
        
        // Nó chỉ readFile 1 lần nhờ debounce gộp 2 event đầu lại
        expect(fsp.readFile).toHaveBeenCalledTimes(1);

        // Phát thêm 1 event nữa nhưng nội dung file KHÔNG ĐỔI
        emitter.emit('data', { filename: "note1.md" });
        await vi.advanceTimersByTimeAsync(2500);

        // fsp.readFile được gọi lần 2
        expect(fsp.readFile).toHaveBeenCalledTimes(2);

        watcher.stopWatching();
    });
});
