/**
 * BrowserDetector.test.ts — Full Coverage Tests
 * Tests browser path detection for Chrome/Edge on multiple platforms.
 * Mocks filesystem to avoid real path checks.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as fsp from "node:fs/promises";

// Mock fs modules — BrowserDetector imports { promises as fsp } from "node:fs"
vi.mock("fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("fs")>();
    return {
        ...actual,
        existsSync: vi.fn().mockReturnValue(false),
        mkdirSync: vi.fn(),
        promises: {
            access: vi.fn().mockRejectedValue(new Error("ENOENT")),
        },
        default: {
            ...actual,
            existsSync: vi.fn().mockReturnValue(false),
            mkdirSync: vi.fn(),
            promises: {
                access: vi.fn().mockRejectedValue(new Error("ENOENT")),
            },
        }
    };
});

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import * as fs from "node:fs";

describe("BrowserDetector", () => {
    let originalChromePath: string | undefined;

    beforeEach(() => {
        vi.clearAllMocks();
        originalChromePath = process.env.CHROME_PATH;
    });

    afterEach(() => {
        if (originalChromePath !== undefined) {
            process.env.CHROME_PATH = originalChromePath;
        } else {
            delete process.env.CHROME_PATH;
        }
    });

    describe("fileExistsSync", () => {
        it("should delegate to fs.existsSync", async () => {
            (fs.existsSync as any).mockReturnValueOnce(true);
            const { fileExistsSync } = await import("../../src/utils/BrowserDetector");
            const result = fileExistsSync("C:\\test\\chrome.exe");
            expect(result).toBe(true);
            expect(fs.existsSync).toHaveBeenCalledWith("C:\\test\\chrome.exe");
        });

        it("should return false when fs.existsSync returns false", async () => {
            (fs.existsSync as any).mockReturnValueOnce(false);
            const { fileExistsSync } = await import("../../src/utils/BrowserDetector");
            const result = fileExistsSync("C:\\nonexistent");
            expect(result).toBe(false);
        });
    });

    describe("BROWSER_CANDIDATES", () => {
        it("should export a list of browser candidate paths", async () => {
            const { BROWSER_CANDIDATES } = await import("../../src/utils/BrowserDetector");
            expect(Array.isArray(BROWSER_CANDIDATES)).toBe(true);
            expect(BROWSER_CANDIDATES.length).toBeGreaterThan(3);
        });

        it("should include common Chrome/Edge paths", async () => {
            const { BROWSER_CANDIDATES } = await import("../../src/utils/BrowserDetector");
            const paths = BROWSER_CANDIDATES.filter(Boolean) as string[];
            const hasChrome = paths.some(p => p.includes("chrome"));
            const hasEdge = paths.some(p => p.includes("edge") || p.includes("Edge"));
            expect(hasChrome || hasEdge).toBe(true);
        });
    });

    describe("detectSystemBrowserSync", () => {
        it("should throw when no browser is found", async () => {
            (fs.existsSync as any).mockReturnValue(false);
            vi.resetModules();
            const mod = await import("../../src/utils/BrowserDetector");
            expect(() => mod.detectSystemBrowserSync()).toThrow("Chrome/Edge not found");
        });
    });

    describe("fileExists (async)", () => {
        it("should return true when file exists", async () => {
            vi.resetModules();
            const { fileExists } = await import("../../src/utils/BrowserDetector");
            // fileExists uses fsp.access internally — already mocked to reject
            // Override for this test to resolve
            const fsModule = await import("fs");
            (fsModule.promises.access as any).mockResolvedValueOnce(undefined);
            const result = await fileExists("C:\\some\\chrome.exe");
            expect(result).toBe(true);
        });

        it("should return false when file does not exist", async () => {
            vi.resetModules();
            const { fileExists } = await import("../../src/utils/BrowserDetector");
            const fsModule = await import("fs");
            (fsModule.promises.access as any).mockRejectedValueOnce(new Error("ENOENT"));
            const result = await fileExists("C:\\nonexistent\\chrome.exe");
            expect(result).toBe(false);
        });
    });
});
