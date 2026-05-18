import { chromium, type Browser, type Page, type BrowserContext } from "playwright-core";
import * as path from "node:path";
import * as fs from "node:fs";
import { logger } from "./logger";

/**
 * PlaywrightBrowser — Shared Singleton Browser Manager
 * =====================================================
 * Manages a single browser instance using playwright-core.
 * Connects to the user's system Chrome/Edge — NO bundled browsers (saves ~500MB).
 *
 * Features:
 *   - Auto-detects Chrome/Edge on Windows
 *   - Persistent user profile via --user-data-dir
 *   - Anti-bot stealth mode
 *   - Singleton pattern to prevent multiple instances
 */

// Cache detected browser path
let cachedBrowserPath: string | null = null;

/**
 * Detect the user's installed Chrome or Edge browser executable.
 * Falls back in order: Chrome → Edge → Chromium
 */
export function getSystemChromePath(): string {
    if (cachedBrowserPath) return cachedBrowserPath;

    const candidates = [
        // Chrome
        process.env.CHROME_PATH,
        "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
        "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
        path.join(process.env.LOCALAPPDATA || "", "Google", "Chrome", "Application", "chrome.exe"),
        // Edge
        "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
        "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
        // Linux/macOS fallbacks
        "/usr/bin/google-chrome",
        "/usr/bin/chromium-browser",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    ];

    for (const candidate of candidates) {
        if (candidate && fs.existsSync(candidate)) {
            cachedBrowserPath = candidate;
            logger.info(`[PlaywrightBrowser] Detected system browser: ${candidate}`);
            return candidate;
        }
    }

    throw new Error(
        "[PlaywrightBrowser] Không tìm thấy Chrome hoặc Edge trên máy. " +
        "Hãy cài Chrome tại: https://www.google.com/chrome/ hoặc set CHROME_PATH env var."
    );
}

// ==========================================
// Singleton Browser Instances
// ==========================================

interface BrowserSlot {
    browser: Browser;
    context: BrowserContext;
    profileDir: string;
}

const browserSlots = new Map<string, BrowserSlot>();

/**
 * Get or create a persistent browser context for a given profile.
 * Each profile (e.g., "computer_use", "zalo", "messenger") has its own
 * isolated user data directory and cookie jar.
 */
export async function getOrCreateBrowser(profileName: string): Promise<{ browser: Browser; context: BrowserContext }> {
    const existing = browserSlots.get(profileName);
    if (existing?.browser.isConnected()) { // NOSONAR
        return { browser: existing.browser, context: existing.context };
    }

    const profileDir = path.resolve(process.cwd(), "data", `liva_${profileName}_profile`);
    if (!fs.existsSync(profileDir)) {
        fs.mkdirSync(profileDir, { recursive: true });
    }

    logger.info(`[PlaywrightBrowser] Launching browser for profile: ${profileName}`);

    const browser = await chromium.launchPersistentContext(profileDir, {
        executablePath: getSystemChromePath(),
        headless: false,
        viewport: null,
        args: [
            "--start-maximized",
            "--disable-extensions",
            "--disable-blink-features=AutomationControlled",
        ],
        ignoreDefaultArgs: ["--enable-automation"],
        // Stealth: override navigator.webdriver
        bypassCSP: true,
    });

    // Anti-bot: inject stealth script on every new page
    browser.on("page", async (page) => {
        await page.addInitScript(() => {
            Object.defineProperty(navigator, "webdriver", { get: () => undefined });
        });
    });

    // Set realistic user agent
    const pages = browser.pages();
    for (const page of pages) {
        await page.addInitScript(() => {
            Object.defineProperty(navigator, "webdriver", { get: () => undefined });
        });
    }

    browserSlots.set(profileName, {
        browser: browser as unknown as Browser,
        context: browser,
        profileDir,
    });

    return { browser: browser as unknown as Browser, context: browser };
}

/**
 * Get the active page for a profile, or a new page if no pages exist.
 */
export async function getActivePage(context: BrowserContext, urlFilter?: string): Promise<Page> {
    const pages = context.pages();

    if (urlFilter) {
        const match = pages.find(p => p.url().includes(urlFilter));
        if (match) return match;
    }

    return pages[pages.length - 1] || await context.newPage();
}

/**
 * Close a specific profile's browser.
 */
export async function closeBrowser(profileName: string): Promise<void> {
    const slot = browserSlots.get(profileName);
    if (slot) {
        await slot.context.close().catch(() => {});
        browserSlots.delete(profileName);
    }
}

// Re-export Playwright types for consumers
export type { Browser, Page, BrowserContext };
