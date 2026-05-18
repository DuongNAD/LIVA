import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("node:child_process", () => ({
    exec: vi.fn((cmd: string, cb: Function) => cb(null, { stdout: "OK" }))
}));

vi.mock("@security/HITLGuard", () => ({
    HITLGuard: {
        requestApproval: vi.fn().mockResolvedValue(true)
    }
}));

import { execute, metadata } from "../../../src/skills/personal/WorkspaceManager";
import { HITLGuard } from "../../../src/security/HITLGuard";

describe("Skill - WorkspaceManager", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("workspace_manager");
    });

    it("should minimize all windows", async () => {
        const result = await execute({ action: "minimize_all" });
        expect(result).toContain("WORKSPACE SUCCESS");
        expect(result).toContain("thu nhỏ");
    });

    it("should lock screen with HITL approval", async () => {
        const result = await execute({ action: "lock_screen" });
        expect(HITLGuard.requestApproval).toHaveBeenCalled();
        expect(result).toContain("WORKSPACE SUCCESS");
        expect(result).toContain("khóa màn hình");
    });

    it("should shutdown with HITL approval", async () => {
        const result = await execute({ action: "shutdown" });
        expect(result).toContain("WORKSPACE SUCCESS");
        expect(result).toContain("TẮT");
    });

    it("should restart with HITL approval", async () => {
        const result = await execute({ action: "restart" });
        expect(result).toContain("WORKSPACE SUCCESS");
        expect(result).toContain("KHỞI ĐỘNG LẠI");
    });

    it("should sleep with HITL approval", async () => {
        const result = await execute({ action: "sleep" });
        expect(result).toContain("WORKSPACE SUCCESS");
        expect(result).toContain("Ngủ");
    });

    it("should block action if HITL rejected", async () => {
        vi.mocked(HITLGuard.requestApproval).mockRejectedValueOnce(new Error("User denied"));
        const result = await execute({ action: "shutdown" });
        expect(result).toContain("WORKSPACE BLOCKED");
        expect(result).toContain("User denied");
    });

    it("should activate focus mode", async () => {
        const stdoutSpy = vi.spyOn(process.stdout, "write").mockReturnValue(true);
        const result = await execute({ action: "focus_mode" });
        expect(result).toContain("WORKSPACE SUCCESS");
        expect(result).toContain("Focus Mode");
        stdoutSpy.mockRestore();
    });

    it("should return error for invalid action (ZodError)", async () => {
        const result = await execute({ action: "invalid" });
        expect(result).toContain("WORKSPACE ERROR");
        expect(result).toContain("Sai định dạng");
    });
});
