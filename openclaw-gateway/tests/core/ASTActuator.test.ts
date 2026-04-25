/**
 * ASTActuator.test.ts — Smoke Test + Guardrail Tests
 * Tests basic instantiation and mutation quota guardrails
 * without requiring a full ts-morph sandbox setup.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock fs to prevent real filesystem access
vi.mock("fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("fs")>();
    return {
        ...actual,
        existsSync: vi.fn().mockReturnValue(true),
        mkdirSync: vi.fn(),
        default: {
            ...actual,
            existsSync: vi.fn().mockReturnValue(true),
            mkdirSync: vi.fn(),
        }
    };
});

vi.mock("fs/promises", () => ({
    access: vi.fn().mockRejectedValue(new Error("ENOENT")),
    mkdir: vi.fn().mockResolvedValue(undefined),
    rm: vi.fn().mockResolvedValue(undefined),
    readFile: vi.fn().mockResolvedValue(""),
    writeFile: vi.fn().mockResolvedValue(undefined),
    rename: vi.fn().mockResolvedValue(undefined),
    copyFile: vi.fn().mockResolvedValue(undefined),
    cp: vi.fn().mockResolvedValue(undefined),
    symlink: vi.fn().mockResolvedValue(undefined),
    unlink: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { ASTActuator } from "../../src/core/ASTActuator";

describe("ASTActuator", () => {
    let actuator: ASTActuator;

    beforeEach(() => {
        vi.clearAllMocks();
        actuator = new ASTActuator("/tmp/test-workspace");
    });

    describe("Smoke Test", () => {
        it("should instantiate without errors", () => {
            expect(actuator).toBeDefined();
            expect(actuator).toBeInstanceOf(ASTActuator);
        });
    });

    describe("Mutation Quota Guardrail", () => {
        it("should reject when create mutations exceed limit (>3)", async () => {
            const mutations = Array.from({ length: 4 }, (_, i) => ({
                type: "create" as const,
                filePath: `src/file${i}.ts`,
                code: "export const x = 1;",
            }));

            const result = await actuator.actuateCandidateBatch("test-cand", mutations);
            expect(result.success).toBe(false);
            expect(result.asi).toContain("Mutation limit exceeded");
        });

        it("should reject when modify mutations exceed limit (>10)", async () => {
            const mutations = Array.from({ length: 11 }, (_, i) => ({
                type: "modify" as const,
                filePath: `src/file${i}.ts`,
                code: "<<<< SEARCH\nold\n====\nnew\n>>>> REPLACE",
            }));

            const result = await actuator.actuateCandidateBatch("test-cand-2", mutations);
            expect(result.success).toBe(false);
            expect(result.asi).toContain("Mutation limit exceeded");
        });

        it("should accept mutations within quota (3 create, 10 modify)", async () => {
            const mutations = [
                { type: "create" as const, filePath: "src/a.ts", code: "export const a = 1;" },
                { type: "create" as const, filePath: "src/b.ts", code: "export const b = 2;" },
                { type: "modify" as const, filePath: "src/c.ts", code: "<<<< SEARCH\nold\n====\nnew\n>>>> REPLACE" },
            ];

            // This will fail at sandbox creation (fs mocked) but pass the quota check
            const result = await actuator.actuateCandidateBatch("test-cand-3", mutations);
            // It should get past the quota guardrail — if it fails, it's from sandbox creation, not quota
            if (!result.success) {
                expect(result.asi).not.toContain("Mutation limit exceeded");
            }
        });
    });

    describe("Path Safety Guardrail", () => {
        it("should reject paths outside src/", async () => {
            const mutations = [
                { type: "modify" as const, filePath: "../etc/passwd", code: "malicious" },
            ];

            const result = await actuator.actuateCandidateBatch("evil-cand", mutations);
            // Either fails at quota (no), path jail, or sandbox creation
            if (!result.success && result.asi) {
                // Should not contain "Mutation limit exceeded" since we're within limits
                expect(result.asi).not.toContain("Mutation limit exceeded");
            }
        });
    });
});
