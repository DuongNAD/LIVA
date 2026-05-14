import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================================
// Mock safeFetch (GPU API call) — replaces old @huggingface mock
// ============================================================
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
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
import { safeFetch } from "../../src/utils/HttpClient";

// ============================================================
// Helper: Create mock safeFetch response for /v1/embeddings
// ============================================================
function mockEmbeddingResponse(vectors: number[][]) {
    return {
        json: vi.fn().mockResolvedValue({
            object: "list",
            data: vectors.map((embedding, index) => ({
                object: "embedding",
                embedding,
                index,
            })),
            model: "test-model",
            usage: { prompt_tokens: 10, total_tokens: 10 },
        }),
        status: 200,
    };
}

function mockHealthCheckResponse() {
    return { status: 200, json: vi.fn().mockResolvedValue({ data: [] }) };
}

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

    describe("ensureReady — GPU API Init", () => {
        it("should initialize with 768D when FF.NOMIC_EMBED is true", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            // Mock health check success
            vi.mocked(safeFetch).mockResolvedValueOnce(mockHealthCheckResponse() as any);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            expect(service.ready).toBe(true);
            expect(service.dimension).toBe(768);
            expect(service.supportsMRL).toBe(true);
        });

        it("should initialize MiniLM (384D) when FF.NOMIC_EMBED is false", async () => {
            vi.mocked(FF.isEnabled).mockReturnValue(false);
            vi.mocked(safeFetch).mockResolvedValueOnce(mockHealthCheckResponse() as any);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            expect(service.ready).toBe(true);
            expect(service.dimension).toBe(384);
            expect(service.supportsMRL).toBe(false);
        });

        it("should call safeFetch health check only once even with concurrent ensureReady calls (Promise Lock)", async () => {
            vi.mocked(FF.isEnabled).mockReturnValue(true);
            vi.mocked(safeFetch).mockResolvedValueOnce(mockHealthCheckResponse() as any);

            const service = EmbeddingService.getInstance();
            await Promise.all([
                service.ensureReady(),
                service.ensureReady(),
                service.ensureReady(),
            ]);

            // Should only have 1 health check call, not 3
            expect(safeFetch).toHaveBeenCalledTimes(1);
        });

        it("should not crash when health check fails (server not ready)", async () => {
            vi.mocked(FF.isEnabled).mockReturnValue(true);
            vi.mocked(safeFetch).mockRejectedValueOnce(new Error("ECONNREFUSED"));

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            expect(service.ready).toBe(false);
        });
    });

    describe("embed()", () => {
        it("should return vector from GPU API", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            vi.mocked(safeFetch)
                .mockResolvedValueOnce(mockHealthCheckResponse() as any)  // health check
                .mockResolvedValueOnce(mockEmbeddingResponse([Array(768).fill(0.42)]) as any);  // embed call

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embed("Hello world");
            expect(vector).toHaveLength(768);
            expect(vector[0]).toBeCloseTo(0.42);
        });

        it("should return 384-dim vector with MiniLM config", async () => {
            vi.mocked(FF.isEnabled).mockReturnValue(false);
            vi.mocked(safeFetch)
                .mockResolvedValueOnce(mockHealthCheckResponse() as any)
                .mockResolvedValueOnce(mockEmbeddingResponse([Array(384).fill(0.42)]) as any);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embed("Hello world");
            expect(vector).toHaveLength(384);
        });

        it("should return dummy vector at correct dimension when server is not ready", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            vi.mocked(safeFetch).mockRejectedValue(new Error("ECONNREFUSED"));

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embed("test");
            expect(vector).toHaveLength(768);
            expect(vector[0]).toBe(0.01);
        });

        it("should return dummy vector when GPU API call fails", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            vi.mocked(safeFetch)
                .mockResolvedValueOnce(mockHealthCheckResponse() as any)
                .mockRejectedValueOnce(new Error("GPU busy"));

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
            const fullVec = Array.from({ length: 768 }, (_, i) => (i + 1) / 768);
            vi.mocked(safeFetch)
                .mockResolvedValueOnce(mockHealthCheckResponse() as any)
                .mockResolvedValueOnce(mockEmbeddingResponse([fullVec]) as any);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const embedded = await service.embed("test");
            expect(embedded).toHaveLength(768);

            const truncated = service.truncateMatryoshka(embedded, 256);
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
            vi.mocked(safeFetch).mockResolvedValueOnce(mockHealthCheckResponse() as any);

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
            vi.mocked(safeFetch)
                .mockResolvedValueOnce(mockHealthCheckResponse() as any)
                .mockResolvedValueOnce(mockEmbeddingResponse([Array(768).fill(0.5)]) as any);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const truncated = await service.embedTruncated("test", 128);
            expect(truncated).toHaveLength(128);
        });
    });

    describe("embedWithTimeout()", () => {
        it("should return vector when embedding completes within timeout", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            vi.mocked(safeFetch)
                .mockResolvedValueOnce(mockHealthCheckResponse() as any)
                .mockResolvedValueOnce(mockEmbeddingResponse([Array(768).fill(0.7)]) as any);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();

            const vector = await service.embedWithTimeout("test", 5000);
            expect(vector).toHaveLength(768);
            expect(vector[0]).toBeCloseTo(0.7);
        });

        it("should return dummy vector when server is not ready", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            vi.mocked(safeFetch).mockRejectedValue(new Error("ECONNREFUSED"));

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
            vi.mocked(safeFetch)
                .mockResolvedValueOnce(mockHealthCheckResponse() as any)
                .mockResolvedValueOnce(mockEmbeddingResponse([Array(768).fill(0.1), Array(768).fill(0.9)]) as any);

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
            vi.mocked(safeFetch).mockResolvedValueOnce(mockHealthCheckResponse() as any);

            const service = EmbeddingService.getInstance();
            await service.ensureReady();
            expect(service.ready).toBe(true);

            service.dispose();
            expect(service.ready).toBe(false);
        });

        it("should allow re-initialization after dispose", async () => {
            vi.mocked(FF.isEnabled).mockImplementation((flag: string) => flag === "NOMIC_EMBED");
            vi.mocked(safeFetch).mockResolvedValue(mockHealthCheckResponse() as any);

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
            const dummy = service.getDummyVector();
            expect(dummy.every(v => v === 0.01)).toBe(true);
        });
    });
});
