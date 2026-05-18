import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import * as path from "node:path";
import { fileURLToPath } from 'node:url';
import { logger } from "./logger";
import { withSafeTimeout } from "./HttpClient";

const safeDelay = (ms: number) => new Promise<void>(resolve => {
    const timer = setTimeout(resolve, ms);
    if (typeof timer === 'object' && typeof timer.unref === 'function') {
        timer.unref();
    }
});

// ESM-first: Node.js 22+ supports import.meta.dirname natively
// SEA fallback: esbuild CJS bundle provides __dirname
const _dirname = import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));

const PROTO_PATH = path.join(_dirname, "../proto/liva_engine.proto");
const IPC_HOST = "127.0.0.1";
const IPC_PORT = 8100;

// Load the protobuf definition
const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
    keepCase: true,
    longs: String,
    enums: String,
    defaults: true,
    oneofs: true,
});

const protoDescriptor = grpc.loadPackageDefinition(packageDefinition);
const livaProto = protoDescriptor.liva as any;

// The gRPC client will be instantiated per NativeIPCClient instance.

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
    signal?: AbortSignal;
    [key: string]: unknown;
}

interface ChatCompletionChunk {
    id: string;
    object: string;
    choices: Array<{
        index: number;
        delta: { role?: string; content?: string };
        finish_reason: string | null;
    }>;
}

export interface ChatCompletionResponse {
    id: string;
    object: string;
    choices: Array<{
        index: number;
        message: { role: string; content: string };
        finish_reason: string;
    }>;
    model: string;
    usage?: { prompt_tokens: number; completion_tokens: number; total_tokens: number };
}

// ===========================
// Embedding Types (matches liva_engine.proto)
// ===========================

interface NativeEmbeddingData {
    embedding: number[];
    index: number;
}

export interface NativeEmbeddingResponse {
    data: NativeEmbeddingData[];
    model: string;
    dimensions: number;
}

/**
 * Async iterator for streaming gRPC responses.
 */
class GRPCStream implements AsyncIterable<ChatCompletionChunk> {
    private chunks: ChatCompletionChunk[] = [];
    private resolveNext: (() => void) | null = null;
    private done = false;
    private error: Error | null = null;

    pushChunk(chunk: ChatCompletionChunk) {
        this.chunks.push(chunk);
        if (this.resolveNext) {
            this.resolveNext();
            this.resolveNext = null;
        }
    }

    finish() {
        this.done = true;
        if (this.resolveNext) {
            this.resolveNext();
            this.resolveNext = null;
        }
    }

    fail(err: Error) {
        this.error = err;
        if (this.resolveNext) {
            this.resolveNext();
            this.resolveNext = null;
        }
    }

    async *[Symbol.asyncIterator](): AsyncGenerator<ChatCompletionChunk, void, unknown> {
        while (!this.done || this.chunks.length > 0) {
            if (this.chunks.length > 0) {
                yield this.chunks.shift()!;
                continue;
            }
            if (this.error) {
                // Drain any remaining chunks before throwing (CRITICAL FIX)
                while (this.chunks.length > 0) {
                    yield this.chunks.shift()!;
                }
                throw this.error;
            }
            // Wait for new data, end, or error signal
            await new Promise<void>((resolve) => {
                this.resolveNext = resolve;
            });
            // After wake-up, loop back to drain chunks before checking error.
            // This ensures chunks pushed before fail() are yielded first.
        }
    }
}

/**
 * NativeIPCClient
 * ================
 * Replaces OpenAI SDK, communicating with Python Engine via gRPC over HTTP/2.
 * This completely eliminates JSON over TCP and serialization bottlenecks.
 */
export class NativeIPCClient {
    private grpcClient: any;

    constructor() {
        this.grpcClient = new livaProto.LivaInferenceService(
            `${IPC_HOST}:${IPC_PORT}`,
            grpc.credentials.createInsecure(),
            {
                "grpc.keepalive_time_ms": 10000,
                "grpc.keepalive_timeout_ms": 5000,
                "grpc.keepalive_permit_without_calls": 1,
                "grpc.max_receive_message_length": 50 * 1024 * 1024, // 50MB
                // ⚡ [PERF] Disable HTTP proxy detection overhead on localhost
                "grpc.enable_http_proxy": 0,
            }
        );
    }
    public chat = {
        completions: {
            create: async (params: ChatCompletionRequest, retryCount = 0): Promise<GRPCStream | ChatCompletionResponse> => {
                const reqId = `g_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`; // NOSONAR
                
                const grpcRequest = {
                    model: params.model || "router",
                    messages: params.messages,
                    temperature: params.temperature ?? 0.7,
                    max_tokens: params.max_tokens ?? 2048,
                    stream: params.stream ?? false,
                    request_id: reqId,
                    top_p: params.top_p ?? 1,
                    frequency_penalty: params.frequency_penalty ?? 0,
                    presence_penalty: params.presence_penalty ?? 0,
                    stop: params.stop || []
                };

                if (params.stream) {
                    return new Promise((resolve, reject) => {
                        const streamResult = new GRPCStream();
                        const call = this.grpcClient.StreamChat(grpcRequest);

                        call.on("data", (chunk: ChatCompletionChunk) => {
                            streamResult.pushChunk(chunk);
                        });

                        call.on("end", () => {
                            streamResult.finish();
                        });

                        if (params.signal) {
                            params.signal.addEventListener("abort", () => {
                                call.cancel();
                                streamResult.fail(new Error("AbortError"));
                            }, { once: true });
                        }

                        call.on("error", async (err: grpc.ServiceError) => {
                            logger.error(`[NativeIPC] gRPC Stream Error: ${err.message}`);
                            if (err.message.includes("14 UNAVAILABLE") && retryCount < 3) {
                                logger.warn(`[NativeIPC] Retrying stream... (${retryCount + 1}/3)`);
                                await safeDelay(Math.pow(2, retryCount) * 500);
                                try {
                                    const newStream = await this.chat.completions.create(params, retryCount + 1) as GRPCStream;
                                    (async () => {
                                        try {
                                            for await (const chunk of newStream) {
                                                streamResult.pushChunk(chunk);
                                            }
                                            streamResult.finish();
                                        } catch (e) {
                                            streamResult.fail(e as Error);
                                        }
                                    })();
                                } catch (e) {
                                    streamResult.fail(e as Error);
                                    reject(e);
                                }
                            } else {
                                streamResult.fail(err);
                                reject(err);
                            }
                        });

                        resolve(streamResult);
                    });
                } else {
                    try {
                        return await new Promise<ChatCompletionResponse>((resolve, reject) => {
                            const call = this.grpcClient.Chat(grpcRequest, (err: grpc.ServiceError | null, response: ChatCompletionResponse) => {
                                if (err) {
                                    reject(err);
                                    return;
                                }
                                resolve(response);
                            });

                            if (params.signal) {
                                params.signal.addEventListener("abort", () => {
                                    call.cancel();
                                    reject(new Error("AbortError"));
                                }, { once: true });
                            }
                        });
                    } catch (err: unknown) {
                        const errMsg = err instanceof Error ? err.message : String(err);
                        if (errMsg.includes("14 UNAVAILABLE") && retryCount < 3) {
                            logger.warn(`[NativeIPC] Retrying unary... (${retryCount + 1}/3)`);
                            await safeDelay(Math.pow(2, retryCount) * 500);
                            return this.chat.completions.create(params, retryCount + 1);
                        }
                        logger.error(`[NativeIPC] gRPC Unary Error: ${errMsg}`);
                        throw err;
                    }
                }
            }
        }
    };

    /**
     * Helper to verify connection to the engine
     */
    async healthCheck(): Promise<boolean> {
        return new Promise((resolve) => {
            this.grpcClient.HealthCheck({}, (err: grpc.ServiceError | null, response: { alive?: boolean }) => {
                if (err) {
                    resolve(false);
                    return;
                }
                resolve(response?.alive === true);
            });
        });
    }

    /**
     * Generate embeddings via gRPC Embed RPC.
     * Supports single string or batch (string[]) input.
     * Returns L2-normalized vectors from the Python engine.
     *
     * [CIRCUIT BREAKER] Wrapped with withSafeTimeout(15s) to prevent
     * zombie Promise deadlock if the Python engine C++ binding hangs.
     * This satisfies Rule 4.2: no gRPC call may Pending indefinitely.
     */
    async embed(input: string | string[]): Promise<NativeEmbeddingResponse> {
        const texts = Array.isArray(input) ? input : [input];

        const task = new Promise<NativeEmbeddingResponse>((resolve, reject) => {
            this.grpcClient.Embed(
                { input: texts, model: "embedding" },
                (err: grpc.ServiceError | null, response: NativeEmbeddingResponse) => {
                    if (err) {
                        logger.error(`[NativeIPC] gRPC Embed error: ${err.message}`);
                        return reject(err);
                    }
                    resolve(response);
                }
            );
        });

        return withSafeTimeout(task, 15000, "NativeIPC_Embed_Timeout");
    }

    /**
     * Cleans up the gRPC client channels
     */
    destroy() {
        if (this.grpcClient) {
            this.grpcClient.close();
            this.grpcClient = null;
        }
    }
}
