import { spawn, ChildProcess } from "node:child_process";
import * as path from "node:path";
import * as fs from "node:fs";
import * as net from "node:net";
import { logger } from "./logger";
import { safeFetch } from "./HttpClient";
import { detectSystemBrowser } from "./BrowserDetector";

/**
 * ChromeLauncher — Chrome Process Manager for CDP
 * ================================================
 * Equivalent to browser-harness's admin.py — handles Chrome lifecycle:
 *   1. Detect system Chrome/Edge (reuses PlaywrightBrowser logic)
 *   2. Launch with --remote-debugging-port (dynamic port allocation)
 *   3. Health check via /json/version endpoint
 *   4. Graceful shutdown + zombie process cleanup
 *   5. Isolated profile directory (separate from Playwright profiles)
 * 
 * Design Decisions:
 *   - Dynamic port allocation (avoids collision with GeminiSurfer's 9222)
 *   - Detached process (survives Gateway restart)
 *   - Periodic zombie cleanup (handles Chrome crash scenarios)
 *   - Profile isolation (protects Playwright RPA cookies)
 */

// ============================================================
// Singleton State
// ============================================================

let chromeProcess: ChildProcess | null = null;
let chromePort: number = 0;
let chromeWsUrl: string = "";
let chromeProfileDir: string = "";

// Chrome detection delegated to shared BrowserDetector utility

// ============================================================
// Dynamic Port Allocation
// ============================================================

/**
 * Find a free TCP port by binding to port 0 and reading the assigned port.
 */
async function findFreePort(): Promise<number> {
    return new Promise((resolve, reject) => {
        const server = net.createServer();
        server.listen(0, "127.0.0.1", () => {
            const address = server.address();
            if (address && typeof address === "object") {
                const port = address.port;
                server.close(() => resolve(port));
            } else {
                server.close(() => reject(new Error("Failed to get port")));
            }
        });
        server.on("error", reject);
    });
}

// ============================================================
// Health Check
// ============================================================

/**
 * Wait for Chrome's /json/version endpoint to respond.
 * Returns the WebSocket debugger URL.
 */
async function waitForChromeReady(port: number, maxWaitMs = 15_000): Promise<string> {
    const startTime = Date.now();
    const url = `http://127.0.0.1:${port}/json/version`;

    while (Date.now() - startTime < maxWaitMs) {
        try {
            const res = await safeFetch(url, {}, 3000);
            const data = await res.json();
            if (data.webSocketDebuggerUrl) {
                return data.webSocketDebuggerUrl as string;
            }
        } catch {
            // Chrome not ready yet, retry
        }
        await new Promise(r => setTimeout(r, 500));
    }

    throw new Error(`[ChromeLauncher] Chrome didn't start within ${maxWaitMs}ms on port ${port}`);
}

// ============================================================
// ChromeLauncher Public API
// ============================================================

export class ChromeLauncher {
    /**
     * Launch Chrome with CDP enabled or connect to existing instance.
     * Uses dynamic port allocation to avoid conflicts.
     * 
     * @returns WebSocket debug URL for CDPClient.connect()
     */
    static async launchOrConnect(requestedPort?: number): Promise<string> {
        // If already running with a valid connection, reuse it
        if (chromeWsUrl && chromePort > 0) {
            try {
                await safeFetch(`http://127.0.0.1:${chromePort}/json/version`, {}, 3000);
                logger.info(`[ChromeLauncher] Reusing existing Chrome on port ${chromePort}`);
                return chromeWsUrl;
            } catch {
                // Existing Chrome died, clean up and relaunch
                logger.warn("[ChromeLauncher] Existing Chrome is dead. Relaunching...");
                await this.shutdown();
            }
        }

        // Allocate port
        const port = requestedPort ?? await findFreePort();
        chromePort = port;

        // Create isolated profile directory
        chromeProfileDir = path.resolve(process.cwd(), "data", "cdp_chrome_profile");
        try {
            await fs.promises.access(chromeProfileDir);
        } catch {
            await fs.promises.mkdir(chromeProfileDir, { recursive: true });
        }

        // Detect Chrome binary
        const chromePath = await detectSystemBrowser();

        // Launch Chrome
        const args = [
            `--remote-debugging-port=${port}`,
            `--user-data-dir=${chromeProfileDir}`,
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--disable-blink-features=AutomationControlled",
            "--disable-background-timer-throttling",
            "--disable-backgrounding-occluded-windows",
            "--disable-renderer-backgrounding",
            // Stealth args
            "--disable-infobars",
            "--disable-dev-shm-usage",
        ];

        logger.info(`[ChromeLauncher] Launching Chrome on port ${port}...`);

        chromeProcess = spawn(chromePath, args, { // NOSONAR
            detached: true,
            stdio: "ignore",
            shell: false
        });
        chromeProcess.on('error', (err) => logger.error(err, "[ChromeLauncher] Chrome spawn error:"));
        chromeProcess.unref();

        // Handle process exit
        chromeProcess.on("exit", (code, signal) => {
            logger.warn(`[ChromeLauncher] Chrome process exited (code=${code}, signal=${signal})`);
            chromeProcess = null;
            chromeWsUrl = "";
        });

        // Wait for Chrome to be ready
        chromeWsUrl = await waitForChromeReady(port);
        logger.info(`[ChromeLauncher] ✅ Chrome ready on port ${port}`);
        logger.info(`[ChromeLauncher] Profile: ${chromeProfileDir}`);

        return chromeWsUrl;
    }

    /**
     * Get WebSocket URL for the first available tab.
     * Use this when you need to attach to a specific page target.
     */
    static async getFirstTabWsUrl(): Promise<string | null> {
        if (!chromePort) return null;

        try {
            const res = await safeFetch(`http://127.0.0.1:${chromePort}/json/list`, {}, 3000);
            const tabs: any[] = await res.json();
            const page = tabs.find(t => t.type === "page");
            return page?.webSocketDebuggerUrl ?? null;
        } catch {
            return null;
        }
    }

    /**
     * Create a new tab and return its target ID.
     */
    static async createNewTab(url = "about:blank"): Promise<{ targetId: string; wsUrl: string } | null> {
        if (!chromePort) return null;

        try {
            const res = await safeFetch(
                `http://127.0.0.1:${chromePort}/json/new?${encodeURIComponent(url)}`,
                { method: "PUT" },
                5000
            );
            const tab: any = await res.json();
            return {
                targetId: tab.id,
                wsUrl: tab.webSocketDebuggerUrl,
            };
        } catch (err: any) {
            logger.error(`[ChromeLauncher] Failed to create new tab: ${err.message}`);
            return null;
        }
    }

    /**
     * Close a specific tab by target ID.
     */
    static async closeTab(targetId: string): Promise<void> {
        if (!chromePort) return;

        try {
            await safeFetch(
                `http://127.0.0.1:${chromePort}/json/close/${targetId}`,
                {},
                3000
            );
        } catch { /* ignore */ }
    }

    /**
     * Graceful shutdown — kill Chrome process and clean up.
     */
    static async shutdown(): Promise<void> {
        if (chromeProcess) {
            try {
                // Try graceful close via CDP first
                if (chromePort) {
                    await safeFetch(
                        `http://127.0.0.1:${chromePort}/json/close`,
                        {},
                        2000
                    ).catch(() => {});
                }
            } catch { /* ignore */ }

            try {
                chromeProcess.kill("SIGTERM");
            } catch { /* ignore — process may already be dead */ }

            chromeProcess = null;
        }

        chromePort = 0;
        chromeWsUrl = "";
        logger.info("[ChromeLauncher] Chrome shutdown complete");
    }

    /**
     * Get the current Chrome debug port.
     */
    static get port(): number {
        return chromePort;
    }

    /**
     * Check if Chrome is currently running and responsive.
     */
    static async isAlive(): Promise<boolean> {
        if (!chromePort) return false;

        try {
            await safeFetch(`http://127.0.0.1:${chromePort}/json/version`, {}, 3000);
            return true;
        } catch {
            return false;
        }
    }

    /**
     * Zombie Process Cleanup — detect and kill orphaned Chrome processes
     * that may have survived a Gateway crash.
     * Call this periodically or at startup.
     */
    static async cleanupZombies(): Promise<number> {
        // Only on Windows
        if (process.platform !== "win32") return 0;

        try {
            const { execSync } = await import("child_process");
            // Find Chrome processes with our specific user-data-dir
            const profileMarker = "cdp_chrome_profile";
            const output = execSync( // NOSONAR
                `wmic process where "name='chrome.exe'" get ProcessId,CommandLine /format:csv`,
                { encoding: "utf-8", timeout: 5000 }
            ).trim();

            let killed = 0;
            const lines = output.split("\n").filter(l => l.includes(profileMarker));

            for (const line of lines) {
                const pidMatch = line.match(/,(\d+)$/);
                if (pidMatch) {
                    const pid = Number.parseInt(pidMatch[1], 10);
                    try {
                        process.kill(pid, "SIGTERM");
                        killed++;
                    } catch { /* process already dead */ }
                }
            }

            if (killed > 0) {
                logger.info(`[ChromeLauncher] 🧹 Cleaned up ${killed} zombie Chrome process(es)`);
            }
            return killed;
        } catch {
            return 0;
        }
    }
}
