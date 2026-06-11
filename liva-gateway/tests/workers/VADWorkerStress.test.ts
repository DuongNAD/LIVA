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

let activeRuns = 0;
let maxConcurrentRuns = 0;
const processedSequence: number[] = [];

vi.mock("onnxruntime-web", () => {
    return {
        env: {
            wasm: { numThreads: 1 }
        },
        InferenceSession: {
            create: vi.fn().mockResolvedValue({
                run: vi.fn().mockImplementation(async (inputs: any) => {
                    activeRuns++;
                    if (activeRuns > maxConcurrentRuns) {
                        maxConcurrentRuns = activeRuns;
                    }
                    const samples = inputs.input.data;
                    const seqNum = samples[0];
                    await new Promise(resolve => setTimeout(resolve, 10));
                    processedSequence.push(seqNum);
                    activeRuns--;
                    return {
                        hn: {},
                        cn: {},
                        output: {
                            data: new Float32Array([0.8])
                        }
                    };
                })
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

describe("VADWorker Stress & Concurrency Test", () => {
    beforeEach(async () => {
        vi.resetModules();
        vi.clearAllMocks();
        mockParentPort.removeAllListeners();
        processedSequence.length = 0;
        activeRuns = 0;
        maxConcurrentRuns = 0;

        console.log("[Test] Importing VADWorker...");
        try {
            await import("../../src/workers/VADWorker");
            console.log("[Test] VADWorker imported successfully");
        } catch (e: any) {
            console.error("[Test] Import VADWorker failed:", e);
        }
    });

    it("should initialize the worker and model successfully", async () => {
        const resultPromise = new Promise<{ type: string; message?: string }>((resolve) => {
            mockParentPort.on("message", (msg: any) => {
                // Since parentPort.postMessage is mocked, let's also capture what the worker sends back
            });
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "ready" || msg.type === "error") {
                    resolve(msg);
                }
            });
        });

        console.log("[Test] Emitting init message to worker...");
        mockParentPort.emit("message", { type: "init", modelPath: "fake_model.onnx" });

        const result = await resultPromise;
        console.log("[Test] Init result:", result);
        expect(result.type).toBe("ready");
    }, 10000);

    it("should process 200 rapid concurrent audio frames sequentially (FIFO Queue Verification)", async () => {
        // Initialize first
        const initPromise = new Promise<any>((resolve) => {
            mockParentPort.postMessage.mockImplementation((msg: any) => {
                if (msg.type === "ready" || msg.type === "error") resolve(msg);
            });
        });
        mockParentPort.emit("message", { type: "init", modelPath: "fake_model.onnx" });
        const initRes = await initPromise;
        expect(initRes.type).toBe("ready");

        const totalFrames = 200;
        const receivedSequence: boolean[] = [];

        mockParentPort.postMessage.mockImplementation((msg: any) => {
            if (msg.type === "vad_result") {
                receivedSequence.push(msg.isSpeech);
            }
        });

        console.log("[Test] Pushing 200 concurrent audio frames...");
        for (let i = 0; i < totalFrames; i++) {
            const buffer = new Float32Array(480);
            buffer[0] = i; // Store sequence index
            mockParentPort.emit("message", { type: "audio", buffer });
        }

        console.log("[Test] Waiting for all audio frames to be processed...");
        const start = Date.now();
        while (processedSequence.length < totalFrames && Date.now() - start < 10000) {
            await new Promise(resolve => setTimeout(resolve, 50));
        }

        console.log(`[Test] Processed ${processedSequence.length} frames. Max concurrency in run: ${maxConcurrentRuns}`);
        expect(maxConcurrentRuns).toBe(1);
        expect(processedSequence.length).toBe(totalFrames);

        for (let i = 0; i < totalFrames; i++) {
            expect(processedSequence[i]).toBe(i);
        }
        expect(receivedSequence.length).toBe(totalFrames);
    }, 15000);
});
