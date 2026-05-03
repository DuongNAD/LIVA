/**
 * AutoGPUSetup.test.ts — Hardware detection & validation tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock child_process.exec
vi.mock("node:child_process", () => ({
    exec: vi.fn(),
}));

// Mock node:fs
vi.mock("node:fs", () => ({
    existsSync: vi.fn().mockReturnValue(true),
    readFileSync: vi.fn().mockReturnValue(JSON.stringify({
        gpu_model: "NVIDIA RTX 4060",
        cuda_version: "12.4",
        vram_mb: 8192,
        llama_server_ok: true,
        status: "success",
    })),
    writeFileSync: vi.fn(),
    mkdirSync: vi.fn(),
}));

import { AutoGPUSetup } from "../../src/scripts/AutoGPUSetup";
import { exec } from "node:child_process";
import * as fs from "node:fs";

describe("AutoGPUSetup", () => {
    let onProgress: ReturnType<typeof vi.fn>;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        onProgress = vi.fn();
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
                } else {
                    cb(null, "CUDA Version: 12.4", "");
                }
            });

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await vi.advanceTimersByTimeAsync(2000);
            await promise;

            // Should detect no change and skip setup
            expect(onProgress).toHaveBeenCalledWith("Đang kiểm tra phần cứng AI...");
        });

        it("should report missing llama-server.exe", async () => {
            (fs.existsSync as any).mockReturnValueOnce(false);  // exePath missing

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await promise;

            expect(onProgress).toHaveBeenCalledWith(expect.stringContaining("llama-server.exe"));
        });

        it("should report missing model GGUF file", async () => {
            (fs.existsSync as any)
                .mockReturnValueOnce(true)   // exePath exists
                .mockReturnValueOnce(false);  // modelPath missing

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await promise;

            expect(onProgress).toHaveBeenCalledWith(expect.stringContaining("model"));
        });

        it("should handle CPU-only mode when no NVIDIA GPU", async () => {
            // Simulate no nvidia-smi
            (exec as any).mockImplementation((_cmd: string, _opts: any, cb: Function) => {
                cb(new Error("nvidia-smi not found"), "", "");
            });
            (fs.readFileSync as any).mockReturnValue(JSON.stringify({ status: "first_run" }));

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await vi.advanceTimersByTimeAsync(2000);
            await promise;

            expect(onProgress).toHaveBeenCalledWith(expect.stringContaining("CPU"));
            expect(fs.writeFileSync).toHaveBeenCalled();
        });

        it("should detect new GPU and save state", async () => {
            // Simulate new GPU detected
            (exec as any).mockImplementation((cmd: string, _opts: any, cb: Function) => {
                if (cmd.includes("--query-gpu")) {
                    cb(null, "NVIDIA RTX 5090, 32768", "");
                } else {
                    cb(null, "CUDA Version: 13.0", "");
                }
            });
            (fs.readFileSync as any).mockReturnValue(JSON.stringify({
                gpu_model: "NVIDIA RTX 4060",
                status: "success",
            }));

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await vi.advanceTimersByTimeAsync(2000);
            await promise;

            expect(onProgress).toHaveBeenCalledWith(expect.stringContaining("RTX 5090"));
            expect(fs.writeFileSync).toHaveBeenCalledWith(
                expect.any(String),
                expect.stringContaining("RTX 5090"),
                "utf-8"
            );
        });

        it("should handle hardware check errors gracefully", async () => {
            (fs.existsSync as any).mockImplementation(() => { throw new Error("Disk failure"); });

            const promise = AutoGPUSetup.runAutoSetupIfNeeded(onProgress);
            await vi.advanceTimersByTimeAsync(3000);
            await promise;

            expect(onProgress).toHaveBeenCalledWith(expect.stringContaining("Không thể kiểm tra"));
        });
    });
});
