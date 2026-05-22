import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

// Mock fs
vi.mock("node:fs", () => ({
    promises: {
        rename: vi.fn(),
    },
}));

import { safeRename } from "@utils/FileUtils";
import { promises as fsp } from "node:fs";

describe("FileUtils — safeRename()", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    // ============================================================
    // Happy Path
    // ============================================================
    describe("Successful rename", () => {
        it("should rename file on first attempt", async () => {
            vi.mocked(fsp.rename).mockResolvedValue(undefined);
            await safeRename("/old", "/new");
            expect(fsp.rename).toHaveBeenCalledTimes(1);
            expect(fsp.rename).toHaveBeenCalledWith("/old", "/new");
        });
    });

    // ============================================================
    // Retry on Windows lock errors
    // ============================================================
    describe("Retry on EPERM/EBUSY/EACCES", () => {
        it("should retry on EPERM and succeed on 2nd attempt", async () => {
            const lockError = Object.assign(new Error("EPERM"), { code: "EPERM" });
            vi.mocked(fsp.rename)
                .mockRejectedValueOnce(lockError)
                .mockResolvedValueOnce(undefined);

            await safeRename("/old", "/new", 3, 10);
            expect(fsp.rename).toHaveBeenCalledTimes(2);
        });

        it("should retry on EBUSY and succeed on 3rd attempt", async () => {
            const busyError = Object.assign(new Error("EBUSY"), { code: "EBUSY" });
            vi.mocked(fsp.rename)
                .mockRejectedValueOnce(busyError)
                .mockRejectedValueOnce(busyError)
                .mockResolvedValueOnce(undefined);

            await safeRename("/old", "/new", 3, 10);
            expect(fsp.rename).toHaveBeenCalledTimes(3);
        });

        it("should retry on EACCES", async () => {
            const accessError = Object.assign(new Error("EACCES"), { code: "EACCES" });
            vi.mocked(fsp.rename)
                .mockRejectedValueOnce(accessError)
                .mockResolvedValueOnce(undefined);

            await safeRename("/old", "/new", 3, 10);
            expect(fsp.rename).toHaveBeenCalledTimes(2);
        });
    });

    // ============================================================
    // Throw after max retries
    // ============================================================
    describe("Max retries exhausted", () => {
        it("should throw after maxRetries on persistent EPERM", async () => {
            const lockError = Object.assign(new Error("EPERM"), { code: "EPERM" });
            vi.mocked(fsp.rename).mockRejectedValue(lockError);

            await expect(safeRename("/old", "/new", 3, 10)).rejects.toThrow("EPERM");
            expect(fsp.rename).toHaveBeenCalledTimes(3);
        });
    });

    // ============================================================
    // Non-retryable errors (thrown immediately)
    // ============================================================
    describe("Non-retryable errors", () => {
        it("should throw ENOENT immediately without retry", async () => {
            const enoent = Object.assign(new Error("ENOENT"), { code: "ENOENT" });
            vi.mocked(fsp.rename).mockRejectedValue(enoent);

            await expect(safeRename("/old", "/new", 3, 10)).rejects.toThrow("ENOENT");
            expect(fsp.rename).toHaveBeenCalledTimes(1);
        });

        it("should throw generic Error immediately without retry", async () => {
            vi.mocked(fsp.rename).mockRejectedValue(new Error("Unknown error"));

            await expect(safeRename("/old", "/new", 3, 10)).rejects.toThrow("Unknown error");
            expect(fsp.rename).toHaveBeenCalledTimes(1);
        });
    });

    // ============================================================
    // Custom parameters
    // ============================================================
    describe("Custom parameters", () => {
        it("should respect custom maxRetries", async () => {
            const lockError = Object.assign(new Error("EPERM"), { code: "EPERM" });
            vi.mocked(fsp.rename).mockRejectedValue(lockError);

            await expect(safeRename("/old", "/new", 1, 10)).rejects.toThrow();
            expect(fsp.rename).toHaveBeenCalledTimes(1);
        });
    });
});
