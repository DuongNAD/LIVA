import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import * as path from "node:path";
import { fileURLToPath } from 'node:url';
import { logger } from "./logger";

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

// Create the gRPC client
const grpcClient = new livaProto.LivaInferenceService(
    `${IPC_HOST}:${IPC_PORT}`,
    grpc.credentials.createInsecure(),
    {
        "grpc.keepalive_time_ms": 10000,
        "grpc.keepalive_timeout_ms": 5000,
        "grpc.keepalive_permit_without_calls": 1,
        "grpc.max_receive_message_length": 50 * 1024 * 1024 // 50MB
    }
);

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

interface ChatCompletionChunk {
    id: string;
    object: string;
    choices: Array<{
        index: number;
        delta: { role?: string; content?: string };
        finish_reason: string | null;
    }>;
}

interface ChatCompletionResponse {
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
    public chat = {
        completions: {
            create: async (params: ChatCompletionRequest): Promise<any> => {
                const reqId = `g_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`; // NOSONAR
                
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
                    const streamResult = new GRPCStream();
                    const call = grpcClient.StreamChat(grpcRequest);

                    call.on("data", (chunk: any) => {
                        // The chunk is already parsed from protobuf!
                        streamResult.pushChunk(chunk);
                    });

                    call.on("end", () => {
                        streamResult.finish();
                    });

                    call.on("error", (err: any) => {
                        logger.error(`[NativeIPC] gRPC Stream Error: ${err.message}`);
                        streamResult.fail(err);
                    });

                    return streamResult;
                } else {
                    return new Promise<ChatCompletionResponse>((resolve, reject) => {
                        grpcClient.Chat(grpcRequest, (err: any, response: any) => {
                            if (err) {
                                logger.error(`[NativeIPC] gRPC Unary Error: ${err.message}`);
                                reject(err);
                                return;
                            }
                            resolve(response);
                        });
                    });
                }
            }
        }
    };

    /**
     * Helper to verify connection to the engine
     */
    async healthCheck(): Promise<boolean> {
        return new Promise((resolve) => {
            grpcClient.HealthCheck({}, (err: any, response: any) => {
                if (err) {
                    resolve(false);
                    return;
                }
                resolve(response?.alive === true);
            });
        });
    }

    /**
     * Cleans up the gRPC client channels
     */
    destroy() {
        if (grpcClient) {
            grpcClient.close();
        }
    }
}
