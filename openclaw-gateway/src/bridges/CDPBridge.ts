/**
 * CDPBridge — Chrome DevTools Protocol Connector (Phase 1)
 * =========================================================
 * Connects LIVA to Google Antigravity IDE via Chrome DevTools Protocol.
 * Enables:
 *   - DOM inspection and manipulation
 *   - JavaScript evaluation in IDE context
 *   - Screenshot capture
 *   - MutationObserver for approval button detection
 *   - Auto-reconnect with exponential backoff
 *
 * Requirements:
 *   Launch Antigravity with: antigravity --remote-debugging-port=9222
 *
 * [v5.0] LIVA Remote Control Hub
 */

import { EventEmitter } from "node:events";
import { WebSocket } from "ws";
import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";
import { CDPUILocators } from "./CDPUILocators";

// ===========================
// CDP Types
// ===========================

interface CDPResponse {
    id: number;
    result?: any;
    error?: { code: number; message: string };
}

interface CDPEvent {
    method: string;
    params: any;
}

interface CDPTarget {
    id: string;
    title: string;
    type: string;
    url: string;
    webSocketDebuggerUrl: string;
}

// ===========================
// CDPBridge
// ===========================

export class CDPBridge extends EventEmitter {
    #ws: WebSocket | null = null;
    #messageId = 0;
    #pendingRequests = new Map<number, { resolve: (value: any) => void; reject: (reason: any) => void; timer: ReturnType<typeof setTimeout> }>();
    #host: string;
    #port: number;
    #autoReconnect: boolean;
    #reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    #reconnectBackoff = 2000;
    readonly #maxBackoff = 30_000;
    readonly #requestTimeout = 10_000;
    #isDisposed = false;

    constructor(host = "127.0.0.1", port = 9222) {
        super();
        this.#host = host;
        this.#port = port;
        this.#autoReconnect = true;
    }

    // ═══════════════════════════════════════
    //  Connection Lifecycle
    // ═══════════════════════════════════════

    /**
     * Connect to Chrome DevTools Protocol.
     * Discovers available targets and connects to the first page.
     */
    public async connect(): Promise<void> {
        if (this.#isDisposed) return;

        try {
            // Step 1: Discover targets via HTTP
            const targets = await this.#discoverTargets();
            const pageTarget = targets.find(t => t.type === "page");

            if (!pageTarget?.webSocketDebuggerUrl) {
                throw new Error("No debuggable page found. Is Antigravity running with --remote-debugging-port?");
            }

            // Step 2: Connect via WebSocket
            await this.#connectWebSocket(pageTarget.webSocketDebuggerUrl);
            this.#reconnectBackoff = 2000; // Reset backoff on success

            // Enable necessary domains
            await this.send("Runtime.enable");
            await this.send("Network.enable");

            logger.info(`🔗 [CDP] Connected to: ${pageTarget.title}`);
            this.emit("connected", pageTarget);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[CDP] Connection failed: ${errMsg}`);
            this.#scheduleReconnect();
            throw e;
        }
    }

    public disconnect(): void {
        this.#autoReconnect = false;
        this.#cleanup();
    }

    public dispose(): void {
        this.#isDisposed = true;
        this.disconnect();
    }

    public isConnected(): boolean {
        return this.#ws !== null && this.#ws.readyState === WebSocket.OPEN;
    }

    // ═══════════════════════════════════════
    //  CDP Command Execution
    // ═══════════════════════════════════════

    /**
     * Send a CDP command and wait for response.
     */
    public async send(method: string, params: Record<string, any> = {}): Promise<any> {
        if (!this.isConnected()) {
            throw new Error("[CDP] Not connected");
        }

        const id = ++this.#messageId;

        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                this.#pendingRequests.delete(id);
                reject(new Error(`[CDP] Timeout: ${method} (${this.#requestTimeout}ms)`));
            }, this.#requestTimeout);

            this.#pendingRequests.set(id, { resolve, reject, timer });

            this.#ws!.send(JSON.stringify({ id, method, params }));
        });
    }

    /**
     * Evaluate JavaScript in the IDE's page context.
     */
    public async evaluateJS(expression: string): Promise<any> {
        const result = await this.send("Runtime.evaluate", {
            expression,
            returnByValue: true,
            awaitPromise: true,
        });

        if (result.exceptionDetails) {
            throw new Error(`[CDP] JS Error: ${result.exceptionDetails.text}`);
        }

        return result.result?.value;
    }

    /**
     * Query a DOM element by CSS selector.
     */
    public async querySelector(selector: string): Promise<number | null> {
        const doc = await this.send("DOM.getDocument");
        try {
            const result = await this.send("DOM.querySelector", {
                nodeId: doc.root.nodeId,
                selector,
            });
            return result.nodeId > 0 ? result.nodeId : null;
        } catch {
            return null;
        }
    }

    /**
     * Click a DOM element by selector (simulates mouse events).
     */
    public async clickElement(selector: string): Promise<void> {
        // Get element position via JS
        const box = await this.evaluateJS(`
            (() => {
                const el = document.querySelector('${selector.replace(/'/g, "\\'")}');
                if (!el) return null;
                const rect = el.getBoundingClientRect();
                return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
            })()
        `);

        if (!box) throw new Error(`[CDP] Element not found: ${selector}`);

        // Dispatch mouse events
        await this.send("Input.dispatchMouseEvent", { type: "mousePressed", x: box.x, y: box.y, button: "left", clickCount: 1 });
        await this.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: box.x, y: box.y, button: "left", clickCount: 1 });
    }

    /**
     * Type text into the focused element using native CDP Input.
     */
    public async typeText(text: string): Promise<void> {
        // Native insertText is faster and more robust than looping char by char
        await this.send("Input.insertText", { text });
    }

    /**
     * Dispatch a specific key event (e.g. Enter)
     */
    public async pressKey(key: string, code?: string): Promise<void> {
        await this.send("Input.dispatchKeyEvent", {
            type: "keyDown",
            key: key,
            code: code || key,
            text: key.length === 1 ? key : undefined,
        });
        await this.send("Input.dispatchKeyEvent", {
            type: "keyUp",
            key: key,
            code: code || key,
        });
    }

    /**
     * Capture a screenshot of the IDE.
     */
    public async captureScreenshot(): Promise<Buffer> {
        const result = await this.send("Page.captureScreenshot", { format: "png" });
        return Buffer.from(result.data, "base64");
    }

    /**
     * Get the current page title.
     */
    public async getPageTitle(): Promise<string> {
        return this.evaluateJS("document.title");
    }

    // ═══════════════════════════════════════
    //  MutationObserver — Approval Detection
    // ═══════════════════════════════════════

    /**
     * Inject a MutationObserver to watch for approval buttons
     * (Run, Allow, Accept, Approve) appearing in the IDE DOM.
     */
    public async watchForApprovalButtons(): Promise<void> {
        await this.send("Runtime.enable");
        await this.send("Page.enable");

        const scriptSource = `
            (() => {
                if (window.__livaObserver) return; // Already installed

                const BUTTON_PATTERNS = ${CDPUILocators.approvalTextPatterns.toString()};
                const LOCATOR = '${CDPUILocators.approvalButtons}';

                function extractCommandText(btn) {
                    try {
                        let el = btn;
                        for (let i = 0; i < 8 && el && el !== document.body; i++) {
                            el = el.parentElement; 
                            if (!el) break;
                            const codeBlock = el.querySelector('code, pre, .terminal, .code-block, [class*="code"]');
                            if (codeBlock) {
                                return (codeBlock.innerText || codeBlock.textContent || '').trim();
                            }
                            const messageRow = el.closest('.agent-message, .message-row, [data-message-id]');
                            if (messageRow) {
                                const codes = messageRow.querySelectorAll('code');
                                if (codes.length > 0) return (codes[codes.length - 1].textContent || '').trim();
                            }
                        }
                    } catch (e) {}
                    return "UNKNOWN_COMMAND";
                }

                const observer = new MutationObserver((mutations) => {
                    for (const mutation of mutations) {
                        for (const node of mutation.addedNodes) {
                            if (node.nodeType !== 1) continue;
                            const el = node;

                            // Check buttons/links
                            const buttons = el.querySelectorAll ? 
                                [el, ...el.querySelectorAll(LOCATOR)] : [el];

                            for (const btn of buttons) {
                                const text = (btn.textContent || '').trim();
                                if (BUTTON_PATTERNS.test(text)) {
                                    const command = extractCommandText(btn);

                                    // Signal LIVA via browser console (this runs in Chrome, not Node.js)
                                    // eslint-disable-next-line no-console
                                    console.log('__LIVA_APPROVAL_DETECTED__:' + JSON.stringify({
                                        text,
                                        command,
                                        tagName: btn.tagName,
                                        selector: btn.id ? '#' + btn.id : btn.className ? '.' + btn.className.split(' ')[0] : btn.tagName.toLowerCase()
                                    }));
                                }
                            }
                        }
                    }
                });

                observer.observe(document.body, {
                    childList: true,
                    subtree: true
                });

                window.__livaObserver = observer;
            })();
        `;

        // Inject for all future reloads
        await this.send("Page.addScriptToEvaluateOnNewDocument", { source: scriptSource });
        
        // Inject into current page immediately
        await this.evaluateJS(scriptSource);

        // Listen for console messages from the observer
        logger.info("[CDP] 🔍 MutationObserver installed — watching for approval buttons (Immortal).");
    }

    /**
     * Click an approval button (approve or reject).
     */
    public async clickApprovalButton(approve: boolean): Promise<void> {
/* istanbul ignore next */
        const patternStr = approve ? CDPUILocators.approvalTextPatterns.source : CDPUILocators.rejectTextPatterns.source;
        const flags = approve ? CDPUILocators.approvalTextPatterns.flags : CDPUILocators.rejectTextPatterns.flags;

        await this.evaluateJS(`
            (() => {
                const buttons = document.querySelectorAll('${CDPUILocators.approvalButtons}');
                const regex = new RegExp('${patternStr}', '${flags}');
                for (const btn of buttons) {
                    const text = (btn.textContent || '').trim();
                    if (regex.test(text)) {
                        btn.click();
                        return true;
                    }
                }
                return false;
            })()
        `);
    }

    // ═══════════════════════════════════════
    //  Private Internals
    // ═══════════════════════════════════════

    async #discoverTargets(): Promise<CDPTarget[]> {
        const res = await safeFetch(`http://${this.#host}:${this.#port}/json`, {}, 5000);
        return res.json() as Promise<CDPTarget[]>;
    }

    #connectWebSocket(wsUrl: string): Promise<void> {
        return new Promise((resolve, reject) => {
            const ws = new WebSocket(wsUrl);

            const connectTimeout = setTimeout(() => {
                ws.close();
                reject(new Error("[CDP] WebSocket connection timeout"));
            }, 10_000);

            ws.on("open", () => {
                clearTimeout(connectTimeout);
                this.#ws = ws;
                resolve();
            });

            ws.on("message", (data) => {
                try {
                    const msg = JSON.parse(data.toString());
                    this.#handleMessage(msg);
                } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e);
                    logger.error(`[CDP] Parse error: ${errMsg}`);
                }
            });

            ws.on("close", () => {
                this.#cleanup();
                this.emit("disconnected");
/* istanbul ignore next */
                if (this.#autoReconnect && !this.#isDisposed) {
                    this.#scheduleReconnect();
                }
            });

            ws.on("error", (err) => {
                clearTimeout(connectTimeout);
                logger.error(`[CDP] WebSocket error: ${err.message}`);
                reject(err);
            });
        });
    }

    #handleMessage(msg: CDPResponse | CDPEvent): void {
        // Response to a sent command
        if ("id" in msg && typeof msg.id === "number") {
            const pending = this.#pendingRequests.get(msg.id);
/* istanbul ignore next */
            if (pending) {
                clearTimeout(pending.timer);
                this.#pendingRequests.delete(msg.id);
                if (msg.error) {
                    pending.reject(new Error(`[CDP] ${msg.error.message}`));
                } else {
                    pending.resolve(msg.result);
                }
            }
        }

        // CDP Event (e.g., Runtime.consoleAPICalled)
        if ("method" in msg) {
            this.emit("cdp_event", msg);

            // Network Sniffing
            if (msg.method === "Network.webSocketFrameReceived" || msg.method === "Network.responseReceived" || msg.method === "Network.requestWillBeSent") {
                this.emit("network_event", msg);
            }

            // Detect approval button signals from MutationObserver
/* istanbul ignore next */
            if (msg.method === "Runtime.consoleAPICalled") {
/* istanbul ignore next */
                const text = msg.params?.args?.[0]?.value || "";
/* istanbul ignore next */
                if (typeof text === "string" && text.startsWith("__LIVA_APPROVAL_DETECTED__:")) {
                    try {
                        const payload = JSON.parse(text.replace("__LIVA_APPROVAL_DETECTED__:", ""));
                        logger.info(`[CDP] 🔔 Approval button detected: "${payload.text}"`);
                        this.emit("approval_required", payload);
                    } catch { /* ignore parse errors */ }
                }
            }
        }
    }

    #scheduleReconnect(): void {
/* istanbul ignore next */
        if (this.#reconnectTimer || this.#isDisposed) return;

        logger.info(`[CDP] ♻️ Auto-reconnect in ${this.#reconnectBackoff}ms...`);
        this.#reconnectTimer = setTimeout(async () => {
            this.#reconnectTimer = null;
            try {
                await this.connect();
            } catch { /* connect() handles error logging */ }
        }, this.#reconnectBackoff);

        this.#reconnectBackoff = Math.min(this.#reconnectBackoff * 2, this.#maxBackoff);
    }

    #cleanup(): void {
        if (this.#reconnectTimer) {
            clearTimeout(this.#reconnectTimer);
            this.#reconnectTimer = null;
        }

        // Reject all pending requests
        for (const [id, pending] of this.#pendingRequests) {
            clearTimeout(pending.timer);
            pending.reject(new Error("[CDP] Connection closed"));
        }
        this.#pendingRequests.clear();

        if (this.#ws) {
            try { this.#ws.close(); } catch { /* ignore */ }
            this.#ws = null;
        }
    }
}
