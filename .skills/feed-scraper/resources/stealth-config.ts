import { chromium } from 'playwright-extra';
import StealthPlugin from 'puppeteer-extra-plugin-stealth';

/**
 * Standard Boilerplate for Registering and Launching Stealth-Augmented Browser Contexts.
 * 
 * Sourced from: server/services/extraction/stealthBrowser.ts
 */

// 1. Initialize and register the stealth plugin once
chromium.use(StealthPlugin());

export async function launchStealthBrowser() {
    // 2. Launch chromium using vanilla playwright arguments
    const browser = await chromium.launch({
        headless: true,
        args: [
            '--disable-blink-features=AutomationControlled',
            '--no-sandbox',
            '--disable-setuid-sandbox',
        ],
    });

    // 3. Create a clean context with customized spoof user-agents
    const context = await browser.newContext({
        userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36',
        viewport: { width: 1280, height: 800 },
        deviceScaleFactor: 1,
        hasTouch: false,
    });

    return { browser, context };
}
