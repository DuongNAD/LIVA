import { describe, it, expect, vi, beforeEach } from "vitest";
import { EventEmitter } from "node:events";

const mockParentPort = new EventEmitter() as any;
mockParentPort.postMessage = vi.fn().mockImplementation((msg: any) => {
    console.log("[Test mockParentPort] Received postMessage from worker:", msg);
});

vi.mock("node:worker_threads", () => {
    return {
        parentPort: mockParentPort
    };
});

// Mock fs to simulate existence of config/models
vi.mock("node:fs", () => {
    return {
        existsSync: vi.fn().mockImplementation(() => {
            return true;
        }),
        readFileSync: vi.fn().mockImplementation((path: string) => {
            if (path.includes("tokenizer.json")) {
                return JSON.stringify({
                    model: {
                        vocab: Array.from({ length: 13088 }, (_, i) => {
                            if (i === 5) return ["▁hello", 0];
                            if (i === 13087) return ["<blank>", 0];
                            return [`token_${i}`, 0];
                        })
                    }
                });
            }
            if (path.includes("genai_config.json")) {
                return JSON.stringify({
                    model: {
                        vocab_size: 13088,
                        blank_id: 13087,
                        max_symbols_per_step: 10,
                        encoder: { filename: "encoder.onnx", hidden_size: 1024, num_hidden_layers: 24 },
                        decoder: { filename: "decoder.onnx", hidden_size: 640, num_hidden_layers: 2 },
                        joiner: { filename: "joint.onnx" }
                    }
                });
            }
            return "";
        })
    };
});

let mockEncoderRun = vi.fn();
let mockDecoderRun = vi.fn();
let mockJoinerRun = vi.fn();

vi.mock("onnxruntime-node", () => {
    return {
        InferenceSession: {
            create: vi.fn().mockImplementation(async (modelPath: string) => {
                if (modelPath.includes("encoder.onnx")) {
                    return { run: mockEncoderRun, release: vi.fn() };
                } else if (modelPath.includes("decoder.onnx")) {
                    return { run: mockDecoderRun, release: vi.fn() };
                } else {
                    return { run: mockJoinerRun, release: vi.fn() };
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

describe("NemotronWorker ASR Streaming Test", () => {
    beforeEach(async () => {
        vi.resetModules();
        vi.clearAllMocks();
        mockParentPort.removeAllListeners();

        // Setup default mock runs
        mockEncoderRun.mockResolvedValue({
            cache_last_channel_next: {
                data: new Float32Array(24 * 1 * 1024 * 56)
            },
            cache_last_time_next: {
                data: new Float32Array(24 * 1 * 1024 * 8)
            },
            cache_last_channel_len_next: {
                data: new BigInt64Array([10n])
            },
            outputs: {
                dims: [1, 9, 1024],
                data: new Float32Array(1 * 9 * 1024)
            },
            encoded_lengths: {
                data: new BigInt64Array([9n])
            }
        });

        mockDecoderRun.mockResolvedValue({
            h_out: {
                data: new Float32Array(2 * 1 * 640)
            },
            c_out: {
                data: new Float32Array(2 * 1 * 640)
            },
            decoder_output: {
                dims: [1, 1, 640],
                data: new Float32Array(640)
            }
        });

        // Alternate or return token 5 (which decodes to "hello") and then blankId
        let joinerCallCount = 0;
        mockJoinerRun.mockImplementation(async () => {
            const logits = new Float32Array(13088);
            if (joinerCallCount % 2 === 0) {
                logits[5] = 10.0; // Index 5 max
            } else {
                logits[13087] = 10.0; // BlankId max
            }
            joinerCallCount++;
            return {
                joint_output: {
                    dims: [1, 1, 1, 13088],
                    data: logits
                }
            };
        });

        // Import worker to register event listeners
        await import("../../src/workers/NemotronWorker");
    });

    it("should initialize the worker and model successfully", async () => {
        const readyPromise = new Promise<void>((resolve, reject) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "ready") {
                    resolve();
                } else if (msg.type === "error") {
                    reject(new Error(msg.message));
                }
            });
        });

        mockParentPort.emit("message", { type: "init", modelDir: "fake_model_dir", language: "vi" });
        await expect(readyPromise).resolves.not.toThrow();
    });

    it("should process audio chunks and generate transcriptions", async () => {
        // Initialize
        const readyPromise = new Promise<void>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "ready") resolve();
            });
        });
        mockParentPort.emit("message", { type: "init", modelDir: "fake_model_dir", language: "vi" });
        await readyPromise;

        // Reset mock postMessage to capture partials and finals
        const messages: any[] = [];
        mockParentPort.postMessage.mockImplementation((msg: any) => {
            messages.push(msg);
        });

        // 10640 samples are needed for mel spectrogram processing
        // Let's send 10640 Float32 samples
        const audioBuffer = new Float32Array(10640);
        mockParentPort.emit("message", {
            type: "audio_chunk",
            buffer: audioBuffer,
            isLast: false
        });

        // Since queue processing is async, wait a bit
        await new Promise((resolve) => setTimeout(resolve, 100));

        // Send isLast = true to trigger final
        mockParentPort.emit("message", {
            type: "audio_chunk",
            buffer: new Float32Array(0),
            isLast: true
        });

        await new Promise((resolve) => setTimeout(resolve, 100));

        // Check that we received partials and finals
        expect(messages.some(m => m.type === "partial")).toBe(true);
        expect(messages.some(m => m.type === "final")).toBe(true);
    });
});
