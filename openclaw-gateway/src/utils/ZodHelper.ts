/**
 * @module ZodHelper
 * Shadow Mode Zod - Safe parsing without crashing runtime
 * =====================================================
 * Phase 3: Type Safety Improvements
 * 
 * Usage:
 *   import { safeParse, tryParseOrDefault } from "../utils/ZodHelper";
 *   
 *   // Shadow mode - logs warning but doesn't crash
 *   const result = safeParse(MySchema, externalData);
 *   if (result.success) {
 *     return result.data;
 *   }
 *   
 *   // With default fallback
 *   const data = tryParseOrDefault(MySchema, externalData, defaultValue);
 */

import { z, ZodError, ZodSchema } from "zod";
import { logger } from "./logger";

export type SafeParseResult<T> =
    | { success: true; data: T }
    | { success: false; data: unknown; error: ZodError };

/**
 * Safe parse with Shadow Mode - logs warning but doesn't crash
 * Use this for parsing external API responses or untrusted data
 */
export function safeParse<T>(
    schema: ZodSchema<T>,
    data: unknown,
    fallbackData: T,
    context?: string
): SafeParseResult<T> {
    const result = schema.safeParse(data);
    
    if (result.success) {
        return { success: true, data: result.data };
    }
    
    // Shadow mode: log warning but return fallback data
    const schemaName = schema.description || schema._def.typeName || "Unknown";
    logger.warn({
        context: "ZodSafeParse",
        schemaName,
        error: result.error.message,
        path: result.error.issues.map(i => i.path.join(".")).join(", "),
    }, `Type mismatch detected${context ? ` in ${context}` : ""}, using fallback`);
    
    return { success: false, data: fallbackData, error: result.error };
}

/**
 * Try to parse or return default value
 * Convenience wrapper around safeParse
 */
export function tryParseOrDefault<T>(
    schema: ZodSchema<T>,
    data: unknown,
    defaultValue: T,
    context?: string
): T {
    const result = safeParse(schema, data, defaultValue, context);
    return result.data;
}

/**
 * Assert parse - throws on failure (use only for internal trusted data)
 * Prefer safeParse for external/untrusted data
 */
export function assertParse<T>(schema: ZodSchema<T>, data: unknown, context?: string): T {
    const result = schema.safeParse(data);
    if (!result.success) {
        const schemaName = schema.description || schema._def.typeName || "Unknown";
        const error = new Error(
            `Zod assertion failed${context ? ` in ${context}` : ""}: ${result.error.message}`
        );
        logger.error({
            context: "ZodAssert",
            schemaName,
            error: result.error.message,
            issues: result.error.issues,
        }, error.message);
        throw error;
    }
    return result.data;
}

// ============================================================
// Common Schema Builders
// ============================================================

/**
 * Build a string schema with max length
 */
export function buildStringSchema(maxLength: number, description?: string) {
    return z.string().max(maxLength, `Max ${maxLength} chars`).describe(description || "String");
}

/**
 * Build a schema for optional fields with defaults
 */
export function optionalWithDefault<T>(
    schema: ZodSchema<T>,
    defaultValue: T
): ZodSchema<T> {
    return schema.optional().default(defaultValue) as unknown as ZodSchema<T>;
}

/**
 * Build a discriminated union schema
 */
export function buildUnionSchema<
    T extends Record<string, ZodSchema<unknown>>
>(
    discriminant: keyof T,
    schemas: T
): ZodSchema<z.infer<T[keyof T]>> {
    return z.discriminatedUnion(
        discriminant as string,
        Object.values(schemas) as [ZodSchema<z.infer<T[keyof T]>>, ...ZodSchema<z.infer<T[keyof T]>>[]]
    );
}

/**
 * Build an object schema with required fields
 */
export function buildObjectSchema<T extends Record<string, ZodSchema<unknown>>>(
    shape: T,
    description?: string
): ZodSchema<z.infer<z.ZodObject<T>>> {
    const schema = z.object(shape);
    return description ? schema.describe(description) : schema;
}
