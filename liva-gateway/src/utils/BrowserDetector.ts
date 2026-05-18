/**
 * BrowserDetector — Shared Chrome/Edge Detection Utility
 * =======================================================
 * Eliminates code duplication between PlaywrightBrowser.ts,
 * ChromeLauncher.ts, and GeminiSurfer.ts.
 * 
 * Provides:
 *   - fileExists() — async file existence check
 *   - detectSystemBrowser() — cached async Chrome/Edge path detection
 *   - BROWSER_CANDIDATES — shared list of browser executable paths
 */
import * as path from "node:path";
import { promises as fsp } from "node:fs";
import * as fs from "node:fs";
import { logger } from "./logger";

// ============================================================
// Shared Browser Candidate Paths
// ============================================================

export const BROWSER_CANDIDATES: (string | undefined)[] = [
    process.env.CHROME_PATH,
    // Chrome
    "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
    "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
    path.join(process.env.LOCALAPPDATA || "", "Google", "Chrome", "Application", "chrome.exe"),
    // Edge
    "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
    "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
    // Linux/macOS
    "/usr/bin/google-chrome",
    "/usr/bin/chromium-browser",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
];

// ============================================================
// Async File Existence Check
// ============================================================

/**
 * Check if a file exists (async, non-throwing).
 * Shared across ChromeLauncher, GeminiSurfer, and other modules.
 */
export async function fileExists(p: string): Promise<boolean> {
    try {
        await fsp.access(p);
        return true;
    } catch {
        return false;
    }
}

/**
 * Check if a file exists (sync, non-throwing).
 * For use in synchronous contexts like PlaywrightBrowser.
 */
export function fileExistsSync(p: string): boolean {
    return fs.existsSync(p);
}

// ============================================================
// Cached Browser Detection
// ============================================================

let cachedBrowserPath: string | null = null;

/**
 * Detect the user's installed Chrome or Edge browser (async).
 * Result is cached after first successful detection.
 */
export async function detectSystemBrowser(): Promise<string> {
    if (cachedBrowserPath) return cachedBrowserPath;

    for (const candidate of BROWSER_CANDIDATES) {
        if (candidate && (await fileExists(candidate))) {
            cachedBrowserPath = candidate;
            logger.info(`[BrowserDetector] Detected browser: ${candidate}`);
            return candidate;
        }
    }

    throw new Error(
        "[BrowserDetector] Chrome/Edge not found. Install Chrome or set CHROME_PATH env var."
    );
}

/**
 * Detect the user's installed Chrome or Edge browser (sync).
 * For use in PlaywrightBrowser which needs synchronous detection.
 */
export function detectSystemBrowserSync(): string {
    if (cachedBrowserPath) return cachedBrowserPath;

    for (const candidate of BROWSER_CANDIDATES) {
        if (candidate && fileExistsSync(candidate)) {
            cachedBrowserPath = candidate;
            logger.info(`[BrowserDetector] Detected browser: ${candidate}`);
            return candidate;
        }
    }

    throw new Error(
        "[BrowserDetector] Chrome/Edge not found. Install Chrome or set CHROME_PATH env var."
    );
}
