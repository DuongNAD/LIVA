/**
 * ShieldGuard.test.ts — Snapshot & Rollback Tests
 * =================================================
 * Tests snapshot creation, rollback, and error handling.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("fs")>();
    return {
        ...actual,
        existsSync: vi.fn(),
        mkdirSync: vi.fn(),
        rmSync: vi.fn(),
        cpSync: vi.fn(),
        default: {
            ...actual,
            existsSync: vi.fn(),
            mkdirSync: vi.fn(),
            rmSync: vi.fn(),
            cpSync: vi.fn(),
        }
    };
});

vi.mock("fs/promises", async (importOriginal) => {
    const actual = await importOriginal<typeof import("fs/promises")>();
    return {
        ...actual,
        mkdir: vi.fn(),
        rm: vi.fn(),
        cp: vi.fn(),
        default: {
            ...actual,
            mkdir: vi.fn(),
            rm: vi.fn(),
            cp: vi.fn(),
        }
    };
});


// ============================================================
// Mock fs and fs/promises BEFORE importing ShieldGuard
// ============================================================
import * as fs from "node:fs";
import * as fsp from "node:fs/promises";
import { ShieldGuard } from "../../src/utils/ShieldGuard";

describe("ShieldGuard", () => {
    beforeEach(() => {
        vi.resetAllMocks();
    });
    describe("deploy()", () => {
        it("should delete old snapshot before creating new one", async () => {
            (fs.existsSync as any).mockReturnValueOnce(true).mockReturnValueOnce(true).mockReturnValueOnce(true);

            await ShieldGuard.deploy();

            expect(fsp.rm).toHaveBeenCalledTimes(1);
            expect(fsp.rm).toHaveBeenCalledWith(
                expect.stringContaining("snapshot_latest"),
                { recursive: true, force: true }
            );
        });

        it("should skip data copy when data directory does not exist", async () => {
            (fs.existsSync as any).mockReturnValueOnce(true).mockReturnValueOnce(false).mockReturnValueOnce(false);

            await ShieldGuard.deploy();

            expect(fsp.cp).toHaveBeenCalledTimes(1);
            expect(fsp.cp).toHaveBeenCalledWith(
                expect.stringContaining("src"),
                expect.stringContaining("src"),
                { recursive: true }
            );
        });

        it("should handle filesystem errors gracefully without crashing", async () => {
            (fs.existsSync as any).mockReturnValue(false);
            (fsp.mkdir as any).mockRejectedValueOnce(new Error("EPERM"));

            await expect(ShieldGuard.deploy()).resolves.toBeUndefined();
        });
    });

    describe("rollback()", () => {
        it("should abort if no snapshot exists", async () => {
            (fs.existsSync as any).mockReturnValue(false);

            await ShieldGuard.rollback();

            expect(fs.cpSync).not.toHaveBeenCalled();
        });

        it("should restore src and data from snapshot", async () => {
            (fs.existsSync as any).mockReturnValueOnce(true).mockReturnValueOnce(true);

            await ShieldGuard.rollback();

            expect(fsp.cp).toHaveBeenCalledTimes(2);
            expect(fsp.cp).toHaveBeenCalledWith(
                expect.stringContaining("src"),
                expect.stringContaining("src"),
                { recursive: true, force: true }
            );
        });

        it("should skip data restore when data backup does not exist", async () => {
            (fs.existsSync as any).mockReturnValueOnce(true).mockReturnValueOnce(false);

            await ShieldGuard.rollback();

            expect(fsp.cp).toHaveBeenCalledTimes(1);
        });

        it("should handle restore errors gracefully", async () => {
            (fs.existsSync as any).mockReturnValue(true);
            (fsp.cp as any).mockRejectedValueOnce(new Error("EIO"));

            await expect(ShieldGuard.rollback()).resolves.toBeUndefined();
        });
    });
});
