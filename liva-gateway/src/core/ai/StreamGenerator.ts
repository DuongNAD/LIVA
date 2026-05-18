import { logger } from "../../utils/logger";
import type { EventCatalog } from "../events/EventCatalog";
import { TypedEventBus } from "../events/TypedEventBus";

export interface AiMessage {
    readonly role: string;
    readonly content: string;
}

interface StreamCompletionRequest {
    readonly model: string;
    readonly messages: readonly AiMessage[];
    readonly temperature: number;
    readonly max_tokens: number;
    readonly stream: true;
}

export interface StreamCompletionClient {
    readonly chat: {
        readonly completions: {
            readonly create: (
                request: StreamCompletionRequest,
            ) => Promise<AsyncIterable<unknown>> | AsyncIterable<unknown>;
        };
    };
}

export interface StreamGeneratorOptions {
    readonly routerClient: StreamCompletionClient;
    readonly expertClient: StreamCompletionClient;
    readonly eventBus: TypedEventBus<EventCatalog>;
}

export interface GenerateTextOptions {
    readonly messages: readonly AiMessage[];
    readonly query: string;
    readonly useExpert?: boolean;
    readonly maxTokens?: number;
    readonly streamId?: string;
}

interface StreamChoice {
    readonly delta?: {
        readonly content?: string | null;
    };
    readonly finish_reason?: string | null;
}

interface StreamChunkShape {
    readonly choices?: readonly StreamChoice[];
}

export class StreamGenerator {
    #routerClient: StreamCompletionClient;
    #expertClient: StreamCompletionClient;
    #eventBus: TypedEventBus<EventCatalog>;
    #streamSequence = 0;

    public constructor(options: StreamGeneratorOptions) {
        this.#routerClient = options.routerClient;
        this.#expertClient = options.expertClient;
        this.#eventBus = options.eventBus;
    }

    public async generateText(options: GenerateTextOptions): Promise<string> {
        const streamId = options.streamId ?? this.#createStreamId();

        try {
            return await this.#generateText(streamId, options);
        } catch (error: unknown) {
            this.#eventBus.emit("ai:stream_error", {
                id: streamId,
                error: this.#toError(error),
            });
            throw error;
        }
    }

    async #generateText(streamId: string, options: GenerateTextOptions): Promise<string> {
        const localMessages = [
            ...options.messages,
            { role: "user", content: options.query },
        ];
        const useExpert = options.useExpert === true;
        const client = useExpert ? this.#expertClient : this.#routerClient;
        const model = process.env.AI_PROVIDER?.toLowerCase() === "cloud"
            ? (process.env.AI_MODEL || "gpt-4")
            : (useExpert ? "local-ghost-expert" : "local-ghost-router");

        const stream = await client.chat.completions.create({
            model,
            messages: localMessages,
            temperature: 0.3,
            max_tokens: options.maxTokens ?? 2500,
            stream: true,
        });

        let fullContent = "";
        let buffer = "";
        let tokenIndex = 0;
        let isToolCallMode = false;
        let passedBufferCheck = false;
        let streamStarted = false;

        for await (const chunk of stream) {
            const token = this.#readToken(chunk);
            fullContent += token;

            if (!passedBufferCheck) {
                buffer += token;
                const isFinished = this.#isFinished(chunk);
                if (buffer.length >= 15 || isFinished) {
                    passedBufferCheck = true;

                    if (this.#looksLikeToolCall(buffer)) {
                        isToolCallMode = true;
                        logger.info("[Stream Mute] 🤫 LIVA đang nhẩm tính lệnh Kỹ năng ngầm...");
                    } else {
                        streamStarted = true;
                        this.#eventBus.emit("ai:stream_start", { id: streamId });
                        this.#eventBus.emit("ai:stream_chunk", {
                            id: streamId,
                            text: buffer,
                            index: tokenIndex,
                        });
                        tokenIndex += 1;
                    }
                }
                continue;
            }

            if (!isToolCallMode) {
                if (!streamStarted) {
                    streamStarted = true;
                    this.#eventBus.emit("ai:stream_start", { id: streamId });
                }
                this.#eventBus.emit("ai:stream_chunk", {
                    id: streamId,
                    text: token,
                    index: tokenIndex,
                });
                tokenIndex += 1;
            }
        }

        this.#eventBus.emit("ai:stream_complete", { id: streamId, text: fullContent });
        return fullContent;
    }

    #createStreamId(): string {
        this.#streamSequence += 1;
        return `ai-stream-${Date.now()}-${this.#streamSequence}`;
    }

    #looksLikeToolCall(buffer: string): boolean {
        const recentTail = buffer.slice(-30);
        return recentTail.includes("<to")
            || buffer.includes('{"name":')
            || buffer.trim().startsWith("{");
    }

    #readToken(chunk: unknown): string {
        const streamChunk = this.#asStreamChunk(chunk);
        const content = streamChunk?.choices?.[0]?.delta?.content;
        return typeof content === "string" ? content : "";
    }

    #isFinished(chunk: unknown): boolean {
        const streamChunk = this.#asStreamChunk(chunk);
        return Boolean(streamChunk?.choices?.[0]?.finish_reason);
    }

    #asStreamChunk(chunk: unknown): StreamChunkShape | null {
        if (typeof chunk !== "object" || chunk === null) {
            return null;
        }

        const candidate = chunk as { readonly choices?: unknown };
        if (!Array.isArray(candidate.choices)) {
            return null;
        }

        return candidate as StreamChunkShape;
    }

    #toError(error: unknown): Error {
        return error instanceof Error ? error : new Error(String(error));
    }
}
