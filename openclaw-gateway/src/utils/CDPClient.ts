import { WebSocket } from "ws";
import { logger } from "./logger";

/**
 * CDPClient — Chrome DevTools Protocol WebSocket Client
 * ======================================================
 * Thinnest possible bridge between LIVA Agent (E4B) and Chrome.
 * 
 * Design Philosophy (from browser-harness):
 *   - ONE WebSocket connection, no middleware, no framework
 *   - CDP is the API — no abstraction layers hiding protocol details
 *   - Async/Promise-based command dispatch with timeout protection
 *   - Auto-reconnect on connection drop (preserves Chrome session)
 * 
 * Architecture:
 *   ChromeLauncher → spawns Chrome with --remote-debugging-port
 *   CDPClient → connects via WebSocket to Chrome's /json/version endpoint
 *   CDPClient.send() → JSON-RPC over WebSocket → Chrome responds
 * 
 * Security:
 *   - Blocked CDP domains: Security.*, Fetch.* (prevent session hijacking)
 *   - All commands audited via RPAGuardrails
 *   - Timeout on every command (no hanging promises)
 */

// ============================================================
// Types
// ============================================================

export interface CDPResponse {
    id: number;
    result?: any;
    error?: { code: number; message: string; data?: string };
}

interface PendingCommand {
    resolve: (value: any) => void;
    reject: (reason: Error) => void;
    timer: ReturnType<typeof setTimeout>;
    method: string;
}

export interface CDPEventHandler {
    (params: any): void;
}

// CDP domains that E4B is NEVER allowed to call (security hardening)
const BLOCKED_CDP_DOMAINS = new Set([
    "Security",       // Certificate manipulation
    "Fetch",          // Request interception (MITM potential)
    "SystemInfo",     // Hardware fingerprinting
    "Browser.close",  // Kill user's Chrome
    "Browser.crash",  // Force crash
]);

// ============================================================
// CDPClient
// ============================================================

export class CDPClient {
    #ws: WebSocket | null = null;
    #commandId = 0;
    #pending: Map<number, PendingCommand> = new Map();
    #eventHandlers: Map<string, CDPEventHandler[]> = new Map();
    #debugUrl: string = "";
    #sessionId: string | null = null;
    #connected = false;
    #disposed = false;

    // Reconnection state
    #reconnectAttempts = 0;
    #maxReconnectAttempts = 5;
    #reconnectDelayMs = 1000;
    #reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    // Command timeout
    #defaultTimeoutMs = 30_000;

    /**
     * Connect to Chrome DevTools via WebSocket.
     * @param debugUrl - WebSocket URL from Chrome's /json/version endpoint
     */
    async connect(debugUrl: string): Promise<void> {
        if (this.#disposed) throw new Error("[CDPClient] Client has been disposed");
        if (this.#connected && this.#ws?.readyState === WebSocket.OPEN) {
            logger.info("[CDPClient] Already connected, reusing existing connection");
            return;
        }

        this.#debugUrl = debugUrl;

        return new Promise<void>((resolve, reject) => {
            const ws = new WebSocket(debugUrl, {
                perMessageDeflate: false, // Disable compression for speed
                maxPayload: 64 * 1024 * 1024, // 64MB max (for large screenshots)
            });

            const connectTimeout = setTimeout(() => {
                ws.close();
                reject(new Error(`[CDPClient] Connection timeout after 10s to ${debugUrl}`));
            }, 10_000);

            ws.on("open", () => {
                clearTimeout(connectTimeout);
                this.#ws = ws;
                this.#connected = true;
                this.#reconnectAttempts = 0;
                logger.info(`[CDPClient] ✅ Connected to Chrome DevTools: ${debugUrl.substring(0, 60)}...`);
                resolve();
            });

            ws.on("message", (data: Buffer) => {
                this.#handleMessage(data);
            });

            ws.on("close", (code: number, _reason: Buffer) => {
                this.#connected = false;
                logger.warn(`[CDPClient] WebSocket closed (code=${code})`);
                this.#rejectAllPending(new Error(`WebSocket closed: code=${code}`));

                // Auto-reconnect if not intentionally disposed
                if (!this.#disposed) {
                    this.#scheduleReconnect();
                }
            });

            ws.on("error", (err: Error) => {
                clearTimeout(connectTimeout);
                if (!this.#connected) {
                    reject(new Error(`[CDPClient] WebSocket error: ${err.message}`));
                } else {
                    logger.error(`[CDPClient] WebSocket error: ${err.message}`);
                }
            });
        });
    }

    /**
     * Send a CDP command and wait for the response.
     * Every command has a timeout to prevent hanging promises.
     * 
     * @example
     * // Navigate to a page
     * await cdp.send("Page.navigate", { url: "https://google.com" });
     * 
     * // Get page title via JavaScript evaluation
     * const result = await cdp.send("Runtime.evaluate", { expression: "document.title" });
     */
    async send(method: string, params: Record<string, any> = {}, timeoutMs?: number): Promise<any> {
        if (!this.#connected || !this.#ws) {
            throw new Error(`[CDPClient] Not connected. Call connect() first.`);
        }

        // Security: Block dangerous CDP domains
        const domain = method.split(".")[0];
        if (BLOCKED_CDP_DOMAINS.has(domain) || BLOCKED_CDP_DOMAINS.has(method)) {
            throw new Error(`[CDPClient] 🛡️ SECURITY BLOCK: CDP domain "${method}" is forbidden`);
        }

        const id = ++this.#commandId;
        const timeout = timeoutMs ?? this.#defaultTimeoutMs;

        const message: any = { id, method, params };
        if (this.#sessionId) {
            message.sessionId = this.#sessionId;
        }

        return new Promise<any>((resolve, reject) => {
            const timer = setTimeout(() => {
                this.#pending.delete(id);
                reject(new Error(`[CDPClient] Command timeout after ${timeout}ms: ${method}`));
            }, timeout);

            this.#pending.set(id, { resolve, reject, timer, method });

            try {
                this.#ws!.send(JSON.stringify(message));
            } catch (err: any) {
                clearTimeout(timer);
                this.#pending.delete(id);
                reject(new Error(`[CDPClient] Failed to send command: ${err.message}`));
            }
        });
    }

    /**
     * Subscribe to CDP events.
     * 
     * @example
     * cdp.on("Page.loadEventFired", (params) => {
     *     console.log("Page fully loaded!");
     * });
     */
    on(event: string, handler: CDPEventHandler): void {
        const handlers = this.#eventHandlers.get(event) ?? [];
        handlers.push(handler);
        this.#eventHandlers.set(event, handlers);
    }

    /**
     * Remove event handler.
     */
    off(event: string, handler: CDPEventHandler): void {
        const handlers = this.#eventHandlers.get(event);
        if (handlers) {
            const idx = handlers.indexOf(handler);
            if (idx !== -1) handlers.splice(idx, 1);
        }
    }

    /**
     * Attach to a specific browser target (tab).
     * Required before sending page-level commands.
     */
    async attachToTarget(targetId: string): Promise<void> {
        const result = await this.send("Target.attachToTarget", {
            targetId,
            flatten: true,
        });
        this.#sessionId = result.sessionId;
        logger.info(`[CDPClient] Attached to target: ${targetId} (session: ${this.#sessionId})`);
    }

    /**
     * Enable common CDP domains needed for browser automation.
     */
    async enableDomains(): Promise<void> {
        await Promise.all([
            this.send("Page.enable"),
            this.send("Runtime.enable"),
            this.send("DOM.enable"),
            this.send("Accessibility.enable"),
            this.send("Network.enable"),
        ]);
        logger.info("[CDPClient] Enabled CDP domains: Page, Runtime, DOM, Accessibility, Network");
    }

    /**
     * Navigate to URL and wait for load.
     */
    async navigateTo(url: string, timeoutMs = 30_000): Promise<{ frameId: string; loaderId: string }> {
        const result = await this.send("Page.navigate", { url }, timeoutMs);

        if (result.errorText) {
            throw new Error(`[CDPClient] Navigation error: ${result.errorText}`);
        }

        // Wait for load event
        await new Promise<void>((resolve, reject) => {
            const timer = setTimeout(() => {
                this.off("Page.loadEventFired", handler);
                reject(new Error(`[CDPClient] Page load timeout after ${timeoutMs}ms`));
            }, timeoutMs);

            const handler = () => {
                clearTimeout(timer);
                this.off("Page.loadEventFired", handler);
                resolve();
            };
            this.on("Page.loadEventFired", handler);
        });

        return result;
    }

    /**
     * Get the full Accessibility Tree from Chrome.
     * This is the key method that replaces Raw DOM with semantic data.
     */
    async getAccessibilityTree(): Promise<any> {
        return await this.send("Accessibility.getFullAXTree", {}, 15_000);
    }

    /**
     * Dispatch a mouse click at coordinates.
     */
    async dispatchClick(x: number, y: number): Promise<void> {
        await this.send("Input.dispatchMouseEvent", {
            type: "mousePressed",
            x, y,
            button: "left",
            clickCount: 1,
        });
        await this.send("Input.dispatchMouseEvent", {
            type: "mouseReleased",
            x, y,
            button: "left",
            clickCount: 1,
        });
    }

    /**
     * Type text character by character (simulates real keyboard input).
     */
    async dispatchType(text: string): Promise<void> {
        for (const char of text) {
            await this.send("Input.dispatchKeyEvent", {
                type: "keyDown",
                text: char,
            });
            await this.send("Input.dispatchKeyEvent", {
                type: "keyUp",
                text: char,
            });
        }
    }

    /**
     * Insert text at once (faster than character-by-character).
     */
    async insertText(text: string): Promise<void> {
        await this.send("Input.insertText", { text });
    }

    /**
     * Capture a screenshot as base64-encoded PNG.
     */
    async screenshot(format: "png" | "jpeg" = "png", quality?: number): Promise<string> {
        const params: any = { format };
        if (quality !== undefined) params.quality = quality;
        const result = await this.send("Page.captureScreenshot", params, 10_000);
        return result.data; // base64 string
    }

    /**
     * Evaluate JavaScript in the page context.
     */
    async evaluate(expression: string): Promise<any> {
        const result = await this.send("Runtime.evaluate", {
            expression,
            returnByValue: true,
            awaitPromise: true,
        });

        if (result.exceptionDetails) {
            throw new Error(`[CDPClient] JS eval error: ${result.exceptionDetails.text}`);
        }

        return result.result?.value;
    }

    /**
     * Scroll the page.
     */
    async scrollPage(deltaY: number): Promise<void> {
        await this.send("Input.dispatchMouseEvent", {
            type: "mouseWheel",
            x: 400,
            y: 400,
            deltaX: 0,
            deltaY,
        });
    }

    /**
     * Get current page URL.
     */
    async getCurrentUrl(): Promise<string> {
        return await this.evaluate("window.location.href");
    }

    /**
     * Get page title.
     */
    async getPageTitle(): Promise<string> {
        return await this.evaluate("document.title");
    }

    /**
     * Check if connection is active.
     */
    get isConnected(): boolean {
        return this.#connected && this.#ws?.readyState === WebSocket.OPEN;
    }

    /**
     * Graceful shutdown — clean up all resources.
     */
    dispose(): void {
        this.#disposed = true;

        if (this.#reconnectTimer) {
            clearTimeout(this.#reconnectTimer);
            this.#reconnectTimer = null;
        }

        this.#rejectAllPending(new Error("[CDPClient] Client disposed"));
        this.#eventHandlers.clear();

        if (this.#ws) {
            try {
                this.#ws.close();
            } catch { /* ignore close errors */ }
            this.#ws = null;
        }

        this.#connected = false;
        logger.info("[CDPClient] Disposed — all resources released");
    }

    // ============================================================
    // Private Methods
    // ============================================================

    #handleMessage(data: Buffer): void {
        let msg: any;
        try {
            msg = JSON.parse(data.toString("utf-8"));
        } catch {
            logger.warn("[CDPClient] Failed to parse CDP message");
            return;
        }

        // Handle command responses
        if (msg.id !== undefined) {
            const pending = this.#pending.get(msg.id);
            if (pending) {
                clearTimeout(pending.timer);
                this.#pending.delete(msg.id);

                if (msg.error) {
                    pending.reject(new Error(`[CDP ${pending.method}] ${msg.error.message}`));
                } else {
                    pending.resolve(msg.result ?? {});
                }
            }
            return;
        }

        // Handle events
        if (msg.method) {
            const handlers = this.#eventHandlers.get(msg.method);
            if (handlers) {
                for (const handler of handlers) {
                    try {
                        handler(msg.params ?? {});
                    } catch (err: any) {
                        logger.error(`[CDPClient] Event handler error for ${msg.method}: ${err.message}`);
                    }
                }
            }
        }
    }

    #rejectAllPending(error: Error): void {
        for (const [_id, pending] of this.#pending) {
            clearTimeout(pending.timer);
            pending.reject(error);
        }
        this.#pending.clear();
    }

    #scheduleReconnect(): void {
        if (this.#disposed || this.#reconnectAttempts >= this.#maxReconnectAttempts) {
            if (this.#reconnectAttempts >= this.#maxReconnectAttempts) {
                logger.error(`[CDPClient] Max reconnect attempts (${this.#maxReconnectAttempts}) reached. Giving up.`);
            }
            return;
        }

        this.#reconnectAttempts++;
        const delay = this.#reconnectDelayMs * Math.pow(2, this.#reconnectAttempts - 1); // Exponential backoff

        logger.info(`[CDPClient] Reconnecting in ${delay}ms (attempt ${this.#reconnectAttempts}/${this.#maxReconnectAttempts})...`);

        this.#reconnectTimer = setTimeout(() => {
            void (async () => {
                try {
                    await this.connect(this.#debugUrl);
                    // Re-enable domains after reconnect
                    if (this.#sessionId) {
                        await this.enableDomains();
                    }
                } catch (err: any) {
                    logger.warn(`[CDPClient] Reconnect attempt ${this.#reconnectAttempts} failed: ${err.message}`);
                }
            })();
        }, delay);
    }
}
