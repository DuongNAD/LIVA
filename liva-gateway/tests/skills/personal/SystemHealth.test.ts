import { describe, it, expect, vi, beforeEach } from "vitest";
import { promisify } from "node:util";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

const { mockExecAsync } = vi.hoisted(() => ({
    mockExecAsync: vi.fn()
}));

vi.mock("node:child_process", () => {
    const execFn = (...args: any[]) => {};
    (execFn as any)[promisify.custom] = mockExecAsync;
    return { exec: execFn };
});

import { execute, metadata } from "../../../src/skills/personal/SystemHealth";

describe("Skill - SystemHealth", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export metadata", () => { expect(metadata.name).toBe("system_health"); });

    it("should return health report", async () => {
        // First call: powershell system metrics
        mockExecAsync.mockResolvedValueOnce({
            stdout: JSON.stringify({ CPU: 45, TotalRAM_GB: 16, UsedRAM_GB: 8, Battery_Percent: 80, DiskC_Free_GB: 100, DiskC_Total_GB: 500 }),
            stderr: ""
        });
        // Second call: nvidia-smi
        mockExecAsync.mockResolvedValueOnce({
            stdout: "NVIDIA RTX 4060, 55, 30, 2048, 8192",
            stderr: ""
        });

        const result = await execute();
        expect(result).toContain("SYSTEM HEALTH REPORT");
        expect(result).toContain("CPU Usage: 45%");
        expect(result).toContain("RTX 4060");
    });

    it("should handle no NVIDIA GPU gracefully", async () => {
        mockExecAsync.mockResolvedValueOnce({
            stdout: JSON.stringify({ CPU: 10, TotalRAM_GB: 8, UsedRAM_GB: 4, Battery_Percent: "N/A (PC Bàn)", DiskC_Free_GB: 50, DiskC_Total_GB: 250 }),
            stderr: ""
        });
        mockExecAsync.mockRejectedValueOnce(new Error("nvidia-smi not found"));

        const result = await execute();
        expect(result).toContain("SYSTEM HEALTH REPORT");
        expect(result).toContain("Không phát hiện");
    });

    it("should handle powershell error", async () => {
        mockExecAsync.mockRejectedValueOnce(new Error("PowerShell crash"));
        const result = await execute();
        expect(result).toContain("HEALTH ERROR");
    });
});
