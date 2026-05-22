import { describe, it, expect, vi } from "vitest";
import { safeExtractJSON } from "@utils/JsonExtractor";

// Mock jsonrepair — we import the real one but want to test error path too
vi.mock("jsonrepair", async (importOriginal) => {
    const actual = await importOriginal() as Record<string, unknown>;
    return actual;
});

describe("JsonExtractor — safeExtractJSON()", () => {
    // ============================================================
    // Happy Path: Valid JSON extraction
    // ============================================================
    describe("Valid JSON extraction", () => {
        it("should extract clean JSON object", () => {
            const result = safeExtractJSON<{ name: string }>('{"name": "test"}');
            expect(result).toEqual({ name: "test" });
        });

        it("should extract JSON wrapped in explanatory text", () => {
            const text = 'Here is the result:\n{"status": "ok", "count": 5}\nThat is all.';
            const result = safeExtractJSON<{ status: string; count: number }>(text);
            expect(result).toEqual({ status: "ok", count: 5 });
        });

        it("should extract JSON from markdown code block", () => {
            const text = '```json\n{"key": "value"}\n```';
            const result = safeExtractJSON<{ key: string }>(text);
            expect(result).toEqual({ key: "value" });
        });

        it("should extract nested JSON objects", () => {
            const text = 'Result: {"outer": {"inner": "deep"}, "list": [1, 2, 3]}';
            const result = safeExtractJSON<{ outer: { inner: string }; list: number[] }>(text);
            expect(result).toEqual({ outer: { inner: "deep" }, list: [1, 2, 3] });
        });

        it("should handle JSON with special characters in values", () => {
            const text = '{"message": "Xin chào bạn! Tôi là LIVA 🤖"}';
            const result = safeExtractJSON<{ message: string }>(text);
            expect(result?.message).toBe("Xin chào bạn! Tôi là LIVA 🤖");
        });
    });

    // ============================================================
    // Edge Cases: Malformed JSON (jsonrepair should fix)
    // ============================================================
    describe("Malformed JSON recovery via jsonrepair", () => {
        it("should repair trailing comma", () => {
            const text = '{"a": 1, "b": 2,}';
            const result = safeExtractJSON<{ a: number; b: number }>(text);
            expect(result).toEqual({ a: 1, b: 2 });
        });

        it("should repair single-quoted keys", () => {
            const text = "{'key': 'value'}";
            const result = safeExtractJSON<{ key: string }>(text);
            expect(result).toEqual({ key: "value" });
        });

        it("should repair unquoted keys", () => {
            const text = "{key: \"value\"}";
            const result = safeExtractJSON<{ key: string }>(text);
            expect(result).toEqual({ key: "value" });
        });
    });

    // ============================================================
    // Error Path: Should return null
    // ============================================================
    describe("Error cases → null", () => {
        it("should return null for empty string", () => {
            expect(safeExtractJSON("")).toBeNull();
        });

        it("should return null for string without any braces", () => {
            expect(safeExtractJSON("Hello world, no JSON here")).toBeNull();
        });

        it("should return null for only opening brace", () => {
            expect(safeExtractJSON("{ no closing")).toBeNull();
        });

        it("should return null for only closing brace", () => {
            expect(safeExtractJSON("no opening }")).toBeNull();
        });

        it("should return null when closing brace comes before opening", () => {
            expect(safeExtractJSON("} before { wrong")).toBeNull();
        });

        it("should return null for completely unparseable content", () => {
            expect(safeExtractJSON("{{{{{")).toBeNull();
        });
    });

    // ============================================================
    // Type parameterization
    // ============================================================
    describe("Type safety", () => {
        it("should extract typed result", () => {
            interface MyType { items: string[]; total: number }
            const text = '{"items": ["a", "b"], "total": 2}';
            const result = safeExtractJSON<MyType>(text);
            expect(result?.items).toEqual(["a", "b"]);
            expect(result?.total).toBe(2);
        });
    });
});
