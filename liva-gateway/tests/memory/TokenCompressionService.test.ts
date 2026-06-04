import { describe, it, expect, beforeEach } from "vitest";
import { TokenCompressionService, estimateTokens } from "../../src/memory/TokenCompressionService";

describe("TokenCompressionService", () => {
    let service: TokenCompressionService;

    beforeEach(() => {
        service = TokenCompressionService.getInstance();
    });

    // ════════════════════════════════════
    //  estimateTokens()
    // ════════════════════════════════════
    describe("estimateTokens()", () => {
        it("should estimate tokens as wordCount × 1.5", () => {
            expect(estimateTokens("Hello world")).toBe(3);
        });

        it("should return 0 for empty string", () => {
            expect(estimateTokens("")).toBe(0);
        });

        it("should handle whitespace-only strings", () => {
            expect(estimateTokens("   \n\t  ")).toBe(0);
        });

        it("should handle multi-word Vietnamese text", () => {
            const tokens = estimateTokens("Xin chào thế giới này");
            expect(tokens).toBeGreaterThanOrEqual(7);
        });
    });

    // ════════════════════════════════════
    //  compress() — Stage 1: Structural Strip
    // ════════════════════════════════════
    describe("compress() — Stage 1: Structural Strip", () => {
        it("should collapse multiple whitespace into single spaces", async () => {
            const input = "Hello    world   \n\n\n   test    data";
            const result = await service.compress(input, 1.0);
            expect(result.compressedText.length).toBeLessThanOrEqual(input.length);
            // Should not contain consecutive spaces
            expect(result.compressedText).not.toMatch(/  +/);
        });

        it("should merge repeated separator lines", async () => {
            const input = "Section A\n---\n---\n---\nSection B\n===\n===\n===\nSection C";
            const result = await service.compress(input, 1.0);
            // Repeated separators should be collapsed to one
            expect((result.compressedText.match(/---/g) || []).length).toBeLessThanOrEqual(1);
        });
    });

    // ════════════════════════════════════
    //  compress() — Stage 2: JSON/XML Condensation
    // ════════════════════════════════════
    describe("compress() — Stage 2: JSON/XML Condensation", () => {
        it("should condense large JSON arrays to schema + first/last", async () => {
            const items = Array.from({ length: 20 }, (_, i) => `{"id": ${i}, "name": "item_${i}"}`);
            const input = `Here is data: [${items.join(", ")}]`;
            const result = await service.compress(input, 0.6);
            expect(result.compressedText.length).toBeLessThan(input.length);
        });
    });

    // ════════════════════════════════════
    //  compress() — Stage 3: Sentence Deduplication
    // ════════════════════════════════════
    describe("compress() — Stage 3: Sentence Deduplication", () => {
        it("should remove near-duplicate sentences (Jaccard ≥ 0.8)", async () => {
            const input = [
                "The user asked about weather in Hanoi today.",
                "The user asked about weather in Hanoi today please.",
                "LIVA responded with temperature data for Hanoi.",
                "LIVA responded with the temperature data for Hanoi.",
                "The system is functioning normally and all services are running.",
            ].join("\n");
            const result = await service.compress(input, 0.8);
            const originalLines = input.split("\n").filter(l => l.trim()).length;
            const resultLines = result.compressedText.split("\n").filter(l => l.trim()).length;
            expect(resultLines).toBeLessThan(originalLines);
        });
    });

    // ════════════════════════════════════
    //  compress() — Stage 4: Budget Enforcement
    // ════════════════════════════════════
    describe("compress() — Stage 4: Budget Enforcement", () => {
        it("should truncate to budget when targetRatio is very small", async () => {
            const longText = Array.from({ length: 100 }, (_, i) =>
                `Line ${i}: This is a sample line of text that represents context data.`
            ).join("\n");
            const result = await service.compress(longText, 0.3);
            const originalTokens = estimateTokens(longText);
            expect(result.compressedTokens).toBeLessThanOrEqual(originalTokens * 0.4);
        });
    });

    // ════════════════════════════════════
    //  compress() — Edge Cases
    // ════════════════════════════════════
    describe("compress() — Edge Cases", () => {
        it("should return empty result for empty input", async () => {
            const result = await service.compress("", 0.6);
            expect(result.compressedText).toBe("");
            expect(result.originalTokens).toBe(0);
            expect(result.compressedTokens).toBe(0);
            expect(result.strategy).toBe("none");
        });

        it("should preserve short text content", async () => {
            const short = "Hello world";
            const result = await service.compress(short, 0.6);
            // Short text: original tokens ≤ budget, so no heavy compression
            expect(result.compressedText).toContain("Hello");
            expect(result.compressedText).toContain("world");
        });

        it("should handle pure code blocks without breaking syntax", async () => {
            const code = "```typescript\nfunction hello() {\n  return 'world';\n}\n```\nThis is the main function that returns a greeting value for the user.";
            const result = await service.compress(code, 0.9);
            expect(result.compressedText).toContain("function");
        });

        it("should return compression metadata", async () => {
            const input = "This is a test sentence that should be analyzed for compression.";
            const result = await service.compress(input, 1.0);
            expect(result.originalTokens).toBeGreaterThan(0);
            expect(result.compressedTokens).toBeGreaterThan(0);
            expect(result.compressionRatio).toBeGreaterThan(0);
            expect(result.compressionRatio).toBeLessThanOrEqual(1);
        });
    });

    // ════════════════════════════════════
    //  Full Pipeline Integration
    // ════════════════════════════════════
    describe("Full Pipeline", () => {
        it("should achieve meaningful compression on realistic context", async () => {
            const lines: string[] = [];
            for (let i = 0; i < 50; i++) {
                lines.push(`[Turn ${i}] User said: This is turn number ${i} of the conversation.`);
                lines.push(`[Turn ${i}] Assistant replied: I understand, this is response number ${i}.`);
            }
            lines.push("[Turn 51] User said: This is turn number 51 of the conversation.");
            lines.push("[Turn 52] User said: This is turn number 52 of the conversation.");

            const input = lines.join("\n");
            const result = await service.compress(input, 0.6);

            expect(result.compressedTokens).toBeLessThan(result.originalTokens * 0.7);
            expect(result.compressedTokens).toBeGreaterThan(0);
            expect(result.compressedText.length).toBeGreaterThan(0);
        });
    });
});
