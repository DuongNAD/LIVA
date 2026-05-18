/**
 * update_session_state.test.ts — Unit Tests for Session State WAL Skill
 * ======================================================================
 * Tests Write-Ahead Logging pattern, parameter validation, and MemoryManager integration.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

// Mock CoreKernel (imported but not directly used by execute)
vi.mock("../../src/core/CoreKernel", () => ({
    CoreKernel: vi.fn(),
}));

describe("update_session_state Skill", () => {
    let savedGlobal: any;

    beforeEach(() => {
        vi.resetAllMocks();
        savedGlobal = (global as any).kernelInstance;
    });

    afterEach(() => {
        (global as any).kernelInstance = savedGlobal;
    });

    async function loadModule() {
        return await import("../../src/skills/core/UpdateSessionState");
    }

    // ──────────────────────────────────────
    //  Metadata
    // ──────────────────────────────────────
    describe("metadata", () => {
        it("should export correct skill name", async () => {
            const { update_session_state } = await loadModule();
            expect(update_session_state.name).toBe("update_session_state");
        });

        it("should be marked as core skill", async () => {
            const { update_session_state } = await loadModule();
            expect(update_session_state.isCoreSkill).toBe(true);
        });

        it("should require intent, current_context, and pending_tasks", async () => {
            const { update_session_state } = await loadModule();
            expect(update_session_state.parameters.required).toContain("intent");
            expect(update_session_state.parameters.required).toContain("current_context");
            expect(update_session_state.parameters.required).toContain("pending_tasks");
        });
    });

    // ──────────────────────────────────────
    //  Input Validation
    // ──────────────────────────────────────
    describe("Input Validation", () => {
        it("should reject when intent is missing", async () => {
            const { update_session_state } = await loadModule();
            const result = await update_session_state.execute!({
                current_context: "test",
                pending_tasks: ["task1"],
            });
            expect(result).toContain("Error");
        });

        it("should reject when current_context is missing", async () => {
            const { update_session_state } = await loadModule();
            const result = await update_session_state.execute!({
                intent: "test",
                pending_tasks: ["task1"],
            });
            expect(result).toContain("Error");
        });

        it("should reject when pending_tasks is not an array", async () => {
            const { update_session_state } = await loadModule();
            const result = await update_session_state.execute!({
                intent: "test",
                current_context: "ctx",
                pending_tasks: "not an array",
            });
            expect(result).toContain("Error");
        });
    });

    // ──────────────────────────────────────
    //  WAL Write Success
    // ──────────────────────────────────────
    describe("Write-Ahead Logging", () => {
        it("should write session state successfully when MemoryManager exists", async () => {
            const mockUpdateSessionState = vi.fn().mockResolvedValue(undefined);
            (global as any).kernelInstance = {
                memory: {
                    updateSessionState: mockUpdateSessionState,
                },
            };

            const { update_session_state } = await loadModule();
            const result = await update_session_state.execute!({
                intent: "Phân tích mã nguồn",
                current_context: "Đang kiểm tra AgentLoop.ts",
                pending_tasks: ["Fix circular dependency", "Add tests"],
            });

            expect(result).toContain("Session State saved successfully");
            expect(mockUpdateSessionState).toHaveBeenCalledWith(
                expect.stringContaining("Phân tích mã nguồn"),
            );
        });

        it("should format markdown content correctly", async () => {
            const mockUpdateSessionState = vi.fn().mockResolvedValue(undefined);
            (global as any).kernelInstance = {
                memory: {
                    updateSessionState: mockUpdateSessionState,
                },
            };

            const { update_session_state } = await loadModule();
            await update_session_state.execute!({
                intent: "Deploy app",
                current_context: "Building Docker image",
                pending_tasks: ["Push to registry", "Update k8s"],
            });

            const writtenContent = mockUpdateSessionState.mock.calls[0][0];
            expect(writtenContent).toContain("# SESSION STATE");
            expect(writtenContent).toContain("## Core Intent");
            expect(writtenContent).toContain("Deploy app");
            expect(writtenContent).toContain("## Current Context");
            expect(writtenContent).toContain("Building Docker image");
            expect(writtenContent).toContain("## Pending Tasks");
            expect(writtenContent).toContain("- [ ] Push to registry");
            expect(writtenContent).toContain("- [ ] Update k8s");
        });
    });

    // ──────────────────────────────────────
    //  Missing MemoryManager
    // ──────────────────────────────────────
    describe("Missing MemoryManager", () => {
        it("should return error when kernelInstance is undefined", async () => {
            (global as any).kernelInstance = undefined;

            const { update_session_state } = await loadModule();
            const result = await update_session_state.execute!({
                intent: "Test",
                current_context: "Test",
                pending_tasks: ["Test"],
            });

            expect(result).toContain("Error");
            expect(result).toContain("MemoryManager");
        });

        it("should return error when memory property is null", async () => {
            (global as any).kernelInstance = { memory: null };

            const { update_session_state } = await loadModule();
            const result = await update_session_state.execute!({
                intent: "Test",
                current_context: "Test",
                pending_tasks: ["Test"],
            });

            expect(result).toContain("Error");
        });
    });
});
