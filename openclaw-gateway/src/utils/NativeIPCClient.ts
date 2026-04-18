/**
 * NativeIPCClient
 * ================
 * Drop-in replacement for OpenAI SDK that communicates via JSONL-over-TCP
 * with the LIVA Native Inference Engine (port 8100).
 * 
 * Eliminates ALL HTTP/REST overhead while maintaining API compatibility:
 *   - livaEngine.chat.completions.create({...}) works identically
 *   - Streaming is supported via AsyncIterator
 *   - Non-streaming returns the same shape as OpenAI SDK responses
 * 
 * Protocol: Raw JSONL (newline-delimited JSON) over TCP socket.
 * No HTTP headers, no REST parsing, no content-length negotiation.
 */

import * as net from "net";
import { logger } from "../utils/logger";
import { EventEmitter } from "events";

const IPC_HOST = "127.0.0.1";
const IPC_PORT = 8100;
const CONNECTION_TIMEOUT_MS = 30_000;
const REQUEST_TIMEOUT_MS = 120_000;

interface ChatMessage {
    role: string;
    content: string;
}

interface ChatCompletionRequest {
    model?: string;
    messages: ChatMessage[];
    temperature?: number;
    max_tokens?: number;
    stream?: boolean;
    [key: string]: any;
}

interface ChatCompletionChoice {
    index: number;
    message: { role: string; content: string };
    finish_reason: string;
}

interface ChatCompletionResponse {
    id: string;
    object: string;
    choices: ChatCompletionChoice[];
    model: string;
    usage?: { prompt_tokens: number; completion_tokens: number; total_tokens: number };
}

// Streaming chunk type compatible with OpenAI SDK's stream
interface ChatCompletionChunk {
    id: string;
    object: string;
    choices: Array<{
        index: number;
        delta: { role?: string; content?: string };
        finish_reason: string | null;
    }>;
}

/**
 * Persistent TCP connection pool to the Native Engine.
 * Reuses connections to avoid TCP handshake overhead per request.
 */
class IPCConnection {
    private socket: net.Socket | null = null;
    private connected = false;
    private pendingResolvers: Map<string, {
        onToken: (chunk: any) => void;
        onDone: (result: any) => void;
        onError: (err: Error) => void;
    }> = new Map();
    private lineBuffer = "";
    private requestCounter = 0;
    private reconnecting = false;

    async connect(): Promise<void> {
        if (this.connected && this.socket) return;

        return new Promise((resolve, reject) => {
            const timeout = setTimeout(() => {
                reject(new Error(`[NativeIPC] Connection timeout after ${CONNECTION_TIMEOUT_MS}ms`));
            }, CONNECTION_TIMEOUT_MS);

            this.socket = new net.Socket();
            this.socket.setEncoding("utf-8");
            this.socket.setNoDelay(true); // Disable Nagle's for minimal latency

            this.socket.connect(IPC_PORT, IPC_HOST, () => {
                clearTimeout(timeout);
                this.connected = true;
                this.lineBuffer = "";
                logger.info(`[NativeIPC] Connected to engine at ${IPC_HOST}:${IPC_PORT}`);
                resolve();
            });

            this.socket.on("data", (data: string) => {
                this.lineBuffer += data;
                this.processLines();
            });

            this.socket.on("error", (err) => {
                if (!this.connected) {
                    clearTimeout(timeout);
                    reject(err);
                } else {
                    logger.warn(`[NativeIPC] Socket error: ${err.message}`);
                    this.handleDisconnect();
                }
            });

            this.socket.on("close", () => {
                this.handleDisconnect();
            });
        });
    }

    private handleDisconnect() {
        this.connected = false;
        this.socket = null;
        // Reject all pending requests
        for (const [id, resolver] of this.pendingResolvers) {
            resolver.onError(new Error("[NativeIPC] Connection lost during request"));
        }
        this.pendingResolvers.clear();
    }

    private processLines() {
        const lines = this.lineBuffer.split("\n");
        // Keep incomplete last line in buffer
        this.lineBuffer = lines.pop() || "";

        for (const line of lines) {
            const trimmed = line.trim();
            if (!trimmed) continue;

            try {
                const msg = JSON.parse(trimmed);
                const reqId = msg.id;

                if (!reqId || !this.pendingResolvers.has(reqId)) {
                    continue;
                }

                const resolver = this.pendingResolvers.get(reqId)!;

                if (msg.type === "token") {
                    resolver.onToken(msg);
                } else if (msg.type === "done") {
                    resolver.onDone(msg);
                    this.pendingResolvers.delete(reqId);
                } else if (msg.type === "result") {
                    resolver.onDone(msg);
                    this.pendingResolvers.delete(reqId);
                } else if (msg.error) {
                    resolver.onError(new Error(msg.error));
                    this.pendingResolvers.delete(reqId);
                }
            } catch (e) {
                // Ignore malformed lines
            }
        }
    }

    async sendRequest(method: string, params: any): Promise<{ id: string; promise: Promise<any>; onToken?: (cb: (chunk: any) => void) => void }> {
        await this.connect();

        const reqId = `r${++this.requestCounter}_${Date.now()}`;
        const tokenListeners: Array<(chunk: any) => void> = [];

        const promise = new Promise<any>((resolve, reject) => {
            const timeout = setTimeout(() => {
                this.pendingResolvers.delete(reqId);
                reject(new Error(`[NativeIPC] Request timeout after ${REQUEST_TIMEOUT_MS}ms`));
            }, REQUEST_TIMEOUT_MS);

            this.pendingResolvers.set(reqId, {
                onToken: (chunk) => {
                    tokenListeners.forEach(cb => cb(chunk));
                },
                onDone: (result) => {
                    clearTimeout(timeout);
                    resolve(result);
                },
                onError: (err) => {
                    clearTimeout(timeout);
                    reject(err);
                },
            });
        });

        const json = JSON.stringify({ id: reqId, method, params }) + "\n";
        this.socket!.write(json);

        return {
            id: reqId,
            promise,
            onToken: (cb) => { tokenListeners.push(cb); },
        };
    }

    async healthCheck(): Promise<boolean> {
        try {
            const req = await this.sendRequest("health", {});
            const result = await req.promise;
            return result.status === "ok";
        } catch {
            return false;
        }
    }

    destroy() {
        if (this.socket) {
            this.socket.destroy();
            this.socket = null;
            this.connected = false;
        }
    }
}

// Singleton connection
const ipcConnection = new IPCConnection();

/**
 * Async iterator for streaming responses.
 * Mimics OpenAI SDK's Stream<ChatCompletionChunk> interface.
 */
class IPCStream implements AsyncIterable<ChatCompletionChunk> {
    private chunks: ChatCompletionChunk[] = [];
    private resolveNext: ((value: IteratorResult<ChatCompletionChunk>) => void) | null = null;
    private done = false;
    private error: Error | null = null;

    pushChunk(chunk: ChatCompletionChunk) {
        if (this.resolveNext) {
            this.resolveNext({ value: chunk, done: false });
            this.resolveNext = null;
        } else {
            this.chunks.push(chunk);
        }
    }

    finish() {
        this.done = true;
        if (this.resolveNext) {
            this.resolveNext({ value: undefined as any, done: true });
            this.resolveNext = null;
        }
    }

    setError(err: Error) {
        this.error = err;
        if (this.resolveNext) {
            this.resolveNext({ value: undefined as any, done: true });
            this.resolveNext = null;
        }
    }

    [Symbol.asyncIterator](): AsyncIterator<ChatCompletionChunk> {
        return {
            next: (): Promise<IteratorResult<ChatCompletionChunk>> => {
                if (this.error) {
                    return Promise.reject(this.error);
                }
                if (this.chunks.length > 0) {
                    return Promise.resolve({ value: this.chunks.shift()!, done: false });
                }
                if (this.done) {
                    return Promise.resolve({ value: undefined as any, done: true });
                }
                return new Promise(resolve => {
                    this.resolveNext = resolve;
                });
            }
        };
    }
}

/**
 * NativeIPCClient
 * Drop-in replacement for `new OpenAI(...)` that uses JSONL-over-TCP IPC.
 * 
 * Usage:
 *   const client = new NativeIPCClient();
 *   const result = await client.chat.completions.create({ messages, max_tokens });
 */
export class NativeIPCClient {
    public readonly chat: {
        completions: {
            create: (params: ChatCompletionRequest) => Promise<ChatCompletionResponse | IPCStream>;
        };
    };

    constructor() {
        this.chat = {
            completions: {
                create: async (params: ChatCompletionRequest) => {
                    return this._createCompletion(params);
                }
            }
        };
    }

    private async _createCompletion(params: ChatCompletionRequest): Promise<any> {
        const isStreaming = params.stream === true;

        if (isStreaming) {
            return this._createStreamingCompletion(params);
        } else {
            return this._createNonStreamingCompletion(params);
        }
    }

    private async _createNonStreamingCompletion(params: ChatCompletionRequest): Promise<ChatCompletionResponse> {
        const req = await ipcConnection.sendRequest("generate", {
            messages: params.messages,
            max_tokens: params.max_tokens || 512,
            stream: false,
        });

        const result = await req.promise;

        // Transform to OpenAI SDK response shape
        return {
            id: `chatcmpl-native-${Date.now()}`,
            object: "chat.completion",
            model: params.model || "native-cffi",
            choices: [{
                index: 0,
                message: { role: "assistant", content: result.content || "" },
                finish_reason: "stop",
            }],
            usage: {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
        };
    }

    private async _createStreamingCompletion(params: ChatCompletionRequest): Promise<IPCStream> {
        const stream = new IPCStream();
        const completionId = `chatcmpl-native-${Date.now()}`;

        const req = await ipcConnection.sendRequest("generate", {
            messages: params.messages,
            max_tokens: params.max_tokens || 512,
            stream: true,
        });

        // Wire up token callbacks to stream
        req.onToken!((msg) => {
            stream.pushChunk({
                id: completionId,
                object: "chat.completion.chunk",
                choices: [{
                    index: 0,
                    delta: { content: msg.content },
                    finish_reason: null,
                }],
            });
        });

        // Wire up completion
        req.promise.then((result) => {
            stream.pushChunk({
                id: completionId,
                object: "chat.completion.chunk",
                choices: [{
                    index: 0,
                    delta: {},
                    finish_reason: "stop",
                }],
            });
            stream.finish();
        }).catch((err) => {
            stream.setError(err);
        });

        return stream;
    }

    /**
     * Health check for the native engine.
     */
    async healthCheck(): Promise<boolean> {
        return ipcConnection.healthCheck();
    }

    /**
     * Destroy the IPC connection.
     */
    destroy() {
        ipcConnection.destroy();
    }
}

// Export a pre-configured instance for convenience
export const nativeIPCClient = new NativeIPCClient();
