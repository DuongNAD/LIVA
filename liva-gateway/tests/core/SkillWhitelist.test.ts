import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

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
        readFile: vi.fn(),
        writeFile: vi.fn(),
        rename: vi.fn(),
        mkdir: vi.fn(),
    },
}));

// Mock FileUtils (safeRename)
vi.mock("../../src/utils/FileUtils", () => ({
    safeRename: vi.fn().mockResolvedValue(undefined),
}));

import { SkillWhitelist } from "@core/SkillWhitelist";
import { promises as fsp } from "node:fs";

describe("SkillWhitelist — Persistent Skill Enablement Registry", () => {
    let whitelist: SkillWhitelist;

    beforeEach(() => {
        vi.useFakeTimers();
        whitelist = new SkillWhitelist();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    // ============================================================
    // Default behavior
    // ============================================================
    describe("Default behavior", () => {
        it("should enable all skills by default (no explicit entry)", () => {
            expect(whitelist.isEnabled("any_skill")).toBe(true);
        });

        it("should return empty getAll() initially", () => {
            expect(whitelist.getAll()).toEqual({});
        });

        it("should return empty getDisabledSkills() initially", () => {
            expect(whitelist.getDisabledSkills().size).toBe(0);
        });
    });

    // ============================================================
    // load() — File persistence
    // ============================================================
    describe("load()", () => {
        it("should load skill states from disk", async () => {
            vi.mocked(fsp.readFile).mockResolvedValue(
                JSON.stringify({
                    "weather": { enabled: true, lastToggled: 1000 },
                    "email": { enabled: false, lastToggled: 2000, note: "Too risky" },
                })
            );

            await whitelist.load();
            expect(whitelist.isEnabled("weather")).toBe(true);
            expect(whitelist.isEnabled("email")).toBe(false);
        });

        it("should start with defaults if file is missing (ENOENT)", async () => {
            const err = new Error("ENOENT") as NodeJS.ErrnoException;
            err.code = "ENOENT";
            vi.mocked(fsp.readFile).mockRejectedValue(err);

            await whitelist.load(); // should not throw
            expect(whitelist.isEnabled("any_skill")).toBe(true);
        });

        it("should warn and start with defaults if file is corrupted", async () => {
            vi.mocked(fsp.readFile).mockRejectedValue(new Error("EISDIR"));
            await whitelist.load(); // should not throw
            expect(whitelist.isEnabled("any_skill")).toBe(true);
        });
    });

    // ============================================================
    // setEnabled() — Toggle skill
    // ============================================================
    describe("setEnabled()", () => {
        it("should disable a skill", () => {
            whitelist.setEnabled("dangerous_skill", false);
            expect(whitelist.isEnabled("dangerous_skill")).toBe(false);
        });

        it("should enable a previously disabled skill", () => {
            whitelist.setEnabled("skill_a", false);
            whitelist.setEnabled("skill_a", true);
            expect(whitelist.isEnabled("skill_a")).toBe(true);
        });

        it("should store optional note", () => {
            whitelist.setEnabled("skill_b", false, "Broken API");
            const all = whitelist.getAll();
            expect(all["skill_b"].note).toBe("Broken API");
        });

        it("should track lastToggled timestamp", () => {
            const now = Date.now();
            whitelist.setEnabled("skill_c", true);
            const all = whitelist.getAll();
            expect(all["skill_c"].lastToggled).toBeGreaterThanOrEqual(now);
        });
    });

    // ============================================================
    // bulkSet()
    // ============================================================
    describe("bulkSet()", () => {
        it("should set multiple skills at once", () => {
            whitelist.bulkSet([
                { name: "a", enabled: true },
                { name: "b", enabled: false },
                { name: "c", enabled: false },
            ]);

            expect(whitelist.isEnabled("a")).toBe(true);
            expect(whitelist.isEnabled("b")).toBe(false);
            expect(whitelist.isEnabled("c")).toBe(false);
        });
    });

    // ============================================================
    // getDisabledSkills()
    // ============================================================
    describe("getDisabledSkills()", () => {
        it("should return set of disabled skill names", () => {
            whitelist.setEnabled("x", false);
            whitelist.setEnabled("y", true);
            whitelist.setEnabled("z", false);

            const disabled = whitelist.getDisabledSkills();
            expect(disabled.has("x")).toBe(true);
            expect(disabled.has("z")).toBe(true);
            expect(disabled.has("y")).toBe(false);
            expect(disabled.size).toBe(2);
        });
    });

    // ============================================================
    // Debounced save
    // ============================================================
    describe("Debounced save", () => {
        it("should trigger save after 500ms debounce", async () => {
            vi.mocked(fsp.mkdir).mockResolvedValue(undefined);
            vi.mocked(fsp.writeFile).mockResolvedValue(undefined);

            whitelist.setEnabled("test", true);

            // Before 500ms — should NOT have saved yet
            expect(fsp.writeFile).not.toHaveBeenCalled();

            // Advance past debounce
            await vi.advanceTimersByTimeAsync(600);

            expect(fsp.writeFile).toHaveBeenCalled();
        });

        it("should debounce multiple rapid toggles into one save", async () => {
            vi.mocked(fsp.mkdir).mockResolvedValue(undefined);
            vi.mocked(fsp.writeFile).mockResolvedValue(undefined);

            // Clear any writes from previous tests
            vi.mocked(fsp.writeFile).mockClear();

            whitelist.setEnabled("a", true);
            whitelist.setEnabled("b", false);
            whitelist.setEnabled("c", true);

            await vi.advanceTimersByTimeAsync(600);

            // Only 1 save despite 3 toggles
            expect(fsp.writeFile).toHaveBeenCalledTimes(1);
        });
    });

    // ============================================================
    // dispose()
    // ============================================================
    describe("dispose()", () => {
        it("should flush pending save on dispose", async () => {
            vi.mocked(fsp.mkdir).mockResolvedValue(undefined);
            vi.mocked(fsp.writeFile).mockResolvedValue(undefined);

            whitelist.setEnabled("test", true);
            await whitelist.dispose();

            expect(fsp.writeFile).toHaveBeenCalled();
        });

        it("should be no-op if no pending save", async () => {
            await whitelist.dispose(); // no throw
        });
    });
});
