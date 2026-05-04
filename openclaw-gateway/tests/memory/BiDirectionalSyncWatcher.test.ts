import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { BiDirectionalSyncWatcher } from '../../src/memory/BiDirectionalSyncWatcher';
import { promises as fsp } from 'node:fs';
import path from 'node:path';
import { logger } from '../../src/utils/logger';

vi.mock('node:fs', () => ({
    promises: {
        watch: vi.fn(),
        readFile: vi.fn()
    }
}));

vi.mock('../../src/utils/logger', () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn(),
        debug: vi.fn()
    }
}));

describe('BiDirectionalSyncWatcher', () => {
    let watcher: BiDirectionalSyncWatcher;
    let eventQueue: any[] = [];
    let resolveNextEvent: ((value: any) => void) | null = null;
    let signal: AbortSignal;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        eventQueue = [];
        resolveNextEvent = null;

        (fsp.watch as any).mockImplementation(async function* (targetPath: string, options: any) {
            signal = options.signal;
            try {
                while (true) {
                    if (signal.aborted) {
                        const err = new Error('The operation was aborted');
                        err.name = 'AbortError';
                        throw err;
                    }
                    if (eventQueue.length > 0) {
                        const event = eventQueue.shift();
                        if (event instanceof Error) throw event;
                        yield event;
                    } else {
                        await new Promise<void>(resolve => {
                            resolveNextEvent = () => resolve();
                            signal.addEventListener('abort', () => resolve(), { once: true });
                        });
                    }
                }
            } catch (err: any) {
                if (err.name !== 'AbortError') throw err;
                // re-throw AbortError so it reaches the source code's catch
                throw err;
            }
        });

        watcher = new BiDirectionalSyncWatcher('/mock/vault');
    });

    afterEach(() => {
        watcher.stopWatching();
        vi.useRealTimers();
    });

    const pushEvent = (event: any) => {
        eventQueue.push(event);
        if (resolveNextEvent) {
            resolveNextEvent(null);
            resolveNextEvent = null;
        }
    };

    it('should start watching and handle valid .md file changes', async () => {
        const watchPromise = watcher.startWatching();
        
        (fsp.readFile as any).mockResolvedValue('file content');
        
        pushEvent({ filename: 'test.md' });
        
        // Advance timers to trigger debounce
        await vi.runAllTimersAsync();
        
        expect(fsp.readFile).toHaveBeenCalledWith(path.resolve('/mock/vault', 'test.md'), 'utf-8');
        expect(logger.info).toHaveBeenCalledWith(expect.objectContaining({ file: path.resolve('/mock/vault', 'test.md') }), expect.stringContaining('Triggering Re-embed'));
        
        watcher.stopWatching();
        await watchPromise;
    });

    it('should ignore non-md files', async () => {
        const watchPromise = watcher.startWatching();
        
        pushEvent({ filename: 'test.txt' });
        pushEvent({ filename: null }); // Ignore missing filename
        
        await vi.runAllTimersAsync();
        
        expect(fsp.readFile).not.toHaveBeenCalled();
        
        watcher.stopWatching();
        await watchPromise;
    });

    it('should deduplicate same hash events', async () => {
        const watchPromise = watcher.startWatching();
        
        (fsp.readFile as any).mockResolvedValue('same content');
        
        // First event
        pushEvent({ filename: 'test.md' });
        await vi.runAllTimersAsync();
        
        expect(fsp.readFile).toHaveBeenCalledTimes(1);
        expect(logger.info).toHaveBeenCalledTimes(1); // One log for trigger re-embed
        
        // Second event with same content
        pushEvent({ filename: 'test.md' });
        await vi.runAllTimersAsync();
        
        expect(fsp.readFile).toHaveBeenCalledTimes(2); // Reads again
        expect(logger.info).toHaveBeenCalledTimes(1); // But no new log because hash is same!
        
        watcher.stopWatching();
        await watchPromise;
    });

    it('should handle ENOENT as file deleted', async () => {
        const watchPromise = watcher.startWatching();
        
        const enoentError = new Error('Not found') as any;
        enoentError.code = 'ENOENT';
        (fsp.readFile as any).mockRejectedValue(enoentError);
        
        pushEvent({ filename: 'deleted.md' });
        await vi.runAllTimersAsync();
        
        expect(logger.info).toHaveBeenCalledWith(expect.objectContaining({ file: path.resolve('/mock/vault', 'deleted.md') }), expect.stringContaining('File deleted'));
        
        watcher.stopWatching();
        await watchPromise;
    });

    it('should log error if processFile throws unexpected error', async () => {
        const watchPromise = watcher.startWatching();
        
        (fsp.readFile as any).mockRejectedValue(new Error('Random read error'));
        
        pushEvent({ filename: 'error.md' });
        await vi.runAllTimersAsync();
        
        expect(logger.error).toHaveBeenCalledWith(expect.objectContaining({ err: 'Random read error' }), "ProcessFile Error");
        
        watcher.stopWatching();
        await watchPromise;
    });

    it('should log crash if watcher iterator throws', async () => {
        const watchPromise = watcher.startWatching();
        
        pushEvent(new Error('Watcher crashed'));
        await watchPromise; // Should resolve after crash
        
        expect(logger.error).toHaveBeenCalledWith(expect.objectContaining({ err: expect.any(Error) }), "BiDirectionalSyncWatcher crashed");
    });

    // removed handleFileChange rejection test as it never rejects in reality

    it("should handle multiple rapid changes and clear debounce timer (Line 60)", async () => {
        const watchPromise = watcher.startWatching();
        
        (fsp.readFile as any).mockResolvedValue("content");

        // Emulate rapid events
        pushEvent({ filename: "rapid.md" });
        pushEvent({ filename: "rapid.md" }); // second one clears the timer of the first
        
        await vi.runAllTimersAsync();
        
        expect(fsp.readFile).toHaveBeenCalledTimes(1); // Should only read once because of debounce!
        
        watcher.stopWatching();
        await watchPromise;
    });

    it("should clear timers on stopWatching (Line 52) and handle AbortError (Line 37)", async () => {
        const infoSpy = vi.spyOn(logger, "info");

        // Put a real timer in by triggering an event
        const watchPromise = watcher.startWatching();
        pushEvent({ filename: "toclear.md" });
        // Wait for event to process (must use fake timer advancement)
        await vi.advanceTimersByTimeAsync(0);
        
        watcher.stopWatching(); 
        await watchPromise;
        
        expect(infoSpy).toHaveBeenCalledWith("BiDirectionalSyncWatcher stopped.");
    });
});
