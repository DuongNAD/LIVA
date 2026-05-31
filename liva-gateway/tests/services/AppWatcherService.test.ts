import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock chokidar
vi.mock("chokidar", () => ({
    watch: vi.fn().mockReturnValue({
        on: vi.fn().mockReturnThis(),
        close: vi.fn(),
    }),
}));

// Mock fs
vi.mock("node:fs", () => ({
    existsSync: vi.fn().mockReturnValue(true),
    promises: {
        access: vi.fn(),
        readFile: vi.fn(),
    },
}));

// Mock MemoryManager
vi.mock("../../src/MemoryManager", () => ({
    MemoryManager: vi.fn(),
}));

import { AppWatcherService } from "@services/AppWatcherService";

describe("AppWatcherService — App Discovery via Shortcut Watcher", () => {
    let watcher: AppWatcherService;
    let mockMemory: any;

    beforeEach(() => {
        vi.clearAllMocks();
        mockMemory = { addMessage: vi.fn() };
        watcher = new AppWatcherService(mockMemory);
    });

    afterEach(() => {
        watcher.stop();
    });

    // ============================================================
    // Constructor
    // ============================================================
    describe("Constructor", () => {
        it("should create without error", () => {
            expect(watcher).toBeTruthy();
        });
    });

    // ============================================================
    // setCallback()
    // ============================================================
    describe("setCallback()", () => {
        it("should set callback without error", () => {
            const cb = vi.fn();
            watcher.setCallback(cb);
            // no throw = pass
        });
    });

    // ============================================================
    // start()
    // ============================================================
    describe("start()", () => {
        it("should start watching without error", () => {
            expect(() => watcher.start()).not.toThrow();
        });
    });

    // ============================================================
    // stop()
    // ============================================================
    describe("stop()", () => {
        it("should stop without error even if never started", () => {
            expect(() => watcher.stop()).not.toThrow();
        });

        it("should stop after start", () => {
            watcher.start();
            expect(() => watcher.stop()).not.toThrow();
        });
    });
});
