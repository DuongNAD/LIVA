/**
 * VSCodeBridge — VS Code WebSocket Bridge (Phase 2)
 * ===================================================
 * Connects LIVA to VS Code via the "vscode-remote-control" extension.
 * Allows executing VS Code commands, opening files, typing text,
 * and interacting with the terminal remotely.
 *
 * Requirements:
 *   VS Code must have a remote-control extension running a WS server on port 3710.
 *
 * [v5.0] LIVA Remote Control Hub
 */

import { EventEmitter } from "node:events";
import { WebSocket } from "ws";
import { logger } from "../utils/logger";

// ===========================
// Protocol Types
// ===========================

export interface VSCodeRequest {
    id: number;
    command: string;
    args?: any[];
}

export interface VSCodeResponse {
    id: number;
    result?: any;
    error?: string;
}

export interface EditorContext {
    fileName: string;
    content: string;
    languageId: string;
    selection?: { startLine: number; endLine: number; text: string };
}

// ===========================
// VSCodeBridge
// ===========================

export class VSCodeBridge extends EventEmitter {
    #ws: WebSocket | null = null;
    #messageId = 0;
    #pendingRequests = new Map<number, { resolve: (val: any) => void; reject: (err: any) => void; timer: ReturnType<typeof setTimeout> }>();
    
    readonly #host: string;
    readonly #port: number;
    #autoReconnect = true;
    #reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    #reconnectBackoff = 2000;
    readonly #maxBackoff = 30_000;
    readonly #requestTimeout = 15_000;
    #isDisposed = false;

    constructor(host = "127.0.0.1", port = 3710) {
        super();
        this.#host = host;
        this.#port = port;
    }

    // ═══════════════════════════════════════
    //  Connection Lifecycle
    // ═══════════════════════════════════════

    public async connect(): Promise<void> {
/* istanbul ignore next */
        if (this.#isDisposed) return;
        if (this.isConnected()) return;

        const wsUrl = `ws://${this.#host}:${this.#port}`;

        return new Promise((resolve, reject) => {
            const ws = new WebSocket(wsUrl);

            const connectTimeout = setTimeout(() => {
                ws.close();
                reject(new Error("[VSCode] WebSocket connection timeout"));
            }, 10_000);

            ws.on("open", () => {
                clearTimeout(connectTimeout);
                this.#ws = ws;
                this.#reconnectBackoff = 2000;
                logger.info(`🔗 [VSCode] Bridge connected on port ${this.#port}`);
                this.emit("connected");
                resolve();
            });

            ws.on("message", (data) => {
                try {
                    const msg = JSON.parse(data.toString()) as VSCodeResponse;
                    this.#handleMessage(msg);
                } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e);
                    logger.error(`[VSCode] Parse error: ${errMsg}`);
                }
            });

            ws.on("close", () => {
                this.#cleanup();
                this.emit("disconnected");
                if (this.#autoReconnect && !this.#isDisposed) {
                    this.#scheduleReconnect();
                }
            });

            ws.on("error", (err) => {
                clearTimeout(connectTimeout);
                logger.error(`[VSCode] WebSocket error: ${err.message}`);
                reject(err);
            });
        });
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
    //  VS Code Commands
    // ═══════════════════════════════════════

    /**
     * Execute a VS Code workbench command
     * e.g. executeCommand("workbench.action.files.save")
     */
    public async executeCommand(command: string, args: any[] = []): Promise<any> {
        return this.#sendRequest("executeCommand", [command, ...args]);
    }

    /**
     * Open a file in the active editor
     */
    public async openFile(filePath: string): Promise<void> {
        await this.#sendRequest("openFile", [filePath]);
    }

    /**
     * Insert text at the current cursor position
     */
    public async insertText(text: string): Promise<void> {
        await this.#sendRequest("insertText", [text]);
    }

    /**
     * Get the active editor's context (content, filename, selection)
     */
    public async getActiveEditor(): Promise<EditorContext | null> {
        return this.#sendRequest("getActiveEditor", []);
    }

    /**
     * Open a terminal (or focus existing)
     */
    public async openTerminal(): Promise<void> {
        await this.executeCommand("workbench.action.terminal.toggleTerminal");
    }

    /**
     * Run a command in the active terminal
     */
    public async runTerminalCommand(commandText: string): Promise<void> {
        await this.#sendRequest("runTerminalCommand", [commandText]);
    }

    // ═══════════════════════════════════════
    //  Private Internals
    // ═══════════════════════════════════════

    async #sendRequest(command: string, args: any[]): Promise<any> {
        if (!this.isConnected()) {
            throw new Error("[VSCode] Not connected to VS Code IDE");
        }

        const id = ++this.#messageId;

        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                this.#pendingRequests.delete(id);
                reject(new Error(`[VSCode] Timeout executing ${command} (${this.#requestTimeout}ms)`));
            }, this.#requestTimeout);

            this.#pendingRequests.set(id, { resolve, reject, timer });

            const payload: VSCodeRequest = { id, command, args };
            this.#ws!.send(JSON.stringify(payload));
        });
    }

    #handleMessage(msg: VSCodeResponse): void {
/* istanbul ignore next */
        if (typeof msg.id === "number") {
            const pending = this.#pendingRequests.get(msg.id);
/* istanbul ignore next */
            if (pending) {
                clearTimeout(pending.timer);
                this.#pendingRequests.delete(msg.id);
                
                if (msg.error) {
                    pending.reject(new Error(`[VSCode] IDE Error: ${msg.error}`));
                } else {
                    pending.resolve(msg.result);
                }
            }
        }
    }

    #scheduleReconnect(): void {
/* istanbul ignore next */
        if (this.#reconnectTimer || this.#isDisposed) return;

        logger.info(`[VSCode] ♻️ Auto-reconnect in ${this.#reconnectBackoff}ms...`);
        this.#reconnectTimer = setTimeout(async () => {
            this.#reconnectTimer = null;
            try {
                await this.connect();
            } catch { /* connect() logs error */ }
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
            pending.reject(new Error("[VSCode] Connection closed"));
        }
        this.#pendingRequests.clear();

        if (this.#ws) {
            try { this.#ws.close(); } catch { /* ignore */ }
            this.#ws = null;
        }
    }
}
