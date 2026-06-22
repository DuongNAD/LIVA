import { describe, it, expect, vi, beforeEach } from "vitest";
import { EventEmitter } from "node:events";

const mockParentPort = new EventEmitter() as any;
mockParentPort.postMessage = vi.fn();

vi.mock("node:worker_threads", () => {
    return {
        parentPort: mockParentPort
    };
});

// Mock fs to simulate existence of config/models
vi.mock("node:fs", () => {
    return {
        existsSync: vi.fn().mockImplementation(() => true),
        readFileSync: vi.fn().mockImplementation(() => {
            return JSON.stringify({
                model: {
                    vocab: {}
                }
            });
        })
    };
});

// Mock onnxruntime-node
const mockPreprocessSession = {
    run: vi.fn().mockResolvedValue({
        features: {
            dims: [1, 64, 50],
            data: new Float32Array(1 * 64 * 50)
        }
    }),
    release: vi.fn().mockResolvedValue(undefined)
};

const mockEncoderSession = {
    run: vi.fn().mockResolvedValue({
        encoder_out: {
            dims: [1, 50, 128],
            data: new Float32Array(1 * 50 * 128)
        }
    }),
    release: vi.fn().mockResolvedValue(undefined)
};

const mockDecoderSession = {
    run: vi.fn(),
    release: vi.fn().mockResolvedValue(undefined)
};

vi.mock("onnxruntime-node", () => {
    return {
        InferenceSession: {
            create: vi.fn().mockImplementation(async (modelPath: string) => {
                if (modelPath.includes("preprocess.onnx")) {
                    return mockPreprocessSession;
                } else if (modelPath.includes("encoder.onnx")) {
                    return mockEncoderSession;
                } else {
                    return mockDecoderSession;
                }
            })
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
    };
});

const mockTokenizer = {
    decode: vi.fn().mockImplementation((tokens: number[], options?: any) => {
        return tokens.map(t => `token_${t}`).join(" ");
    })
};

vi.mock("@huggingface/tokenizers", () => {
    return {
        Tokenizer: vi.fn().mockImplementation(() => mockTokenizer)
    };
});

describe("MoonshineWorker Tokenizer decoding and ASR streaming", () => {
    beforeEach(async () => {
        vi.resetModules();
        vi.clearAllMocks();
        mockParentPort.removeAllListeners();
        mockParentPort.postMessage.mockReset();
        mockPreprocessSession.run.mockClear();
        mockEncoderSession.run.mockClear();
        mockDecoderSession.run.mockClear();

        // Import the worker to run the listener setup code
        await import("../../src/workers/MoonshineWorker");
    });

    it("should initialize sessions and post ready message", async () => {
        const readyPromise = new Promise<void>((resolve, reject) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "ready") {
                    resolve();
                } else if (msg.type === "error") {
                    reject(new Error(msg.message));
                }
            });
        });

        mockParentPort.emit("message", { type: "init", modelDir: "fake_model_dir" });
        await expect(readyPromise).resolves.not.toThrow();
    });

    it("should decode using tokenizer under normal ONNX inference", async () => {
        // 1. Initialize
        const readyPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "ready") resolve();
            });
        });
        mockParentPort.emit("message", { type: "init", modelDir: "fake_model_dir" });
        await readyPromise;

        // Mock decoder run to return a word (index 5) on first call, and EOS (index 2) on second call
        let decodeCallCount = 0;
        mockDecoderSession.run.mockImplementation(async (feeds: any) => {
            const tokensLength = feeds.tokens.dims[1];
            const vocabSize = 10;
            const logitsData = new Float32Array(tokensLength * vocabSize);
            
            if (decodeCallCount === 0) {
                // Return index 5
                logitsData[5] = 10.0;
            } else {
                // Last token offset: (tokensLength - 1) * vocabSize
                const offset = (tokensLength - 1) * vocabSize;
                // Return EOS (index 2)
                logitsData[offset + 2] = 10.0;
            }
            decodeCallCount++;
            
            return {
                logits: {
                    dims: [1, tokensLength, vocabSize],
                    data: logitsData
                }
            };
        });

        // 2. Send audio chunk
        const resultPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "final") {
                    // Tokens: [1, 5] -> decoded: "token_1 token_5"
                    expect(msg.text).toBe("token_1 token_5");
                    resolve();
                }
            });
        });

        const audioBuffer = new Float32Array(1600); // 100ms at 16kHz
        mockParentPort.emit("message", {
            type: "audio_chunk",
            buffer: audioBuffer,
            isLast: true
        });

        await expect(resultPromise).resolves.not.toThrow();
        expect(mockPreprocessSession.run).toHaveBeenCalledOnce();
        expect(mockEncoderSession.run).toHaveBeenCalledOnce();
        expect(mockDecoderSession.run).toHaveBeenCalledTimes(2);
        expect(mockTokenizer.decode).toHaveBeenCalledOnce();
    });

    it("should handle ping message", async () => {
        const pongPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "pong") {
                    resolve();
                }
            });
        });

        mockParentPort.emit("message", { type: "ping" });
        await expect(pongPromise).resolves.not.toThrow();
    });

    it("should handle reset message", async () => {
        // Reset doesn't send a response, but we should make sure sending it doesn't throw
        mockParentPort.emit("message", { type: "reset" });
        // Send a ping to verify worker is still responsive
        const pongPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "pong") {
                    resolve();
                }
            });
        });

        mockParentPort.emit("message", { type: "ping" });
        await expect(pongPromise).resolves.not.toThrow();
    });
});
