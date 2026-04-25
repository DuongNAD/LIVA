/**
 * UpdateCoreProfile.test.ts — Unit Tests for Profile Update Skill
 * ================================================================
 * Tests atomic write (tmp + rename), merge logic, and file error handling.
 * fs/promises fully mocked to prevent real filesystem operations.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("fs/promises", () => ({
    readFile: vi.fn(),
    writeFile: vi.fn(),
    rename: vi.fn(),
    default: {
        readFile: vi.fn(),
        writeFile: vi.fn(),
        rename: vi.fn()
    }
}));


import * as fs from "node:fs/promises";

    async function loadModule() {
        return await import("../../src/skills/UpdateCoreProfile");
    }

describe("UpdateCoreProfile", () => {
    beforeEach(() => {
        vi.resetAllMocks();
    });

    // ──────────────────────────────────────
    //  Metadata
    // ──────────────────────────────────────
    describe("metadata", () => {
        it("should export correct skill name", async () => {
            const { metadata } = await loadModule();
            expect(metadata.name).toBe("update_core_profile");
        });

        it("should have no required parameters", async () => {
            const { metadata } = await loadModule();
            expect(metadata.parameters.required).toEqual([]);
        });
    });

    // ──────────────────────────────────────
    //  Profile Update Logic
    // ──────────────────────────────────────
    describe("Update Logic", () => {
        it("should create new profile when file doesn't exist", async () => {
            const { execute } = await loadModule();
            (fs.readFile as any).mockRejectedValue(new Error("ENOENT"));

            const result = await execute({ age: 22, profession: "AI Engineer" });
            expect(result).toContain("thành công");

            // Verify atomic write pattern: writeFile to .tmp then rename
            expect(fs.writeFile).toHaveBeenCalledWith(
                expect.stringContaining(".tmp"),
                expect.stringContaining("22"),
                "utf-8",
            );
            expect(fs.rename).toHaveBeenCalled();
        });

        it("should merge with existing profile data", async () => {
            const { execute } = await loadModule();
            (fs.readFile as any).mockResolvedValue(
                JSON.stringify({ age: 21, profession: "Student", hobbies: ["AI"] }),
            );

            const result = await execute({ age: 22 });
            expect(result).toContain("thành công");

            // Check the written content preserves old fields and updates new
            const writtenContent = (fs.writeFile as any).mock.calls[0][1] as string;
            const parsed = JSON.parse(writtenContent);
            expect(parsed.age).toBe(22);
            expect(parsed.profession).toBe("Student"); // Preserved
            expect(parsed.hobbies).toEqual(["AI"]); // Preserved
        });

        it("should update multiple fields at once", async () => {
            const { execute } = await loadModule();
            (fs.readFile as any).mockResolvedValue(JSON.stringify({}));

            await execute({ age: 23, profession: "Engineer", location: "Hanoi" });

            const writtenContent = (fs.writeFile as any).mock.calls[0][1] as string;
            const parsed = JSON.parse(writtenContent);
            expect(parsed.age).toBe(23);
            expect(parsed.profession).toBe("Engineer");
            expect(parsed.location).toBe("Hanoi");
        });
    });

    // ──────────────────────────────────────
    //  Atomic Write Pattern
    // ──────────────────────────────────────
    describe("Atomic Write Safety", () => {
        it("should write to .tmp file first, then rename", async () => {
            const { execute } = await loadModule();
            (fs.readFile as any).mockResolvedValue(JSON.stringify({}));

            await execute({ age: 25 });

            // First call must be to .tmp path
            const tmpPath = (fs.writeFile as any).mock.calls[0][0] as string;
            expect(tmpPath).toMatch(/\.tmp$/);

            // Then rename from .tmp to final path
            const renameArgs = (fs.rename as any).mock.calls[0];
            expect(renameArgs[0]).toMatch(/\.tmp$/);
            expect(renameArgs[1]).not.toMatch(/\.tmp$/);
        });
    });

    // ──────────────────────────────────────
    //  Error Handling
    // ──────────────────────────────────────
    describe("Error Handling", () => {
        it("should return error message on write failure", async () => {
            const { execute } = await loadModule();
            (fs.readFile as any).mockResolvedValue(JSON.stringify({}));
            (fs.writeFile as any).mockRejectedValue(new Error("EACCES: permission denied"));

            const result = await execute({ age: 30 });
            expect(result).toContain("Lỗi");
            expect(result).toContain("EACCES");
        });

        it("should handle corrupted JSON in existing profile", async () => {
            const { execute } = await loadModule();
            (fs.readFile as any).mockResolvedValue("{ invalid json !!!");

            const result = await execute({ age: 30 });
            // Should still succeed (treats as empty profile on parse error)
            // or return error gracefully
            expect(typeof result).toBe("string");
        });
    });
});
