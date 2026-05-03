import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";

// Mock logger to prevent pino initialization during tests
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

// Use vi.hoisted() to declare mock functions that work across vi.mock factory boundaries
const { mockDbExec, mockDbClose, mockStmtGet, mockStmtAll, mockStmtRun, mockPrepare } = vi.hoisted(() => {
    const mockStmtRun = vi.fn(() => ({ changes: 1 }));
    const mockStmtGet = vi.fn();
    const mockStmtAll = vi.fn();
    const mockPrepare = vi.fn(() => ({
        get: mockStmtGet,
        all: mockStmtAll,
        run: mockStmtRun,
    }));
    const mockDbExec = vi.fn();
    const mockDbClose = vi.fn();
    return { mockDbExec, mockDbClose, mockStmtGet, mockStmtAll, mockStmtRun, mockPrepare };
});

vi.mock("node:sqlite", () => {
    class MockDatabaseSync {
        exec = mockDbExec;
        close = mockDbClose;
        prepare = mockPrepare;
        constructor(_path: string) {}
    }
    return { DatabaseSync: MockDatabaseSync };
});

import { SQLiteStorageProvider, createStorageProvider } from "../../src/memory/StorageProvider";

describe("SQLiteStorageProvider", () => {
    let provider: SQLiteStorageProvider;

    beforeEach(() => {
        vi.clearAllMocks();
        provider = new SQLiteStorageProvider(":memory:");
    });

    afterEach(async () => {
        await provider.close();
    });

    describe("initialize()", () => {
        it("should enable WAL mode and PRAGMAs on init", async () => {
            await provider.initialize();

            // WAL and other PRAGMAs should be set
            expect(mockDbExec).toHaveBeenCalledWith("PRAGMA journal_mode = WAL");
            expect(mockDbExec).toHaveBeenCalledWith("PRAGMA synchronous = NORMAL");
            expect(mockDbExec).toHaveBeenCalledWith("PRAGMA busy_timeout = 5000");
            expect(mockDbExec).toHaveBeenCalledWith("PRAGMA wal_autocheckpoint = 1000");
        });
    });

    describe("close()", () => {
        it("should close the database", async () => {
            await provider.initialize();
            await provider.close();
            expect(mockDbClose).toHaveBeenCalled();
        });

        it("should be safe to call close() on uninitialized provider", async () => {
            await provider.close();
            // Should not throw
        });
    });

    describe("get()", () => {
        it("should return null if db is not initialized", async () => {
            const result = await provider.get("facts", "key1");
            expect(result).toBeNull();
        });

        it("should prepare and execute SELECT with key", async () => {
            await provider.initialize();
            mockStmtGet.mockReturnValue({ key: "key1", value: "hello" });

            const result = await provider.get("facts", "key1");
            expect(mockPrepare).toHaveBeenCalledWith("SELECT * FROM facts WHERE key = ?");
            expect(mockStmtGet).toHaveBeenCalledWith("key1");
            expect(result).toEqual({ key: "key1", value: "hello" });
        });

        it("should return null if row does not exist", async () => {
            await provider.initialize();
            mockStmtGet.mockReturnValue(undefined);

            const result = await provider.get("facts", "nonexistent");
            expect(result).toBeNull();
        });
    });

    describe("getAll()", () => {
        it("should return empty array if db is not initialized", async () => {
            const result = await provider.getAll("facts");
            expect(result).toEqual([]);
        });

        it("should return all rows without filter", async () => {
            await provider.initialize();
            const mockRows = [{ key: "a" }, { key: "b" }];
            mockStmtAll.mockReturnValue(mockRows);

            const result = await provider.getAll("facts");
            expect(mockPrepare).toHaveBeenCalledWith("SELECT * FROM facts");
            expect(result).toEqual(mockRows);
        });

        it("should apply filter clauses", async () => {
            await provider.initialize();
            mockStmtAll.mockReturnValue([{ key: "a", status: "active" }]);

            const result = await provider.getAll("facts", { status: "active" });
            expect(mockPrepare).toHaveBeenCalledWith("SELECT * FROM facts WHERE status = ?");
            expect(mockStmtAll).toHaveBeenCalledWith("active");
            expect(result).toHaveLength(1);
        });

        it("should handle empty filter object", async () => {
            await provider.initialize();
            mockStmtAll.mockReturnValue([]);

            await provider.getAll("facts", {});
            expect(mockPrepare).toHaveBeenCalledWith("SELECT * FROM facts");
        });
    });

    describe("upsert()", () => {
        it("should be a no-op if db is not initialized", async () => {
            await provider.upsert("facts", "key1", { key: "key1", value: "test" });
            expect(mockPrepare).not.toHaveBeenCalled();
        });

        it("should prepare INSERT ON CONFLICT DO UPDATE", async () => {
            await provider.initialize();
            await provider.upsert("facts", "key1", { key: "key1", value: "test" });

            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("INSERT INTO facts")
            );
            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("ON CONFLICT(key) DO UPDATE SET")
            );
            expect(mockStmtRun).toHaveBeenCalledWith("key1", "test");
        });
    });

    describe("delete()", () => {
        it("should return false if db is not initialized", async () => {
            const result = await provider.delete("facts", "key1");
            expect(result).toBe(false);
        });

        it("should delete by key and return true on success", async () => {
            await provider.initialize();
            mockStmtRun.mockReturnValue({ changes: 1 });

            const result = await provider.delete("facts", "key1");
            expect(mockPrepare).toHaveBeenCalledWith("DELETE FROM facts WHERE key = ?");
            expect(result).toBe(true);
        });

        it("should return false if no rows affected", async () => {
            await provider.initialize();
            mockStmtRun.mockReturnValue({ changes: 0 });

            const result = await provider.delete("facts", "nonexistent");
            expect(result).toBe(false);
        });
    });

    describe("deleteAll()", () => {
        it("should be a no-op if db is not initialized", async () => {
            await provider.deleteAll("facts");
            expect(mockDbExec).not.toHaveBeenCalledWith(expect.stringContaining("DELETE"));
        });

        it("should execute DELETE FROM table", async () => {
            await provider.initialize();
            await provider.deleteAll("facts");
            expect(mockDbExec).toHaveBeenCalledWith("DELETE FROM facts");
        });
    });

    describe("count()", () => {
        it("should return 0 if db is not initialized", async () => {
            const result = await provider.count("facts");
            expect(result).toBe(0);
        });

        it("should return count of rows", async () => {
            await provider.initialize();
            mockStmtGet.mockReturnValue({ c: 42 });

            const result = await provider.count("facts");
            expect(mockPrepare).toHaveBeenCalledWith("SELECT count(*) as c FROM facts");
            expect(result).toBe(42);
        });

        it("should return 0 if count query returns null", async () => {
            await provider.initialize();
            mockStmtGet.mockReturnValue(null);

            const result = await provider.count("facts");
            expect(result).toBe(0);
        });
    });

    describe("exec()", () => {
        it("should be a no-op if db is not initialized", async () => {
            await provider.exec("CREATE TABLE test (id INT)");
            // No exec calls should happen except from initialize
        });

        it("should execute raw SQL", async () => {
            await provider.initialize();
            await provider.exec("CREATE TABLE test (id INT)");
            expect(mockDbExec).toHaveBeenCalledWith("CREATE TABLE test (id INT)");
        });
    });
});

describe("createStorageProvider Factory", () => {
    const originalEnv = process.env.STORAGE_PROVIDER;

    afterEach(() => {
        if (originalEnv !== undefined) {
            process.env.STORAGE_PROVIDER = originalEnv;
        } else {
            delete process.env.STORAGE_PROVIDER;
        }
    });

    it("should return SQLiteStorageProvider by default", () => {
        delete process.env.STORAGE_PROVIDER;
        const provider = createStorageProvider("/tmp/test.db");
        expect(provider).toBeInstanceOf(SQLiteStorageProvider);
    });

    it("should return SQLiteStorageProvider when env is sqlite", () => {
        process.env.STORAGE_PROVIDER = "sqlite";
        const provider = createStorageProvider("/tmp/test.db");
        expect(provider).toBeInstanceOf(SQLiteStorageProvider);
    });

    it("should fallback to SQLiteStorageProvider for unknown provider", () => {
        process.env.STORAGE_PROVIDER = "unknown_provider";
        const provider = createStorageProvider("/tmp/test.db");
        expect(provider).toBeInstanceOf(SQLiteStorageProvider);
    });
});
