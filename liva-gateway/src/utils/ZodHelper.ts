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

import { z, ZodError, ZodSchema, ZodTypeAny, ZodObject, ZodRawShape } from "zod";
import { logger } from "./logger";

export type SafeParseResult<T> =
    | { success: true; data: T }
    | { success: false; data: unknown; error: ZodError };

/**
 * Extract a human-readable schema name for logging.
 * Zod v4 removed `_def.typeName` — use `.description` or fallback to "Unknown".
 */
function getSchemaName(schema: ZodSchema<unknown>): string {
    return schema.description || "Unknown";
}

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
    const schemaName = getSchemaName(schema);
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
    return result.data as T;
}

/**
 * Assert parse - throws on failure (use only for internal trusted data)
 * Prefer safeParse for external/untrusted data
 */
export function assertParse<T>(schema: ZodSchema<T>, data: unknown, context?: string): T {
    const result = schema.safeParse(data);
    if (!result.success) {
        const schemaName = getSchemaName(schema);
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
// Agentic Metadata Preservation & Envelope Pattern
// ============================================================

// Định nghĩa chuẩn Envelope Pattern cho toàn hệ thống
export const EnvelopeSchema = z.object({
  payload: z.any(),
  metadata: z.record(z.string(), z.any()).optional().default({}),
});

export type Envelope = z.infer<typeof EnvelopeSchema>;

/**
 * Tạo Schema an toàn cho Agentic Tools.
 * Cơ chế passthrough() đảm bảo không lột bỏ (strip) các tham số context động.
 */
export function createAgenticSchema<T extends ZodRawShape>(
  shape: T
) {
  return z.object(shape).passthrough();
}

/**
 * Phân tích an toàn sử dụng Envelope Pattern.
 * Đầu vào có thể là raw data của LLM, hàm sẽ tách Payload ra để validate
 * và giữ nguyên vẹn mọi thuộc tính khác vào Metadata.
 */
export function safeParsePreserve(
  schema: ZodTypeAny, 
  data: unknown,
  context?: string
): { success: boolean; data?: Envelope; error?: any } {
  // Bước 1: Extract động mọi trường không thuộc schema chính
  const rawObject = (typeof data === 'object' && data !== null) ? data as Record<string, any> : {};
  let preservingSchema: ZodTypeAny = schema;
  
  if (schema instanceof ZodObject) {
     preservingSchema = schema.passthrough() as unknown as ZodTypeAny;
  }

  // Xác định các keys thuộc về payload hợp lệ (Dựa trên schema)
  const schemaKeys = schema instanceof ZodObject ? Object.keys(schema.shape) : [];
  
  // Bước 2: Validate phần cốt lõi
  const result = preservingSchema.safeParse(data);
  
  if (!result.success) {
    const schemaName = schema.description || "Unknown";
    logger.error({
        context: "ZodSafeParsePreserve",
        schemaName,
        error: result.error.message,
        path: result.error.issues.map((i: any) => i.path.join(".")).join(", "),
    }, `Validation failed${context ? ` in ${context}` : ""}. Potential LLM hallucination or strip`);
    return { success: false, error: result.error };
  }

  // Bước 3: Đóng gói (Envelope) lại
  const validatedPayload = result.data;
  const dynamicMetadata: Record<string, any> = {};

  // Gom nhặt mọi keys thừa (hallucinations, meta, traceId...) đưa vào metadata
  for (const key in rawObject) {
    if (!schemaKeys.includes(key) && key !== 'payload') {
      dynamicMetadata[key] = rawObject[key];
    }
  }

  const finalEnvelope: Envelope = {
    payload: validatedPayload,
    metadata: Object.keys(dynamicMetadata).length > 0 ? dynamicMetadata : (rawObject['_meta'] || {})
  };

  return { success: true, data: finalEnvelope };
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
    defaultValue: NoInfer<T>
): ZodSchema<T> {
    return schema.optional().default(defaultValue as never) as unknown as ZodSchema<T>;
}

/**
 * Build a union schema from a record of schemas.
 * Uses z.union for broad compatibility (Zod v4 discriminatedUnion requires strict $ZodTypeDiscriminable).
 */
export function buildUnionSchema<
    T extends Record<string, ZodSchema>
>(
    _discriminant: keyof T,
    schemas: T
): ZodSchema {
    const values = Object.values(schemas);
    if (values.length < 2) {
        return values[0] ?? z.never();
    }
    return z.union(
        [values[0], values[1], ...values.slice(2)] as [ZodSchema, ZodSchema, ...ZodSchema[]]
    );
}

/**
 * Build an object schema with required fields
 */
export function buildObjectSchema<T extends z.ZodRawShape>(
    shape: T,
    description?: string
): z.ZodObject<T> {
    const schema = z.object(shape);
    return description ? schema.describe(description) : schema;
}

/**
 * Recursively removes Prototype Pollution vectors like '__proto__', 'constructor', and 'prototype'
 * from any object, array, or nested values.
 */
export function sanitizeMetadata(data: unknown): unknown {
    if (data === null || data === undefined) {
        return data;
    }
    if (Array.isArray(data)) {
        return data.map(sanitizeMetadata);
    }
    if (typeof data === "object") {
        const cleanObj: Record<string, unknown> = {};
        for (const [key, value] of Object.entries(data)) {
            if (key === "__proto__" || key === "constructor" || key === "prototype") {
                logger.warn({ key }, `[Security] Strip Prototype Pollution vector during sanitization`);
                continue;
            }
            cleanObj[key] = sanitizeMetadata(value);
        }
        return cleanObj;
    }
    return data;
}

export const AgentStateSchema = z.preprocess(
    (val) => sanitizeMetadata(val),
    z.object({
        sessionId: z.string().uuid(),
        agentId: z.string(),
        status: z.enum(['IDLE', 'THINKING', 'ACTING', 'ERROR']),
        context: z.record(z.string(), z.any()).optional().default({}),
        confidence: z.number().min(0).max(1).optional(),
        lastActionTimestamp: z.number().optional()
    }).passthrough()
).refine((data) => {
    try {
        const str = JSON.stringify(data);
        return Buffer.byteLength(str, 'utf-8') <= 50 * 1024;
    } catch {
        return false;
    }
}, { message: "Metadata payload size exceeds 50KB limit" });

export type AgentState = z.infer<typeof AgentStateSchema>;

