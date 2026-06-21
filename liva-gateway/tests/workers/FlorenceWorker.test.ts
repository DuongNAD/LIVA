import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

// Mock the logger to prevent cluttering output
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn()
    }
}));

// Mock onnxruntime-node
const mockSession = {
    inputNames: ["pixel_values"],
    run: vi.fn().mockResolvedValue({
        output: {
            type: "string",
            data: ["cat", "dog"],
            dims: [2]
        },
        coordinates: {
            type: "float32",
            data: new Float32Array([10, 20, 100, 200, 30, 40, 150, 250]),
            dims: [2, 4]
        }
    }),
    release: vi.fn().mockResolvedValue(undefined)
};

vi.mock("onnxruntime-node", () => {
    return {
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
    };
});

// Mock sharp image processor
vi.mock("sharp", () => {
    const mockSharpFn = vi.fn().mockImplementation(() => {
        return {
            resize: vi.fn().mockReturnThis(),
            raw: vi.fn().mockReturnThis(),
            toBuffer: vi.fn().mockResolvedValue({
                data: Buffer.alloc(768 * 768 * 3),
                info: { width: 768, height: 768, channels: 3 }
            })
        };
    });
    return {
        default: mockSharpFn,
        __esModule: true
    };
});

// Setup mock worker threads bridge
const mockParentPort = new EventEmitter() as any;
const mockWorkerInstances: any[] = [];

class MockWorker extends EventEmitter {
    postMessage = vi.fn().mockImplementation((msg: any) => {
        // Asynchronously emit event to simulate worker thread receiving message
        process.nextTick(() => {
            mockParentPort.emit("message", msg);
        });
    });

    terminate = vi.fn().mockResolvedValue(0);

    constructor(_workerPath: any, _options: any) {
        super();
        mockWorkerInstances.push(this);
    }
}

mockParentPort.postMessage = vi.fn().mockImplementation((msg: any) => {
    // Send message back to the active main thread worker instance
    const activeWorker = mockWorkerInstances[mockWorkerInstances.length - 1];
    if (activeWorker) {
        process.nextTick(() => {
            activeWorker.emit("message", msg);
        });
    }
});

vi.mock("node:worker_threads", () => {
    return {
        Worker: MockWorker,
        parentPort: mockParentPort
    };
});

// Import service & load worker thread in-process
import { FlorenceVisionService } from "../../src/services/FlorenceVisionService";

describe("Florence Vision Pipeline (Milestone 1)", () => {
    let service: FlorenceVisionService;

    beforeEach(async () => {
        vi.clearAllMocks();
        mockWorkerInstances.length = 0;
        service = new FlorenceVisionService();

        // Dynamically import worker so its message listeners register on mockParentPort
        await import("../../src/workers/FlorenceWorker");
    });

    afterEach(async () => {
        await service.dispose();
    });

    it("should successfully initialize the worker thread and load ONNX model", async () => {
        const initPromise = service.initialize("/mock/path/model.onnx");

        await expect(initPromise).resolves.not.toThrow();
        expect(mockWorkerInstances.length).toBe(1);

        // Verify init message was sent to worker
        const mockWorker = mockWorkerInstances[0];
        expect(mockWorker.postMessage).toHaveBeenCalledWith(
            expect.objectContaining({ type: "init", modelPath: "/mock/path/model.onnx" })
        );
    });

    it("should run inference on the image buffer and return text and coordinates", async () => {
        await service.initialize("/mock/path/model.onnx");

        const mockImageBuffer = Buffer.from("fake-image-data");
        const resultPromise = service.processImage(mockImageBuffer);

        await expect(resultPromise).resolves.toEqual({
            text: "cat dog",
            coordinates: [
                [10, 20, 100, 200],
                [30, 40, 150, 250]
            ]
        });
    });

    it("should gracefully handle uninitialized processImage call", async () => {
        const mockImageBuffer = Buffer.from("fake-image-data");
        await expect(service.processImage(mockImageBuffer)).rejects.toThrow(
            "FlorenceVisionService is not initialized. Call initialize() first."
        );
    });

    it("should handle session initialization failures gracefully", async () => {
        const { InferenceSession } = await import("onnxruntime-node");
        vi.mocked(InferenceSession.create).mockRejectedValueOnce(new Error("Failed to load model file"));

        const failedService = new FlorenceVisionService();
        await expect(failedService.initialize("/mock/path/invalid.onnx")).rejects.toThrow(
            "FlorenceWorker init failed: Failed to load model file"
        );
    });

    it("should handle processing errors inside the worker thread", async () => {
        await service.initialize("/mock/path/model.onnx");

        // Force InferenceSession.run to throw
        mockSession.run.mockRejectedValueOnce(new Error("Inference execution failed"));

        const mockImageBuffer = Buffer.from("fake-image-data");
        await expect(service.processImage(mockImageBuffer)).rejects.toThrow(
            "Inference execution failed"
        );
    });

    it("should reject all pending requests on disposal", async () => {
        await service.initialize("/mock/path/model.onnx");

        // Make InferenceSession.run hang/delay so we can trigger dispose while running
        mockSession.run.mockImplementationOnce(async () => {
            return new Promise((resolve) => setTimeout(resolve, 1000));
        });

        const mockImageBuffer = Buffer.from("fake-image-data");
        const processPromise = service.processImage(mockImageBuffer);

        // Disposing immediately
        await service.dispose();

        await expect(processPromise).rejects.toThrow("FlorenceVisionService was disposed");
    });
});
