import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

/**
 * NativeIPCClient Integration Test Suite
 * ========================================
 * Tests the gRPC client layer that bridges Node.js Gateway ↔ Python Native Engine.
 * 
 * Strategy: We mock @grpc/grpc-js and @grpc/proto-loader at the module level 
 * to simulate a gRPC server without requiring the real Python engine or GPU.
 * This ensures the test suite runs 100% offline in CI/CD (no hardware deps).
 */

// ============================================================
// Module-Level Mocks (must be before imports)
// ============================================================
const mockChat = vi.fn();
const mockStreamChat = vi.fn();
const mockHealthCheck = vi.fn();
const mockClose = vi.fn();

vi.mock("@grpc/grpc-js", () => {
    // Create a class that returns our mock methods when instantiated
    class MockLivaInferenceService {
        Chat = mockChat;
        StreamChat = mockStreamChat;
        HealthCheck = mockHealthCheck;
        close = mockClose;
    }
    return {
        loadPackageDefinition: () => ({
            liva: {
                LivaInferenceService: MockLivaInferenceService,
            },
        }),
        credentials: {
            createInsecure: vi.fn(),
        },
    };
});

vi.mock("@grpc/proto-loader", () => ({
    loadSync: vi.fn().mockReturnValue({}),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

// Dynamic import AFTER mocks
const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");

// ============================================================
// TEST GROUP 1: Unary Chat (Non-Streaming)
// ============================================================
describe("NativeIPCClient — Unary Chat", () => {
    let client: InstanceType<typeof NativeIPCClient>;

    beforeEach(() => {
        vi.clearAllMocks();
        client = new NativeIPCClient();
    });

    it("should send a unary chat request and receive response", async () => {
        const mockResponse = {
            id: "resp_123",
            object: "chat.completion",
            choices: [{
                index: 0,
                message: { role: "assistant", content: "Xin chào Anh Dương!" },
                finish_reason: "stop",
            }],
            model: "router",
            usage: { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 },
        };

        mockChat.mockImplementation((_req: any, cb: Function) => cb(null, mockResponse));

        const result = await client.chat.completions.create({
            messages: [{ role: "user", content: "Xin chào" }],
            stream: false,
        });

        expect(result).toBeDefined();
        expect(result.choices[0].message.content).toBe("Xin chào Anh Dương!");
        expect(result.model).toBe("router");
        expect(mockChat).toHaveBeenCalledTimes(1);
    });

    it("should pass temperature and max_tokens to gRPC request", async () => {
        mockChat.mockImplementation((req: any, cb: Function) => cb(null, { choices: [] }));

        await client.chat.completions.create({
            messages: [{ role: "user", content: "test" }],
            temperature: 0.3,
            max_tokens: 512,
            stream: false,
        });

        const sentReq = mockChat.mock.calls[0][0];
        expect(sentReq.temperature).toBe(0.3);
        expect(sentReq.max_tokens).toBe(512);
    });

    it("should use default values when optional params are omitted", async () => {
        mockChat.mockImplementation((req: any, cb: Function) => cb(null, { choices: [] }));

        await client.chat.completions.create({
            messages: [{ role: "user", content: "test" }],
        });

        const sentReq = mockChat.mock.calls[0][0];
        expect(sentReq.temperature).toBe(0.7);
        expect(sentReq.max_tokens).toBe(2048);
        expect(sentReq.model).toBe("router");
        expect(sentReq.stream).toBe(false);
    });

    it("should generate unique request IDs", async () => {
        mockChat.mockImplementation((req: any, cb: Function) => cb(null, { choices: [] }));

        await client.chat.completions.create({ messages: [{ role: "user", content: "a" }] });
        await client.chat.completions.create({ messages: [{ role: "user", content: "b" }] });

        const id1 = mockChat.mock.calls[0][0].request_id;
        const id2 = mockChat.mock.calls[1][0].request_id;
        expect(id1).not.toBe(id2);
        expect(id1).toMatch(/^g_\d+_/);
    });

    it("should reject on gRPC error", async () => {
        mockChat.mockImplementation((_req: any, cb: Function) =>
            cb(new Error("ECONNREFUSED 127.0.0.1:8100"), null)
        );

        await expect(
            client.chat.completions.create({
                messages: [{ role: "user", content: "test" }],
                stream: false,
            })
        ).rejects.toThrow("ECONNREFUSED");
    });
});

// ============================================================
// TEST GROUP 2: Streaming Chat (Server-Side Streaming)
// ============================================================
describe("NativeIPCClient — Streaming Chat", () => {
    let client: InstanceType<typeof NativeIPCClient>;

    beforeEach(() => {
        vi.clearAllMocks();
        client = new NativeIPCClient();
    });

    it("should return an async iterable for streaming", async () => {
        const { EventEmitter } = await import("events");
        const mockStream = new EventEmitter();

        mockStreamChat.mockReturnValue(mockStream);

        const stream = await client.chat.completions.create({
            messages: [{ role: "user", content: "hello" }],
            stream: true,
        });

        // Should be async iterable
        expect(stream[Symbol.asyncIterator]).toBeDefined();

        // Simulate server pushing chunks
        setTimeout(() => {
            mockStream.emit("data", {
                id: "chunk_1",
                choices: [{ index: 0, delta: { content: "Xin " }, finish_reason: null }],
            });
            mockStream.emit("data", {
                id: "chunk_2",
                choices: [{ index: 0, delta: { content: "chào!" }, finish_reason: "stop" }],
            });
            mockStream.emit("end");
        }, 10);

        const chunks: any[] = [];
        for await (const chunk of stream) {
            chunks.push(chunk);
        }

        expect(chunks).toHaveLength(2);
        expect(chunks[0].choices[0].delta.content).toBe("Xin ");
        expect(chunks[1].choices[0].delta.content).toBe("chào!");
    });

    it("should propagate stream error", async () => {
        const { EventEmitter } = await import("events");
        const mockStream = new EventEmitter();

        mockStreamChat.mockReturnValue(mockStream);

        const stream = await client.chat.completions.create({
            messages: [{ role: "user", content: "hello" }],
            stream: true,
        });

        // Simulate error mid-stream
        setTimeout(() => {
            mockStream.emit("data", {
                id: "chunk_1",
                choices: [{ index: 0, delta: { content: "..." }, finish_reason: null }],
            });
            mockStream.emit("error", new Error("Stream interrupted: peer reset"));
        }, 10);

        const chunks: any[] = [];
        await expect(async () => {
            for await (const chunk of stream) {
                chunks.push(chunk);
            }
        }).rejects.toThrow("Stream interrupted");

        // Should have received the first chunk before error
        expect(chunks).toHaveLength(1);
    });

    it("should handle empty stream (immediate end)", async () => {
        const { EventEmitter } = await import("events");
        const mockStream = new EventEmitter();

        mockStreamChat.mockReturnValue(mockStream);

        const stream = await client.chat.completions.create({
            messages: [{ role: "user", content: "hello" }],
            stream: true,
        });

        setTimeout(() => {
            mockStream.emit("end");
        }, 10);

        const chunks: any[] = [];
        for await (const chunk of stream) {
            chunks.push(chunk);
        }

        expect(chunks).toHaveLength(0);
    });

    it("should handle rapid burst of chunks without memory leak", async () => {
        const { EventEmitter } = await import("events");
        const mockStream = new EventEmitter();

        mockStreamChat.mockReturnValue(mockStream);

        const stream = await client.chat.completions.create({
            messages: [{ role: "user", content: "hello" }],
            stream: true,
        });

        // Simulate rapid burst of 100 chunks
        setTimeout(() => {
            for (let i = 0; i < 100; i++) {
                mockStream.emit("data", {
                    id: `chunk_${i}`,
                    choices: [{ index: 0, delta: { content: `W${i} ` }, finish_reason: null }],
                });
            }
            mockStream.emit("end");
        }, 10);

        const chunks: any[] = [];
        for await (const chunk of stream) {
            chunks.push(chunk);
        }

        expect(chunks).toHaveLength(100);
    });
});

// ============================================================
// TEST GROUP 3: Health Check
// ============================================================
describe("NativeIPCClient — Health Check", () => {
    let client: InstanceType<typeof NativeIPCClient>;

    beforeEach(() => {
        vi.clearAllMocks();
        client = new NativeIPCClient();
    });

    it("should return true when engine is alive", async () => {
        mockHealthCheck.mockImplementation((_req: any, cb: Function) =>
            cb(null, { alive: true })
        );

        const alive = await client.healthCheck();
        expect(alive).toBe(true);
    });

    it("should return false when engine is not responding", async () => {
        mockHealthCheck.mockImplementation((_req: any, cb: Function) =>
            cb(new Error("ECONNREFUSED"), null)
        );

        const alive = await client.healthCheck();
        expect(alive).toBe(false);
    });

    it("should return false when response has alive=false", async () => {
        mockHealthCheck.mockImplementation((_req: any, cb: Function) =>
            cb(null, { alive: false })
        );

        const alive = await client.healthCheck();
        expect(alive).toBe(false);
    });

    it("should return false for null response", async () => {
        mockHealthCheck.mockImplementation((_req: any, cb: Function) =>
            cb(null, null)
        );

        const alive = await client.healthCheck();
        expect(alive).toBe(false);
    });
});

// ============================================================
// TEST GROUP 4: Resource Cleanup
// ============================================================
describe("NativeIPCClient — Cleanup", () => {
    it("should close gRPC channel on destroy()", () => {
        const client = new NativeIPCClient();
        client.destroy();
        expect(mockClose).toHaveBeenCalledTimes(1);
    });
});
