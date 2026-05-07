/**
 * AutoGPUSetup.test.ts — Hardware detection & validation tests
 * Updated to match async fsp-based source (no sync fs calls)
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { promises as fsp } from "node:fs";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock child_process.exec
vi.mock("node:child_process", () => ({
    exec: vi.fn(),
}));

// Mock node:fs — async-only (matches source)
vi.mock("node:fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("node:fs")>();
    return {
        ...actual,
        promises: {
            access: vi.fn().mockResolvedValue(undefined),
            readFile: vi.fn().mockResolvedValue(JSON.stringify({
                gpu_model: "NVIDIA RTX 4060",
                cuda_version: "12.4",
                vram_mb: 8192,
                ram_mb: 16000,
                cpu_threads: 8,
                is_battery: false,
                llama_server_ok: true,
                status: "success",
            })),
            writeFile: vi.fn().mockResolvedValue(undefined),
            rename: vi.fn().mockResolvedValue(undefined),
            mkdir: vi.fn().mockResolvedValue(undefined),
        },
        constants: actual.constants,
    };
});

import { AutoGPUSetup } from "../../src/scripts/AutoGPUSetup";
import { exec } from "node:child_process";

describe("AutoGPUSetup", () => {
    let onProgress: ReturnType<typeof vi.fn>;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        onProgress = vi.fn();

        // Restore default mocks
        (fsp.access as any).mockResolvedValue(undefined);
        (fsp.writeFile as any).mockResolvedValue(undefined);
        (fsp.rename as any).mockResolvedValue(undefined);
        (fsp.mkdir as any).mockResolvedValue(undefined);
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    describe("runAutoSetupIfNeeded", () => {
        it("should skip when hardware unchanged", async () => {
            // nvidia-smi returns same GPU as cached
            (exec as any).mockImplementation((cmd: string, opts: any, cb: Function) => {
                if (cmd.includes("--query-gpu")) {
                    cb(null, "NVIDIA RTX 4060, 8192", "");
                } else if (cmd.includes("BatteryStatus")) {
                    cb(null, "BatteryStatus\n2", ""); // AC Power
                } else {
                    cb(null, "CUDA Version: 12.4", "");
                }
            });

            // readFile returns cached state with matching GPU and is_battery
            (fsp.readFile as any).mockResolvedValue(JSON.stringify({
                gpu_model: "NVIDIA RTX 4060",
                cuda_version: "12.4",
                vram_mb: 8192,
                ram_mb: 16000,
                cpu_threads: 8,
                is_battery: false,
                llama_server_ok: true,
                status: "success",
            }));

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await vi.advanceTimersByTimeAsync(2000);
            await promise;

            // Should detect no change and skip setup
            expect(onProgress).toHaveBeenCalledWith("Đang kiểm tra phần cứng AI...");
        });

        it("should report missing llama-server.exe", async () => {
            // First access check (exePath) rejects
            (fsp.access as any).mockRejectedValueOnce(new Error("ENOENT"));

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await promise;

            expect(onProgress).toHaveBeenCalledWith(expect.stringContaining("llama-server.exe"));
        });

        it("should report missing model GGUF file", async () => {
            // First access (exePath) resolves, second (modelPath) rejects
            (fsp.access as any)
                .mockResolvedValueOnce(undefined)   // exePath exists
                .mockRejectedValueOnce(new Error("ENOENT"));  // modelPath missing

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await promise;

            expect(onProgress).toHaveBeenCalledWith(expect.stringContaining("model"));
        });

        it("should handle CPU-only mode when no NVIDIA GPU", async () => {
            // Simulate no nvidia-smi
            (exec as any).mockImplementation((_cmd: string, _opts: any, cb: Function) => {
                if (_cmd.includes("BatteryStatus")) {
                    cb(null, "BatteryStatus\n2", ""); // AC Power
                } else {
                    cb(new Error("nvidia-smi not found"), "", "");
                }
            });
            // readFile returns first-run state (force new detection)
            (fsp.readFile as any).mockResolvedValue(JSON.stringify({ status: "first_run" }));

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await vi.advanceTimersByTimeAsync(2000);
            await promise;

            expect(onProgress).toHaveBeenCalledWith(expect.stringContaining("CPU"));
            expect(fsp.writeFile).toHaveBeenCalled();
        });

        it("should detect new GPU and save state", async () => {
            // Simulate new GPU detected
            (exec as any).mockImplementation((cmd: string, _opts: any, cb: Function) => {
                if (cmd.includes("--query-gpu")) {
                    cb(null, "NVIDIA RTX 5090, 32768", "");
                } else if (cmd.includes("BatteryStatus")) {
                    cb(null, "BatteryStatus\n2", ""); // AC Power
                } else {
                    cb(null, "CUDA Version: 13.0", "");
                }
            });
            // readFile returns old cached state with different GPU
            (fsp.readFile as any).mockResolvedValue(JSON.stringify({
                gpu_model: "NVIDIA RTX 4060",
                status: "success",
                is_battery: false,
            }));

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await vi.advanceTimersByTimeAsync(2000);
            await promise;

            expect(onProgress).toHaveBeenCalledWith(expect.stringContaining("RTX 5090"));
            // Atomic write: writeFile writes to .tmp path
            expect(fsp.writeFile).toHaveBeenCalledWith(
                expect.stringMatching(/\.tmp$/),
                expect.stringContaining("RTX 5090"),
                "utf-8"
            );
            // Then rename .tmp to final path
            expect(fsp.rename).toHaveBeenCalled();
        });

        it("should handle hardware check errors gracefully", async () => {
            // Simulate nvidia-smi failure (CPU-only path)
            (exec as any).mockImplementation((_cmd: string, _opts: any, cb: Function) => {
                if (_cmd.includes("BatteryStatus")) {
                    cb(null, "BatteryStatus\n2", "");
                } else {
                    cb(new Error("nvidia-smi not found"), "", "");
                }
            });
            // readFile returns different state to force save
            (fsp.readFile as any).mockResolvedValue(JSON.stringify({ status: "first_run" }));
            // Force mkdir to throw (simulates disk failure during save)
            (fsp.mkdir as any).mockRejectedValue(new Error("Disk failure"));
            // Force writeFile to throw too
            (fsp.writeFile as any).mockRejectedValue(new Error("Disk failure"));

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await vi.advanceTimersByTimeAsync(3000);
            await promise;

            // saveHardwareState catches its own errors, so we get CPU-only message
            expect(onProgress).toHaveBeenCalledWith(expect.stringContaining("CPU"));
        });
    });
});
