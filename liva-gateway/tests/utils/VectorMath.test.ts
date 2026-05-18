import { describe, it, expect } from "vitest";
import { cosineSimilarity, cosineSimilarityF32 } from "../../src/utils/VectorMath";

describe("VectorMath", () => {
    describe("cosineSimilarity", () => {
        it("should return 1 for identical vectors", () => {
            expect(cosineSimilarity([1, 2, 3], [1, 2, 3])).toBeCloseTo(1);
        });

        it("should return 0 for orthogonal vectors", () => {
            expect(cosineSimilarity([1, 0], [0, 1])).toBe(0);
        });

        it("should return -1 for opposite vectors", () => {
            expect(cosineSimilarity([1, 2, 3], [-1, -2, -3])).toBeCloseTo(-1);
        });

        it("should return 0 if either vector has 0 norm", () => {
            expect(cosineSimilarity([0, 0], [1, 2])).toBe(0);
            expect(cosineSimilarity([1, 2], [0, 0])).toBe(0);
        });

        it("should return 0 if arrays have different lengths", () => {
            expect(cosineSimilarity([1, 2], [1, 2, 3])).toBe(0);
        });

        it("should return 0 if arrays are empty", () => {
            expect(cosineSimilarity([], [])).toBe(0);
        });
    });

    describe("cosineSimilarityF32", () => {
        it("should compute correctly with remainder (len % 4 !== 0)", () => {
            const a = new Float32Array([1, 2, 3, 4, 5]);
            const b = new Float32Array([1, 2, 3, 4, 5]);
            expect(cosineSimilarityF32(a, b)).toBeCloseTo(1);
        });

        it("should compute correctly without remainder (len % 4 === 0)", () => {
            const a = new Float32Array([1, 2, 3, 4]);
            const b = new Float32Array([1, 2, 3, 4]);
            expect(cosineSimilarityF32(a, b)).toBeCloseTo(1);
        });

        it("should return 0 if arrays have different lengths", () => {
            const a = new Float32Array([1, 2]);
            const b = new Float32Array([1, 2, 3]);
            expect(cosineSimilarityF32(a, b)).toBe(0);
        });

        it("should return 0 for zero vectors", () => {
            const a = new Float32Array([0, 0, 0, 0]);
            const b = new Float32Array([1, 2, 3, 4]);
            expect(cosineSimilarityF32(a, b)).toBe(0);
        });
        
        it("should return 0 if arrays are empty", () => {
            expect(cosineSimilarityF32(new Float32Array([]), new Float32Array([]))).toBe(0);
        });
    });
});
