import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock HITLGuard
vi.mock("@security/HITLGuard", () => ({
    HITLGuard: {
        requestApproval: vi.fn(),
    },
}));

// Mock logger
vi.mock("@utils/logger", () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn(),
        debug: vi.fn(),
    },
}));

// Mock execAsync via node:util promisify
const mockExecAsync = vi.hoisted(() => vi.fn());
vi.mock("node:util", () => ({
    promisify: () => mockExecAsync,
}));

import { execute, metadata, zombieScanner } from "../../../src/skills/devops/ZombieProcessHunter";
import { HITLGuard } from "@security/HITLGuard";

describe("ZombieProcessHunter Skill", () => {
    let dateNowSpy: any;
    let mockTime = 1000000000;

    beforeEach(() => {
        vi.clearAllMocks();
        mockTime = 1000000000;
        dateNowSpy = vi.spyOn(Date, "now").mockImplementation(() => mockTime);
    });

    afterEach(() => {
        zombieScanner.autoScanStop();
        if (dateNowSpy) {
            dateNowSpy.mockRestore();
        }
    });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("zombie_process_hunter");
        expect(metadata.kit).toBe("DEVOPS_KIT");
        expect(metadata.requires_hitl).toBe(true);
    });

    describe("scan", () => {
        it("should return clean status when no high-memory processes are found", async () => {
            mockExecAsync.mockResolvedValueOnce({ stdout: "" });
            const result = await execute({ action: "scan", memoryThresholdMB: 1024 });
            expect(result).toContain("Không tìm thấy tiến trình nào sử dụng >1024 MB RAM");
        });

        it("should track processes on first seen without flagging them as zombie immediately", async () => {
            const processData = { Id: 1234, ProcessName: "leak-app", MemMB: 2048, CPU_Sec: 50 };
            mockExecAsync.mockResolvedValueOnce({ stdout: JSON.stringify(processData) });

            const result = await execute({ action: "scan", memoryThresholdMB: 1024, idleThresholdMinutes: 10 });
            expect(result).toContain("Không phát hiện zombie");
            expect(result).toContain("leak-app");
        });

        it("should detect zombie process when idle and exceeding time threshold on subsequent scans", async () => {
            const processData = { Id: 1234, ProcessName: "zombie-app", MemMB: 2048, CPU_Sec: 50 };
            
            // First scan: registers the process
            mockExecAsync.mockResolvedValueOnce({ stdout: JSON.stringify(processData) });
            await execute({ action: "scan", memoryThresholdMB: 1024, idleThresholdMinutes: 10 });

            // Advance time past 10 minutes (10 * 60 * 1000 = 600000 ms)
            mockTime += 15 * 60 * 1000;

            // Second scan: CPU hasn't increased (idle), time has passed
            mockExecAsync.mockResolvedValueOnce({ stdout: JSON.stringify({ ...processData, CPU_Sec: 50.5 }) });
            const result = await execute({ action: "scan", memoryThresholdMB: 1024, idleThresholdMinutes: 10 });

            expect(result).toContain("🧟 Phát hiện 1 tiến trình zombie");
            expect(result).toContain("zombie-app");
            expect(result).toContain("1234");
        });

        it("should cleanup untracked/exited processes from history", async () => {
            const process1 = { Id: 111, ProcessName: "app1", MemMB: 2000, CPU_Sec: 10 };
            const process2 = { Id: 222, ProcessName: "app2", MemMB: 2000, CPU_Sec: 20 };

            // Scan 1: tracks app1 and app2
            mockExecAsync.mockResolvedValueOnce({ stdout: JSON.stringify([process1, process2]) });
            await execute({ action: "scan", memoryThresholdMB: 1000 });

            // Scan 2: only app1 remains active, app2 exited
            mockExecAsync.mockResolvedValueOnce({ stdout: JSON.stringify([process1]) });
            await execute({ action: "scan", memoryThresholdMB: 1000 });

            // Check status to see if tracked count is 1 (only app1 tracked)
            const statusResult = await execute({ action: "status" });
            expect(statusResult).toContain("Tiến trình đang track: 1");
        });

        it("should handle execution or json parsing errors gracefully", async () => {
            mockExecAsync.mockRejectedValueOnce(new Error("Powershell failed"));
            const result = await execute({ action: "scan", memoryThresholdMB: 1024 });
            expect(result).toContain("[ZOMBIE ERROR]");
            expect(result).toContain("Powershell failed");
        });
    });

    describe("auto scan", () => {
        it("should start and stop auto scan status correctly", async () => {
            mockExecAsync.mockResolvedValue({ stdout: "" });

            const startResult = await execute({ action: "auto_scan_start", memoryThresholdMB: 1024 });
            expect(startResult).toContain("Auto-scan đã bật");

            const statusResult = await execute({ action: "status" });
            expect(statusResult).toContain("Auto-scan: 🟢 Đang chạy");

            const stopResult = await execute({ action: "auto_scan_stop" });
            expect(stopResult).toContain("Auto-scan đã tắt");

            const statusResultAfter = await execute({ action: "status" });
            expect(statusResultAfter).toContain("Auto-scan: 🔴 Tắt");
        });

        it("should handle double start info cleanly", async () => {
            mockExecAsync.mockResolvedValue({ stdout: "" });
            await execute({ action: "auto_scan_start" });
            const secondStart = await execute({ action: "auto_scan_start" });
            expect(secondStart).toContain("Auto-scan đã đang chạy");
        });

        it("should handle double stop info cleanly", async () => {
            const stopResult = await execute({ action: "auto_scan_stop" });
            expect(stopResult).toContain("Auto-scan chưa được bật");
        });
    });

    describe("auto scan triggers", () => {
        it("should request HITL approval and kill zombie if auto scan finds one", async () => {
            vi.useFakeTimers();
            const processData = { Id: 9999, ProcessName: "zombie-auto", MemMB: 6000, CPU_Sec: 10 };

            mockExecAsync.mockResolvedValue({ stdout: JSON.stringify(processData) });
            vi.mocked(HITLGuard.requestApproval).mockResolvedValue(true);

            // Start auto scan - runs first scan
            await execute({ action: "auto_scan_start", memoryThresholdMB: 5000, idleThresholdMinutes: 5 });

            // Move time forward and trigger interval callback
            mockTime += 10 * 60 * 1000;
            // mockExecAsync returns same process (idle)
            mockExecAsync.mockResolvedValue({ stdout: JSON.stringify({ ...processData, CPU_Sec: 10.2 }) });

            const stdoutWriteSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

            // Run pending timers to trigger interval scan
            await vi.runOnlyPendingTimersAsync();

            expect(HITLGuard.requestApproval).toHaveBeenCalled();
            expect(mockExecAsync).toHaveBeenCalledWith(
                expect.stringContaining("Stop-Process -Id 9999"),
                expect.any(Object)
            );
            expect(stdoutWriteSpy).toHaveBeenCalledWith(expect.stringContaining("✅ Đã kill zombie-auto"));

            stdoutWriteSpy.mockRestore();
            vi.useRealTimers();
        });

        it("should handle HITL rejection, skip killing and reset firstSeen time", async () => {
            vi.useFakeTimers();
            const processData = { Id: 8888, ProcessName: "zombie-skipped", MemMB: 6000, CPU_Sec: 10 };

            mockExecAsync.mockResolvedValue({ stdout: JSON.stringify(processData) });
            vi.mocked(HITLGuard.requestApproval).mockRejectedValue(new Error("User denied"));

            // Start auto scan
            await execute({ action: "auto_scan_start", memoryThresholdMB: 5000, idleThresholdMinutes: 5 });

            // Move time forward
            mockTime += 10 * 60 * 1000;
            mockExecAsync.mockResolvedValue({ stdout: JSON.stringify({ ...processData, CPU_Sec: 10.2 }) });

            const stdoutWriteSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

            // Trigger interval
            await vi.runOnlyPendingTimersAsync();

            expect(HITLGuard.requestApproval).toHaveBeenCalled();
            // Should NOT have run kill command
            expect(mockExecAsync).not.toHaveBeenCalledWith(
                expect.stringContaining("Stop-Process -Id 8888"),
                expect.any(Object)
            );

            stdoutWriteSpy.mockRestore();
            vi.useRealTimers();
        });
    });

    describe("Zod and error formatting", () => {
        it("should validate action parameters", async () => {
            const result = await execute({ action: "invalid_action" });
            expect(result).toContain("[ZOMBIE ERROR] Sai định dạng");
        });
    });
});
