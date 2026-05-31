import { describe, it, expect } from "vitest";
import { validateSkillMetadata, AgentStateSchema } from "../../src/mcp/SkillMetadataSchema";

describe("SkillMetadataSchema", () => {
    describe("validateSkillMetadata", () => {
        it("should accept valid skill metadata", () => {
            const metadata = {
                name: "web_search",
                description: "Search the web for information",
                parameters: {
                    type: "object",
                    properties: { query: { type: "string" } },
                    required: ["query"],
                },
                isCoreSkill: true,
                category: "web",
            };
            const result = validateSkillMetadata(metadata, "test.ts");
            expect(result).not.toBeNull();
            expect(result?.name).toBe("web_search");
        });

        it("should accept minimal valid metadata (name + description only)", () => {
            const metadata = {
                name: "simple_tool",
                description: "A simple tool for testing",
            };
            const result = validateSkillMetadata(metadata, "test.ts");
            expect(result).not.toBeNull();
        });

        it("should reject metadata with empty name", () => {
            const metadata = {
                name: "",
                description: "Some description",
            };
            const result = validateSkillMetadata(metadata, "test.ts");
            expect(result).toBeNull();
        });

        it("should reject metadata with invalid name format (uppercase)", () => {
            const metadata = {
                name: "WebSearch",
                description: "Some description",
            };
            const result = validateSkillMetadata(metadata, "test.ts");
            expect(result).toBeNull();
        });

        it("should reject metadata with short description", () => {
            const metadata = {
                name: "test_tool",
                description: "Hi",
            };
            const result = validateSkillMetadata(metadata, "test.ts");
            expect(result).toBeNull();
        });

        it("should reject metadata missing name entirely", () => {
            const metadata = {
                description: "A description without name",
            };
            const result = validateSkillMetadata(metadata, "test.ts");
            expect(result).toBeNull();
        });

        it("should accept metadata with optional fields", () => {
            const metadata = {
                name: "full_skill",
                description: "A fully-featured skill with all optional fields",
                search_keywords: ["search", "web"],
                isCoreSkill: false,
                category: "core",
                semantic_tags: ["search"],
                requires_hitl: true,
                is_cpu_heavy: false,
            };
            const result = validateSkillMetadata(metadata, "test.ts");
            expect(result).not.toBeNull();
            expect(result?.requires_hitl).toBe(true);
            expect(result?.is_cpu_heavy).toBe(false);
        });

        it("should preserve extra metadata fields via passthrough", () => {
            const metadata = {
                name: "extra_fields_tool",
                description: "A tool with extra custom fields",
                customReasoningTraces: "trace_xyz",
                confidenceScore: 0.98,
            };
            const result = validateSkillMetadata(metadata, "test.ts") as any;
            expect(result).not.toBeNull();
            expect(result.customReasoningTraces).toBe("trace_xyz");
            expect(result.confidenceScore).toBe(0.98);
        });
    });

    describe("AgentStateSchema", () => {
        it("should validate a correct AgentState and preserve extra fields via passthrough", () => {
            const agentState = {
                sessionId: "123e4567-e89b-12d3-a456-426614174000",
                agentId: "agent_alpha",
                status: "THINKING",
                context: { customKey: "customValue" },
                confidence: 0.85,
                lastActionTimestamp: Date.now(),
                extendedMeta: {
                    modelUsed: "gemini",
                    promptTokens: 120,
                }
            };

            const parsed = AgentStateSchema.parse(agentState);
            expect(parsed.sessionId).toBe("123e4567-e89b-12d3-a456-426614174000");
            expect(parsed.extendedMeta).toEqual({
                modelUsed: "gemini",
                promptTokens: 120,
            });
        });

        it("should strip prototype pollution keys during validation", () => {
            const agentState = {
                sessionId: "123e4567-e89b-12d3-a456-426614174000",
                agentId: "agent_alpha",
                status: "THINKING",
                __proto__: { polluted: true },
                constructor: { polluted: true },
                prototype: { polluted: true },
                context: {
                    cleanKey: "cleanValue",
                    __proto__: { pollutedInner: true }
                }
            };

            const parsed = AgentStateSchema.parse(agentState);
            expect(parsed.polluted).toBeUndefined();
            expect(parsed.pollutedInner).toBeUndefined();
            expect(Object.getPrototypeOf(parsed)).toBe(Object.prototype);
            expect(parsed.context.cleanKey).toBe("cleanValue");
        });

        it("should reject states that exceed the 50KB limit", () => {
            const bigData = "a".repeat(60 * 1024); // 60KB
            const agentState = {
                sessionId: "123e4567-e89b-12d3-a456-426614174000",
                agentId: "agent_alpha",
                status: "THINKING",
                context: {
                    bigPayload: bigData
                }
            };

            const result = AgentStateSchema.safeParse(agentState);
            expect(result.success).toBe(false);
            if (!result.success) {
                expect(result.error.message).toContain("size exceeds 50KB");
            }
        });
    });
});
