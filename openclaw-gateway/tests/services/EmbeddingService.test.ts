import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================================
// Mock @huggingface/transformers to avoid downloading 140MB model
// ============================================================
const mockPipelineFn = vi.fn();

vi.mock("@huggingface/transformers", () => ({
    pipeline: mockPipelineFn,
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { EmbeddingService } from "../../src/services/EmbeddingService";

// ============================================================
// Tests
// ============================================================
describe("EmbeddingService", () => {
    // Reset singleton between tests (critical for singleton state leakage)
    afterEach(() => {
        const instance = EmbeddingService.getInstance();
        instance.dispose();
        // Reset the singleton reference
        // @ts-ignore — accessing private static for test cleanup
        EmbeddingService.instance = undefined;
        vi.resetAllMocks();
    });

    describe("Singleton Pattern", () => {
        it("should return the same instance on multiple calls", () => {
            const a = EmbeddingService.getInstance();
            const b = EmbeddingService.getInstance();
            expect(a).toBe(b);
        });

        it("should create a new instance after dispose + reset", () => {
            const a = EmbeddingService.getInstance();
            a.dispose();
            // @ts-ignore
            EmbeddingService.instance = undefined;
            const b = EmbeddingService.getInstance();
            expect(a).not.toBe(b);
        });
    });

    describe("ensureReady — Promise Lock", () => {
        it("should initialize model and set ready flag", async () => {
            const mockEmbedder = vi.fn().mockResolvedValue({ data: new Float32Array(384).fill(0.5) });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            expect(service.ready).toBe(false);

            await service.ensureReady();
            expect(service.ready).toBe(true);
        });

        it("should call pipeline() only once even with concurrent ensureReady calls (Promise Lock)", async () => {
            const mockEmbedder = vi.fn();
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();

            // Simulate 3 concurrent calls (e.g., MemoryManager + LanceMemory + LearningLog at boot)
            await Promise.all([
                service.ensureReady(),
                service.ensureReady(),
                service.ensureReady(),
            ]);

            // pipeline should be called exactly once (WebGPU attempt + optional CPU fallback)
            // In the mock, it succeeds on first call
            expect(mockPipelineFn).toHaveBeenCalledTimes(1);
        });

        it("should not crash when model init fails", async () => {
            // Must reject BOTH calls: 1st = WebGPU attempt, 2nd = CPU fallback
            mockPipelineFn
                .mockRejectedValueOnce(new Error("WebGPU not available"))
                .mockRejectedValueOnce(new Error("ONNX runtime not found"));

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            // Both paths failed → isReady stays false, embedder is null
            expect(service.ready).toBe(false);
        });
    });

    describe("embed()", () => {
        it("should return 384-dim vector on success", async () => {
            const fakeData = new Float32Array(384);
            fakeData.fill(0.42);
            const mockEmbedder = vi.fn().mockResolvedValue({ data: fakeData });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embed("Hello world");
            expect(vector).toHaveLength(384);
            expect(vector[0]).toBeCloseTo(0.42);
        });

        it("should return dummy vector (384D, 0.01) when embedder is null", async () => {
            // Don't init model → embedder remains null
            mockPipelineFn.mockRejectedValueOnce(new Error("init fail"));

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embed("test");
            expect(vector).toHaveLength(384);
            expect(vector[0]).toBe(0.01);
        });

        it("should return dummy vector when embedding throws", async () => {
            const mockEmbedder = vi.fn().mockRejectedValue(new Error("tensor error"));
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embed("bad input");
            expect(vector).toHaveLength(384);
            expect(vector[0]).toBe(0.01);
        });
    });

    describe("embedWithTimeout()", () => {
        it("should return vector when embedding completes within timeout", async () => {
            const fakeData = new Float32Array(384).fill(0.7);
            const mockEmbedder = vi.fn().mockResolvedValue({ data: fakeData });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embedWithTimeout("test", 5000);
            expect(vector).toHaveLength(384);
            expect(vector[0]).toBeCloseTo(0.7);
        });

        it("should return dummy vector when embedder is null", async () => {
            mockPipelineFn.mockRejectedValueOnce(new Error("init fail"));

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embedWithTimeout("test", 1000);
            expect(vector).toHaveLength(384);
            expect(vector[0]).toBe(0.01);
        });
    });

    describe("dispose()", () => {
        it("should reset all internal state", async () => {
            const mockEmbedder = vi.fn();
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();
            expect(service.ready).toBe(true);

            service.dispose();
            expect(service.ready).toBe(false);
        });

        it("should allow re-initialization after dispose", async () => {
            const mockEmbedder = vi.fn().mockResolvedValue({ data: new Float32Array(384) });
            mockPipelineFn.mockResolvedValue(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();
            expect(service.ready).toBe(true);

            service.dispose();
            expect(service.ready).toBe(false);

            // Re-init should work
            await service.ensureReady();
            expect(service.ready).toBe(true);
        });
    });
});
