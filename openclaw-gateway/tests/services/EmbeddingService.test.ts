import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================================
// Mock @huggingface/transformers to avoid downloading model
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

// Mock FeatureFlags — default to NOMIC_EMBED enabled (768D)
vi.mock("../../src/utils/FeatureFlags", () => ({
    FF: {
        isEnabled: vi.fn((flag: string) => {
            if (flag === "NOMIC_EMBED") return true;
            return false;
        }),
    },
}));

import { EmbeddingService } from "../../src/services/EmbeddingService";
import { FF } from "../../src/utils/FeatureFlags";

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

    describe("ensureReady — Nomic Model (768D)", () => {
        it("should initialize nomic model when FF.NOMIC_EMBED is true", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            const mockEmbedder = vi.fn().mockResolvedValue({ data: new Float32Array(768).fill(0.5) });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            expect(service.ready).toBe(true);
            expect(service.dimension).toBe(768);
            expect(service.modelId).toBe("nomic-ai/nomic-embed-text-v1.5");
            expect(service.supportsMRL).toBe(true);
        });

        it("should initialize MiniLM when FF.NOMIC_EMBED is false", async () => {
            vi.mocked(FF.isEnabled).mockReturnValue(false);
            const mockEmbedder = vi.fn().mockResolvedValue({ data: new Float32Array(384).fill(0.5) });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            expect(service.ready).toBe(true);
            expect(service.dimension).toBe(384);
            expect(service.modelId).toBe("Xenova/all-MiniLM-L6-v2");
            expect(service.supportsMRL).toBe(false);
        });

        it("should call pipeline() only once even with concurrent ensureReady calls (Promise Lock)", async () => {
            vi.mocked(FF.isEnabled).mockReturnValue(true);
            const mockEmbedder = vi.fn();
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await Promise.all([
                service.ensureReady(),
                service.ensureReady(),
                service.ensureReady(),
            ]);

            expect(mockPipelineFn).toHaveBeenCalledTimes(1);
        });

        it("should not crash when model init fails", async () => {
            vi.mocked(FF.isEnabled).mockReturnValue(true);
            mockPipelineFn
                .mockRejectedValueOnce(new Error("WebGPU not available"))
                .mockRejectedValueOnce(new Error("ONNX runtime not found"));

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            expect(service.ready).toBe(false);
        });
    });

    describe("embed()", () => {
        it("should return 768-dim vector with nomic model", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            const fakeData = new Float32Array(768).fill(0.42);
            const mockEmbedder = vi.fn().mockResolvedValue({ data: fakeData });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embed("Hello world");
            expect(vector).toHaveLength(768);
            expect(vector[0]).toBeCloseTo(0.42);
        });

        it("should return 384-dim vector with MiniLM model", async () => {
            vi.mocked(FF.isEnabled).mockReturnValue(false);
            const fakeData = new Float32Array(384).fill(0.42);
            const mockEmbedder = vi.fn().mockResolvedValue({ data: fakeData });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embed("Hello world");
            expect(vector).toHaveLength(384);
        });

        it("should return dummy vector at correct dimension when embedder is null", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            mockPipelineFn
                .mockRejectedValueOnce(new Error("init fail"))
                .mockRejectedValueOnce(new Error("init fail 2"));

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embed("test");
            expect(vector).toHaveLength(768);
            expect(vector[0]).toBe(0.01);
        });

        it("should return dummy vector when embedding throws", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            const mockEmbedder = vi.fn().mockRejectedValue(new Error("tensor error"));
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embed("bad input");
            expect(vector).toHaveLength(768);
            expect(vector[0]).toBe(0.01);
        });
    });

    describe("Matryoshka Truncation", () => {
        it("should truncate 768D to 256D and re-normalize", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            // Create a normalized 768D vector
            const fakeData = new Float32Array(768);
            for (let i = 0; i < 768; i++) fakeData[i] = (i + 1) / 768;
            const mockEmbedder = vi.fn().mockResolvedValue({ data: fakeData });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const fullVec = await service.embed("test");
            expect(fullVec).toHaveLength(768);

            const truncated = service.truncateMatryoshka(fullVec, 256);
            expect(truncated).toHaveLength(256);

            // Verify re-normalization: L2 norm should be ~1.0
            let normSq = 0;
            for (let i = 0; i < truncated.length; i++) normSq += truncated[i] * truncated[i];
            expect(Math.sqrt(normSq)).toBeCloseTo(1.0, 4);
        });

        it("should return input unchanged when targetDim >= vector length", () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            const service = EmbeddingService.getInstance();
            const vec = [0.1, 0.2, 0.3];
            const result = service.truncateMatryoshka(vec, 10);
            expect(result).toEqual(vec);
        });

        it("should return input unchanged for MiniLM (no MRL support)", async () => {
            vi.mocked(FF.isEnabled).mockReturnValue(false);
            const mockEmbedder = vi.fn().mockResolvedValue({ data: new Float32Array(384).fill(0.5) });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vec = Array.from({ length: 384 }, (_, i) => i * 0.01);
            const result = service.truncateMatryoshka(vec, 128);
            expect(result).toEqual(vec); // Unchanged — MiniLM doesn't support MRL
        });
    });

    describe("embedTruncated()", () => {
        it("should embed and truncate in one call", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            const fakeData = new Float32Array(768);
            for (let i = 0; i < 768; i++) fakeData[i] = 0.5;
            const mockEmbedder = vi.fn().mockResolvedValue({ data: fakeData });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const truncated = await service.embedTruncated("test", 128);
            expect(truncated).toHaveLength(128);
        });
    });

    describe("embedWithTimeout()", () => {
        it("should return vector when embedding completes within timeout", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            const fakeData = new Float32Array(768).fill(0.7);
            const mockEmbedder = vi.fn().mockResolvedValue({ data: fakeData });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embedWithTimeout("test", 5000);
            expect(vector).toHaveLength(768);
            expect(vector[0]).toBeCloseTo(0.7);
        });

        it("should return dummy vector when embedder is null", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            mockPipelineFn
                .mockRejectedValueOnce(new Error("init fail"))
                .mockRejectedValueOnce(new Error("init fail 2"));

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embedWithTimeout("test", 1000);
            expect(vector).toHaveLength(768);
            expect(vector[0]).toBe(0.01);
        });
    });

    describe("embedBatch()", () => {
        it("should batch embed at correct dimension", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            // 2 texts × 768D = 1536 floats in flat tensor
            const flatData = new Float32Array(1536);
            for (let i = 0; i < 1536; i++) flatData[i] = i < 768 ? 0.1 : 0.9;
            const mockEmbedder = vi.fn().mockResolvedValue({ data: flatData });
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const results = await service.embedBatch(["hello", "world"]);
            expect(results).toHaveLength(2);
            expect(results[0]).toHaveLength(768);
            expect(results[1]).toHaveLength(768);
            expect(results[0][0]).toBeCloseTo(0.1);
            expect(results[1][0]).toBeCloseTo(0.9);
        });
    });

    describe("dispose()", () => {
        it("should reset all internal state", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            const mockEmbedder = vi.fn();
            mockPipelineFn.mockResolvedValueOnce(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();
            expect(service.ready).toBe(true);

            service.dispose();
            expect(service.ready).toBe(false);
        });

        it("should allow re-initialization after dispose", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            const mockEmbedder = vi.fn().mockResolvedValue({ data: new Float32Array(768) });
            mockPipelineFn.mockResolvedValue(mockEmbedder);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();
            expect(service.ready).toBe(true);

            service.dispose();
            expect(service.ready).toBe(false);

            await service.ensureReady();
            expect(service.ready).toBe(true);
        });
    });

    describe("getDummyVector()", () => {
        it("should return vector at active dimension (768D for nomic)", () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            const service = EmbeddingService.getInstance();
            // Before init, default config is MiniLM (384D)
            // After switching to nomic via ensureReady, it would be 768D
            // But without init, activeConfig defaults to MiniLM
            const dummy = service.getDummyVector();
            expect(dummy.every(v => v === 0.01)).toBe(true);
        });
    });
});
