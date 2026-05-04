import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ObsidianVaultManager } from '../../src/memory/ObsidianVaultManager';
import { promises as fsp } from 'node:fs';
import path from 'node:path';
import { logger } from '../../src/utils/logger';

vi.mock('node:fs', () => ({
    promises: {
        stat: vi.fn(),
        readFile: vi.fn(),
        mkdir: vi.fn(),
        writeFile: vi.fn(),
        rename: vi.fn()
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

describe('ObsidianVaultManager', () => {
    let manager: ObsidianVaultManager;
    const vaultRoot = path.resolve('/mock/vault');

    beforeEach(() => {
        vi.clearAllMocks();
        manager = new ObsidianVaultManager(vaultRoot);
    });

    describe('Path Traversal Guard', () => {
        it('should throw SECURITY_VIOLATION for traversal attempts', async () => {
            await expect(manager.readNote('../outside.md')).rejects.toThrow('SECURITY_VIOLATION');
            expect(logger.error).toHaveBeenCalledWith(expect.objectContaining({ relativePath: '../outside.md' }), expect.stringContaining('Path Traversal'));
        });
    });

    describe('safeAppendInsights', () => {
        it('should append insights safely with atomic write', async () => {
            (fsp.stat as any).mockResolvedValue({ mtimeMs: 1000 });
            (fsp.readFile as any).mockResolvedValue('Existing content\n');
            (fsp.mkdir as any).mockResolvedValue(undefined);
            (fsp.writeFile as any).mockResolvedValue(undefined);
            (fsp.rename as any).mockResolvedValue(undefined);

            await manager.safeAppendInsights('test.md', 'New Insight', 1000);

            expect(fsp.writeFile).toHaveBeenCalledWith(
                expect.stringContaining('.tmp'),
                'Existing content\n\n> [!ai] LIVA Graph Weaver:\n> New Insight\n',
                'utf-8'
            );
            expect(fsp.rename).toHaveBeenCalled();
            expect(logger.info).toHaveBeenCalledWith(expect.objectContaining({ file: 'test.md' }), expect.stringContaining('Atomic Write Success'));
        });

        it('should append properly if content has no trailing newline', async () => {
            (fsp.stat as any).mockResolvedValue({ mtimeMs: 1000 });
            (fsp.readFile as any).mockResolvedValue('Existing content');
            
            await manager.safeAppendInsights('test2.md', 'New Insight', 1000);

            expect(fsp.writeFile).toHaveBeenCalledWith(
                expect.any(String),
                'Existing content\n\n> [!ai] LIVA Graph Weaver:\n> New Insight\n',
                'utf-8'
            );
        });

        it('should throw CONCURRENCY_ERROR if file modified by user', async () => {
            (fsp.stat as any).mockResolvedValue({ mtimeMs: 2000 }); // newer than expected 1000
            
            await expect(manager.safeAppendInsights('test.md', 'Insight', 1000)).rejects.toThrow('CONCURRENCY_ERROR');
        });

        it('should handle ENOENT gracefully (new file)', async () => {
            const enoentError = new Error('Not found') as any;
            enoentError.code = 'ENOENT';
            (fsp.stat as any).mockRejectedValue(enoentError);
            
            await manager.safeAppendInsights('new.md', 'Insight', 1000);
            
            expect(fsp.writeFile).toHaveBeenCalledWith(
                expect.any(String),
                '\n> [!ai] LIVA Graph Weaver:\n> Insight\n',
                'utf-8'
            );
        });

        it('should lock file and prevent concurrent writes', async () => {
            (fsp.stat as any).mockResolvedValue({ mtimeMs: 1000 });
            // Make readFile slow to simulate concurrent operation
            (fsp.readFile as any).mockImplementation(() => new Promise(resolve => setTimeout(() => resolve('old'), 100)));
            
            const promise1 = manager.safeAppendInsights('test.md', 'Insight 1', 1000);
            const promise2 = manager.safeAppendInsights('test.md', 'Insight 2', 1000);
            
            await expect(promise2).rejects.toThrow('LOCKED');
            await promise1;
        });

        it('should release lock even if stat throws other error', async () => {
            const unexpectedError = new Error('Disk failure');
            (fsp.stat as any).mockRejectedValue(unexpectedError);
            
            await expect(manager.safeAppendInsights('error.md', 'Insight', 1000)).rejects.toThrow('Disk failure');
            
            // Should be unlocked, next attempt should reach fsp.stat again
            (fsp.stat as any).mockRejectedValue(unexpectedError);
            await expect(manager.safeAppendInsights('error.md', 'Insight', 1000)).rejects.toThrow('Disk failure');
        });
    });

    describe('readNote', () => {
        it('should read file content and mtime', async () => {
            (fsp.stat as any).mockResolvedValue({ mtimeMs: 5000 });
            (fsp.readFile as any).mockResolvedValue('Content');
            
            const result = await manager.readNote('test.md');
            
            expect(result.content).toBe('Content');
            expect(result.mtimeMs).toBe(5000);
            expect(fsp.readFile).toHaveBeenCalledWith(path.resolve(vaultRoot, 'test.md'), 'utf-8');
        });

        it('should throw FILE_NOT_FOUND on ENOENT', async () => {
            const enoentError = new Error('Not found') as any;
            enoentError.code = 'ENOENT';
            (fsp.stat as any).mockRejectedValue(enoentError);
            
            await expect(manager.readNote('missing.md')).rejects.toThrow('FILE_NOT_FOUND');
        });

        it('should prevent reading if file is locked', async () => {
            (fsp.stat as any).mockImplementation(() => new Promise(resolve => setTimeout(() => resolve({ mtimeMs: 1000 }), 100)));
            
            const writePromise = manager.safeAppendInsights('test.md', 'Insight', 1000);
            await expect(manager.readNote('test.md')).rejects.toThrow('LOCKED');
            await writePromise;
        });

        it('should throw original error if not ENOENT', async () => {
            const eaccesError = new Error('Permission denied') as any;
            eaccesError.code = 'EACCES';
            (fsp.stat as any).mockRejectedValue(eaccesError);
            
            await expect(manager.readNote('noperms.md')).rejects.toThrow('Permission denied');
        });
    });

    describe('createOrOverwriteNote', () => {
        it('should create or overwrite file safely', async () => {
            await manager.createOrOverwriteNote('test.md', 'New Content');
            
            expect(fsp.writeFile).toHaveBeenCalledWith(
                expect.any(String),
                'New Content',
                'utf-8'
            );
            expect(fsp.rename).toHaveBeenCalled();
            expect(logger.info).toHaveBeenCalledWith(expect.objectContaining({ file: 'test.md' }), expect.stringContaining('Atomic Write/Create Success'));
        });

        it('should prevent concurrent writes', async () => {
            (fsp.mkdir as any).mockImplementation(() => new Promise(resolve => setTimeout(resolve, 100)));
            
            const promise1 = manager.createOrOverwriteNote('test.md', 'Content 1');
            const promise2 = manager.createOrOverwriteNote('test.md', 'Content 2');
            
            await expect(promise2).rejects.toThrow('LOCKED');
            await promise1;
        });
    });
});
