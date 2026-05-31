import { describe, expect, it } from "vitest";
import {
  SkillsLockDataSchema,
  TechStackSchema,
  ToolMappingSchema,
} from "../../src/skills/autoskills-types";

describe("autoskills-types", () => {
  it("should parse valid tech stack values", () => {
    expect(TechStackSchema.parse("nodejs")).toBe("nodejs");
    expect(TechStackSchema.parse("unknown")).toBe("unknown");
  });

  it("should reject unknown tech stack values", () => {
    expect(() => TechStackSchema.parse("laravel")).toThrow();
  });

  it("should parse a local tool mapping", () => {
    const mapping = ToolMappingSchema.parse({
      toolName: "gitnexus_query",
      version: "latest",
      source: "local",
      description: "Semantic search",
      kit: "DEVOPS_KIT",
    });

    expect(mapping.toolName).toBe("gitnexus_query");
  });

  it("should reject malformed lock data", () => {
    expect(() => SkillsLockDataSchema.parse({
      workspaceHash: "abc",
      detectedStack: ["nodejs"],
      activeTools: [],
      lastUpdated: new Date().toISOString(),
      schemaVersion: "2.0.0",
    })).toThrow();
  });
});
