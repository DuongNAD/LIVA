import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
const { mockParentPort, mockSession, mockTokenizer } = vi.hoisted(() => {
    const { EventEmitter } = require("node:events");
    const port = new EventEmitter() as any;
    port.postMessage = vi.fn();
    return {
        mockParentPort: port,
        mockSession: {
            inputNames: ["input_ids", "attention_mask", "token_type_ids"],
            outputNames: ["logits"],
            run: vi.fn().mockResolvedValue({
                logits: {
                    data: new Float32Array([1.5]) // logit of 1.5 -> sigmoid(1.5) = 0.817
                }
            }),
            release: vi.fn().mockResolvedValue(undefined)
        },
        mockTokenizer: {
            encode: vi.fn().mockImplementation((text: string) => {
                return {
                    ids: text.split(/\s+/).map((_, idx) => idx + 1),
                    attention_mask: text.split(/\s+/).map(() => 1),
                    token_type_ids: text.split(/\s+/).map(() => 0)
                };
            })
        }
    };
});
vi.mock("node:worker_threads", () => ({
    get parentPort() {
        return (globalThis as any).activeParentPort;
    }
}));

describe("FlashRankWorker Tokenizer and Reranking Test", () => {
    beforeEach(async () => {
        (globalThis as any).activeParentPort = mockParentPort;
        vi.resetModules();
        vi.clearAllMocks();
        mockParentPort.removeAllListeners();
        mockParentPort.postMessage.mockReset();
        mockSession.run.mockClear();
        vi.doMock("node:fs", () => ({
            existsSync: vi.fn().mockImplementation(() => true),
            readFileSync: vi.fn().mockImplementation(() => {
                return JSON.stringify({
                    model: {
                        vocab: {}
                    }
                });
            })
        }));
        vi.doMock("onnxruntime-node", () => ({
            InferenceSession: {
                create: vi.fn().mockResolvedValue(mockSession)
            },
            Tensor: class {
                type: string;
                data: any;
                dims: number[];
                constructor(type: string, data: any, dims: number[]) {
                    this.type = type;
                    this.data = data;
                    this.dims = dims;
                }
            }
        }));
        vi.doMock("@huggingface/tokenizers", () => ({
            Tokenizer: vi.fn().mockImplementation(function() { return mockTokenizer; })
        }));

        // Import the worker to run the listener setup code
        await import("../../src/workers/FlashRankWorker");
    });

    afterEach(() => {
        mockParentPort.removeAllListeners();
    });

    it("should initialize the worker in ONNX mode if files exist", async () => {
        const readyPromise = new Promise<void>((resolve, reject) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "ready") {
                    expect(msg.mode).toBe("onnx");
                    resolve();
                } else if (msg.type === "error") {
                    reject(new Error(msg.message));
                }
            });
        });

        mockParentPort.emit("message", { type: "init", modelPath: "fake_model.onnx" });
        await expect(readyPromise).resolves.not.toThrow();
    });

    it("should run onnx inference for rerank message", async () => {
        // 1. Initialize
        const readyPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "ready") resolve();
            });
        });
        mockParentPort.emit("message", { type: "init", modelPath: "fake_model.onnx" });
        await readyPromise;

        // 2. Rerank
        const resultPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "result") {
                    expect(msg.reranked).toBeDefined();
                    expect(msg.reranked.length).toBe(2);
                    // sigmoid(1.5) = 1 / (1 + exp(-1.5)) = ~0.817
                    expect(msg.reranked[0].score).toBeCloseTo(0.81757, 4);
                    resolve();
                }
            });
        });

        mockParentPort.emit("message", {
            type: "rerank",
            id: "job-1",
            query: "hello test",
            documents: ["this is a hello test doc", "another doc"]
        });

        await expect(resultPromise).resolves.not.toThrow();
        expect(mockTokenizer.encode).toHaveBeenCalledTimes(4); // 2 per doc (1 query, 1 doc) * 2 docs
    });

    it("should fallback to simulated scoring if ONNX fails", async () => {
        // 1. Initialize
        const readyPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "ready") resolve();
            });
        });
        mockParentPort.emit("message", { type: "init", modelPath: "fake_model.onnx" });
        await readyPromise;

        // Make ONNX run throw an error
        mockSession.run.mockRejectedValueOnce(new Error("ONNX runtime error"));

        // 2. Rerank
        const resultPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "result") {
                    expect(msg.reranked).toBeDefined();
                    expect(msg.reranked.length).toBe(1);
                    // Expect simulated score (Jaccard + phrase match)
                    // Query: "hello", Doc: "hello world"
                    // Query words: ["hello"] (1 word), Doc words: ["hello", "world"] (2 words)
                    // Jaccard: 1 / 2 = 0.5. Substring bonus: 0.5.
                    // Score = 0.5 * 0.7 + 0.5 = 0.85
                    expect(msg.reranked[0].score).toBeCloseTo(0.85, 2);
                    resolve();
                }
            });
        });

        mockParentPort.emit("message", {
            type: "rerank",
            id: "job-2",
            query: "hello",
            documents: ["hello world"]
        });

        await expect(resultPromise).resolves.not.toThrow();
    });

    it("should handle invalid parameters gracefully", async () => {
        // 1. Initialize
        const readyPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "ready") resolve();
            });
        });
        mockParentPort.emit("message", { type: "init", modelPath: "fake_model.onnx" });
        await readyPromise;

        // 2. Send invalid rerank (missing documents)
        const errorPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "error") {
                    expect(msg.message).toContain("Invalid parameters");
                    resolve();
                }
            });
        });

        mockParentPort.emit("message", {
            type: "rerank",
            id: "job-3",
            query: "hello"
        });

        await expect(errorPromise).resolves.not.toThrow();
    });
});
