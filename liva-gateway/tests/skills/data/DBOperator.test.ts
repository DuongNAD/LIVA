import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

const mockAccess = vi.fn();
vi.mock("node:fs/promises", () => ({
    access: (...args: any[]) => mockAccess(...args)
}));

// vi.mock is hoisted — cannot reference variables declared outside
const mockAll = vi.fn();
const mockRun = vi.fn();
const mockClose = vi.fn();

vi.mock("node:sqlite", () => {
    return {
        DatabaseSync: class {
            prepare() { return { all: mockAll, run: mockRun }; }
            close() { mockClose(); }
        }
    };
});

import { execute, metadata } from "../../../src/skills/data/DBOperator";

describe("Skill - DBOperator", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mockAccess.mockResolvedValue(undefined);
    });

    it("should export metadata", () => {
        expect(metadata.name).toBe("db_operator");
    });

    it("should execute SELECT and return rows", async () => {
        mockAll.mockReturnValue([{ id: 1 }, { id: 2 }]);
        const result = await execute({ dbPath: "test.db", query: "SELECT * FROM users" });
        expect(result).toContain("DB RESULT");
        expect(result).toContain("2 dòng");
    });

    it("should handle SELECT with 0 rows", async () => {
        mockAll.mockReturnValue([]);
        const result = await execute({ dbPath: "test.db", query: "SELECT * FROM empty" });
        expect(result).toContain("0 rows");
    });

    it("should truncate rows beyond MAX_ROWS", async () => {
        mockAll.mockReturnValue(Array.from({ length: 60 }, (_, i) => ({ id: i })));
        const result = await execute({ dbPath: "test.db", query: "SELECT * FROM big" });
        expect(result).toContain("60 dòng");
        expect(result).toContain("50 dòng đầu tiên");
    });

    it("should handle PRAGMA as SELECT", async () => {
        mockAll.mockReturnValue([{ name: "users" }]);
        const result = await execute({ dbPath: "test.db", query: "PRAGMA table_list" });
        expect(result).toContain("DB RESULT");
    });

    it("should execute INSERT and return changes", async () => {
        mockRun.mockReturnValue({ changes: 3 });
        const result = await execute({ dbPath: "test.db", query: "INSERT INTO t VALUES (1)" });
        expect(result).toContain("DB SUCCESS");
        expect(result).toContain("3");
    });

    it("should return error for missing db file", async () => {
        mockAccess.mockRejectedValue(new Error("ENOENT"));
        const result = await execute({ dbPath: "nonexistent.db", query: "SELECT 1" });
        expect(result).toContain("DB ERROR");
        expect(result).toContain("Không tìm thấy");
    });

    it("should handle ZodError", async () => {
        const result = await execute({});
        expect(result).toContain("DB ERROR");
        expect(result).toContain("Sai định dạng");
    });
});
