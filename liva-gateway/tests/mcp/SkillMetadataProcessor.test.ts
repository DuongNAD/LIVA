import { describe, it, expect, beforeEach } from "vitest";
import { z } from "zod";
import { SkillMetadataProcessor, BaseMetadata } from "../../src/skills/SkillMetadata";

describe("SkillMetadataProcessor — Metadata Preservation & Strict Mode", () => {
    let processor: SkillMetadataProcessor;
    let sampleMeta: BaseMetadata;

    beforeEach(() => {
        processor = new SkillMetadataProcessor();
        sampleMeta = {
            name: "send_zalo_message",
            description: "Send a message via Zalo",
            version: "1.0.0",
            category: "social",
            strictMode: true
        };
    });

    it("should preserve meta parameters on schema properties", () => {
        const shape = {
            phoneNumber: z.string().describe("Phone number to send message to"),
            message: z.string().describe("Message text content"),
            isUrgent: z.boolean().optional().describe("Mark message as high priority")
        };

        const compiled = processor.preserveAndValidateSchema(sampleMeta, shape);
        expect(compiled).toBeDefined();

        const preserved = processor.getPreservedMeta(compiled);
        expect(preserved).not.toBeNull();
        expect(preserved?.name).toBe("send_zalo_message");
        expect(preserved?.description).toBe("Send a message via Zalo");
        expect(preserved?.compiledAt).toBeDefined();
        expect(preserved?.jsonManifest).toEqual({
            type: "function",
            function: {
                name: "send_zalo_message",
                description: "Send a message via Zalo",
                parameters: {
                    type: "object",
                    properties: {
                        phoneNumber: { type: "string", description: "Phone number to send message to" },
                        message: { type: "string", description: "Message text content" },
                        isUrgent: { type: "boolean", description: "Mark message as high priority" }
                    },
                    required: ["phoneNumber", "message"]
                }
            }
        });
    });

    it("should enforce strictMode schema validation correctly", () => {
        const shape = {
            name: z.string()
        };

        const compiled = processor.preserveAndValidateSchema(sampleMeta, shape);

        // Valid data should pass
        const validResult = compiled.safeParse({ name: "Alice" });
        expect(validResult.success).toBe(true);

        // Extra property in strictMode should fail
        const invalidResult = compiled.safeParse({ name: "Alice", age: 30 });
        expect(invalidResult.success).toBe(false);
        if (!invalidResult.success) {
            const hasStrictViolation = invalidResult.error.issues.some(
                issue => issue.message.includes("Strict Mode Violation")
            );
            expect(hasStrictViolation).toBe(true);
        }
    });

    it("should allow extra properties if strictMode is false", () => {
        const laxMeta = { ...sampleMeta, strictMode: false };
        const shape = {
            name: z.string()
        };

        const compiled = processor.preserveAndValidateSchema(laxMeta, shape);
        const result = compiled.safeParse({ name: "Alice", age: 30 });
        expect(result.success).toBe(true);
    });
});
