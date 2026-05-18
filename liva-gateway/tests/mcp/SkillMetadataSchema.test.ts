import { describe, it, expect, vi } from "vitest";
import { validateSkillMetadata } from "../../src/mcp/SkillMetadataSchema";

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
    });
});
