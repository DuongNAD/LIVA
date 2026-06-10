import { describe, it, expect, vi, beforeEach } from "vitest";
import { promisify } from "node:util";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

const { mockExecAsync, mockCpus, mockTotalmem, mockFreemem } = vi.hoisted(() => ({
    mockExecAsync: vi.fn(),
    mockCpus: vi.fn().mockReturnValue(new Array(8).fill({ model: "Intel CPU" })),
    mockTotalmem: vi.fn().mockReturnValue(16 * 1024 * 1024 * 1024),
    mockFreemem: vi.fn().mockReturnValue(8 * 1024 * 1024 * 1024)
}));

vi.mock("node:child_process", () => {
    const execFn = (...args: any[]) => {};
    (execFn as any)[promisify.custom] = mockExecAsync;
    return { exec: execFn };
});

vi.mock("node:os", () => ({
    cpus: mockCpus,
    totalmem: mockTotalmem,
    freemem: mockFreemem
}));

import { execute, metadata } from "../../../src/skills/personal/SystemHealth";

describe("Skill - SystemHealth", () => {
    let platformSpy: any;

    beforeEach(() => {
        vi.clearAllMocks();
        platformSpy = vi.spyOn(process, "platform", "get");
        
        // Reset defaults
        mockCpus.mockReturnValue(new Array(8).fill({ model: "Intel CPU" }));
        mockTotalmem.mockReturnValue(16 * 1024 * 1024 * 1024);
        mockFreemem.mockReturnValue(8 * 1024 * 1024 * 1024);
    });

    it("should export metadata", () => { expect(metadata.name).toBe("system_health"); });

    describe("Windows Platform", () => {
        beforeEach(() => {
            platformSpy.mockReturnValue("win32");
        });

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

    describe("macOS Platform", () => {
        beforeEach(() => {
            platformSpy.mockReturnValue("darwin");
            vi.spyOn(process, "arch", "get").mockReturnValue("arm64");
            mockCpus.mockReturnValue(new Array(8).fill({ model: "Apple M5" }));
            mockTotalmem.mockReturnValue(32 * 1024 * 1024 * 1024);
            mockFreemem.mockReturnValue(2 * 1024 * 1024 * 1024);
        });

        it("should return macOS health report", async () => {
            // macOS calls:
            // 1. CPU: ps -A -o %cpu | awk ... (expect 360% total across 8 cores -> 45%)
            mockExecAsync.mockResolvedValueOnce({ stdout: "360", stderr: "" });
            // 2. Battery: pmset -g batt
            mockExecAsync.mockResolvedValueOnce({ stdout: "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=12345) 80%; discharging;", stderr: "" });
            // 3. Disk: df -g / | tail -1
            mockExecAsync.mockResolvedValueOnce({ stdout: "/dev/disk3s1s1 500 400 100 80% 123 456 0% /", stderr: "" });

            const result = await execute();
            expect(result).toContain("SYSTEM HEALTH REPORT");
            expect(result).toContain("RAM Usage: 30 GB / 32 GB");
            expect(result).toContain("CPU Usage: 45%");
            expect(result).toContain("Battery: 80%");
            expect(result).toContain("Ổ đĩa hệ thống: Còn trống 100 GB / Tổng 500 GB");
            expect(result).toContain("Apple M5");
        });
    });
});
