import { describe, expect, it, vi } from "vitest";
import { StreamGenerator, type StreamCompletionClient } from "../../../src/core/ai/StreamGenerator";
import type { EventCatalog } from "../../../src/core/events/EventCatalog";
import { TypedEventBus } from "../../../src/core/events/TypedEventBus";

vi.mock("../../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
        child: vi.fn().mockReturnThis(),
    },
}));

function createStream(tokens: Array<{ content: string; finishReason?: string | null }>): AsyncIterable<unknown> {
    return {
        async *[Symbol.asyncIterator]() {
            for (const token of tokens) {
                yield {
                    choices: [
                        {
                            delta: { content: token.content },
                            finish_reason: token.finishReason ?? null,
                        },
                    ],
                };
            }
        },
    };
}

function createClient(stream: AsyncIterable<unknown>): StreamCompletionClient {
    return {
        chat: {
            completions: {
                create: vi.fn().mockResolvedValue(stream),
            },
        },
    };
}

describe("StreamGenerator", () => {
    it("emits stream events for normal text while returning full content", async () => {
        const eventBus = new TypedEventBus<EventCatalog>();
        const routerClient = createClient(createStream([
            { content: "This is a normal " },
            { content: "response", finishReason: "stop" },
        ]));
        const expertClient = createClient(createStream([]));
        const generator = new StreamGenerator({ routerClient, expertClient, eventBus });
        const starts: string[] = [];
        const chunks: string[] = [];
        const completes: string[] = [];

        eventBus.on("ai:stream_start", (payload) => starts.push(payload.id));
        eventBus.on("ai:stream_chunk", (payload) => chunks.push(payload.text));
        eventBus.on("ai:stream_complete", (payload) => completes.push(payload.text));

        const fullText = await generator.generateText({
            messages: [{ role: "system", content: "ctx" }],
            query: "hello",
            streamId: "stream-1",
        });

        expect(fullText).toBe("This is a normal response");
        expect(starts).toEqual(["stream-1"]);
        expect(chunks).toEqual(["This is a normal ", "response"]);
        expect(completes).toEqual(["This is a normal response"]);
        expect(routerClient.chat.completions.create).toHaveBeenCalledWith(expect.objectContaining({
            model: "local-ghost-router",
            stream: true,
        }));
    });

    it("mutes tool-call streams while still returning raw content", async () => {
        const eventBus = new TypedEventBus<EventCatalog>();
        const routerClient = createClient(createStream([
            { content: '{"name":"send_zalo_bot","arguments":{}}', finishReason: "stop" },
        ]));
        const generator = new StreamGenerator({
            routerClient,
            expertClient: createClient(createStream([])),
            eventBus,
        });
        const chunks: string[] = [];

        eventBus.on("ai:stream_chunk", (payload) => chunks.push(payload.text));

        const fullText = await generator.generateText({
            messages: [],
            query: "send",
            streamId: "stream-2",
        });

        expect(fullText).toBe('{"name":"send_zalo_bot","arguments":{}}');
        expect(chunks).toEqual([]);
    });

    it("uses expert client when requested", async () => {
        const routerClient = createClient(createStream([]));
        const expertClient = createClient(createStream([{ content: "expert answer", finishReason: "stop" }]));
        const generator = new StreamGenerator({
            routerClient,
            expertClient,
            eventBus: new TypedEventBus<EventCatalog>(),
        });

        await generator.generateText({
            messages: [],
            query: "deep task",
            useExpert: true,
        });

        expect(routerClient.chat.completions.create).not.toHaveBeenCalled();
        expect(expertClient.chat.completions.create).toHaveBeenCalledWith(expect.objectContaining({
            model: "local-ghost-expert",
        }));
    });
});
