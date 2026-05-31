import { describe, it, expect, vi } from "vitest";
import { z } from "zod";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

import {
    safeParse,
    tryParseOrDefault,
    assertParse,
    buildStringSchema,
    optionalWithDefault,
    buildUnionSchema,
    buildObjectSchema,
} from "@utils/ZodHelper";
import { logger } from "../../src/utils/logger";

describe("ZodHelper — Safe Parsing & Schema Builders", () => {
    // ============================================================
    // safeParse()
    // ============================================================
    describe("safeParse()", () => {
        const schema = z.object({ name: z.string(), age: z.number() });

        it("should return success=true for valid data", () => {
            const result = safeParse(schema, { name: "LIVA", age: 1 }, { name: "", age: 0 });
            expect(result.success).toBe(true);
            expect(result.data).toEqual({ name: "LIVA", age: 1 });
        });

        it("should return success=false with fallback for invalid data", () => {
            const fallback = { name: "default", age: 0 };
            const result = safeParse(schema, { name: 123 }, fallback);
            expect(result.success).toBe(false);
            expect(result.data).toEqual(fallback);
        });

        it("should return success=false with ZodError for missing fields", () => {
            const result = safeParse(schema, {}, { name: "", age: 0 });
            expect(result.success).toBe(false);
            expect("error" in result && result.error).toBeTruthy();
        });

        it("should log warning for invalid data", () => {
            safeParse(schema, "wrong", { name: "", age: 0 }, "TestContext");
            expect(logger.warn).toHaveBeenCalled();
        });
    });

    // ============================================================
    // tryParseOrDefault()
    // ============================================================
    describe("tryParseOrDefault()", () => {
        const schema = z.string().min(1);

        it("should return parsed data for valid input", () => {
            expect(tryParseOrDefault(schema, "hello", "fallback")).toBe("hello");
        });

        it("should return default for invalid input", () => {
            expect(tryParseOrDefault(schema, "", "fallback")).toBe("fallback");
        });

        it("should return default for null input", () => {
            expect(tryParseOrDefault(schema, null, "fallback")).toBe("fallback");
        });
    });

    // ============================================================
    // assertParse()
    // ============================================================
    describe("assertParse()", () => {
        const schema = z.object({ x: z.number() });

        it("should return parsed data for valid input", () => {
            const result = assertParse(schema, { x: 42 });
            expect(result).toEqual({ x: 42 });
        });

        it("should throw Error for invalid input", () => {
            expect(() => assertParse(schema, { x: "not number" })).toThrow("Zod assertion failed");
        });

        it("should include context in error message", () => {
            expect(() => assertParse(schema, {}, "MyModule")).toThrow("MyModule");
        });
    });

    // ============================================================
    // buildStringSchema()
    // ============================================================
    describe("buildStringSchema()", () => {
        it("should enforce max length", () => {
            const schema = buildStringSchema(5);
            expect(schema.safeParse("abcde").success).toBe(true);
            expect(schema.safeParse("abcdef").success).toBe(false);
        });

        it("should accept description", () => {
            const schema = buildStringSchema(10, "User name");
            expect(schema.description).toBe("User name");
        });

        it("should use default description if not provided", () => {
            const schema = buildStringSchema(10);
            expect(schema.description).toBe("String");
        });
    });

    // ============================================================
    // optionalWithDefault()
    // ============================================================
    describe("optionalWithDefault()", () => {
        it("should provide default value when input is undefined", () => {
            const schema = optionalWithDefault(z.number(), 42);
            expect(schema.parse(undefined)).toBe(42);
        });

        it("should use provided value when given", () => {
            const schema = optionalWithDefault(z.string(), "default");
            expect(schema.parse("custom")).toBe("custom");
        });
    });

    // ============================================================
    // buildUnionSchema()
    // ============================================================
    describe("buildUnionSchema()", () => {
        it("should create union from 2+ schemas", () => {
            const union = buildUnionSchema("type", {
                a: z.object({ type: z.literal("a"), value: z.number() }),
                b: z.object({ type: z.literal("b"), label: z.string() }),
            });
            expect(union.safeParse({ type: "a", value: 1 }).success).toBe(true);
            expect(union.safeParse({ type: "b", label: "hello" }).success).toBe(true);
        });

        it("should return single schema when only 1 provided", () => {
            const single = buildUnionSchema("type", {
                only: z.object({ type: z.literal("only") }),
            });
            expect(single.safeParse({ type: "only" }).success).toBe(true);
        });

        it("should return z.never() when empty object provided", () => {
            const empty = buildUnionSchema("type", {});
            expect(empty.safeParse("anything").success).toBe(false);
        });
    });

    // ============================================================
    // buildObjectSchema()
    // ============================================================
    describe("buildObjectSchema()", () => {
        it("should create object schema from shape", () => {
            const schema = buildObjectSchema({
                name: z.string(),
                age: z.number(),
            });
            expect(schema.safeParse({ name: "LIVA", age: 1 }).success).toBe(true);
        });

        it("should add description when provided", () => {
            const schema = buildObjectSchema({ x: z.number() }, "Point");
            expect(schema.description).toBe("Point");
        });

        it("should work without description", () => {
            const schema = buildObjectSchema({ x: z.number() });
            expect(schema.safeParse({ x: 1 }).success).toBe(true);
        });
    });
});
