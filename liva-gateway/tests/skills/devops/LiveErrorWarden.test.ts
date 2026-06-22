import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

// Track active watchers created by mock chokidar
let activeWatchers: any[] = [];

vi.mock("chokidar", () => ({
    watch: vi.fn().mockImplementation(() => {
        const w = new EventEmitter() as any;
        w.close = vi.fn().mockResolvedValue(undefined);
        activeWatchers.push(w);
        return w;
    }),
}));

// Mock node:fs
const mockAccessSync = vi.fn();
const mockStatSync = vi.fn();
const mockCreateReadStream = vi.fn();
vi.mock("node:fs", () => ({
    accessSync: (...args: any[]) => mockAccessSync(...args),
    statSync: (...args: any[]) => mockStatSync(...args),
    createReadStream: (...args: any[]) => mockCreateReadStream(...args),
    constants: { R_OK: 4 },
}));

// Mock logger
vi.mock("@utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

// Mock execAsync
const mockExecAsync = vi.hoisted(() => vi.fn());
vi.mock("node:util", () => ({
    promisify: () => mockExecAsync,
}));

import { execute, metadata, logWatcherRegistry } from "../../../src/skills/devops/LiveErrorWarden";
import * as chokidar from "chokidar";

describe("LiveErrorWarden Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mockAccessSync.mockReset();
        mockStatSync.mockReset();
        mockCreateReadStream.mockReset();
        mockExecAsync.mockReset();
        activeWatchers = [];
    });

    afterEach(async () => {
        await logWatcherRegistry.dispose();
    });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("live_error_warden");
        expect(metadata.kit).toBe("DEVOPS_KIT");
    });

    describe("watch action", () => {
        it("should return error if file does not exist or is not readable", async () => {
            mockAccessSync.mockImplementationOnce(() => {
                throw new Error("ENOENT");
            });

            const result = await execute({ action: "watch", filePath: "nonexistent.log" });
            expect(result).toContain("[WARDEN ERROR] Không tìm thấy hoặc không đọc được file");
        });

        it("should start watching file successfully when file is accessible", async () => {
            mockAccessSync.mockImplementationOnce(() => {});
            mockStatSync.mockReturnValueOnce({ size: 500 });

            const result = await execute({ action: "watch", filePath: "app.log" });
            expect(result).toContain("[WARDEN SUCCESS] Đang theo dõi file");
            expect(result).toContain("byte 500");
            expect(chokidar.watch).toHaveBeenCalled();
        });

        it("should return info if file is already being watched", async () => {
            mockAccessSync.mockImplementation(() => {});
            mockStatSync.mockReturnValue({ size: 500 });

            await execute({ action: "watch", filePath: "app.log" });
            const result = await execute({ action: "watch", filePath: "app.log" });

            expect(result).toContain("đã đang được theo dõi");
        });
    });

    describe("unwatch action", () => {
        it("should return error if unwatching a file that is not watched", async () => {
            const result = await execute({ action: "unwatch", filePath: "notwatched.log" });
            expect(result).toContain("không đang được theo dõi");
        });

        it("should unwatch file successfully", async () => {
            mockAccessSync.mockImplementation(() => {});
            mockStatSync.mockReturnValue({ size: 500 });

            await execute({ action: "watch", filePath: "app.log" });
            const result = await execute({ action: "unwatch", filePath: "app.log" });

            expect(result).toContain("[WARDEN SUCCESS] Đã ngừng theo dõi");
            expect(activeWatchers[0].close).toHaveBeenCalled();
        });
    });

    describe("list action", () => {
        it("should report empty state when no files are watched", async () => {
            const result = await execute({ action: "list" });
            expect(result).toContain("Không có file nào đang được theo dõi");
        });

        it("should list all currently watched files", async () => {
            mockAccessSync.mockImplementation(() => {});
            mockStatSync.mockReturnValue({ size: 500 });

            await execute({ action: "watch", filePath: "app.log" });
            const result = await execute({ action: "list" });

            expect(result).toContain("Đang theo dõi 1 file");
            expect(result).toContain("app.log");
        });
    });

    describe("file change detection", () => {
        it("should detect errors on file change and copy suggestions to clipboard", async () => {
            mockAccessSync.mockImplementation(() => {});
            // 1st call: watch init size = 100
            // 2nd call: change trigger check size = 300
            mockStatSync
                .mockReturnValueOnce({ size: 100 })
                .mockReturnValueOnce({ size: 300 });

            // Watch file
            await execute({ action: "watch", filePath: "error.log" });

            // Set up mock read stream
            const mockStream = new EventEmitter();
            mockCreateReadStream.mockReturnValueOnce(mockStream);

            mockExecAsync.mockResolvedValue({ stdout: "" });
            const stdoutWriteSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

            // Trigger change event on the watcher
            activeWatchers[0].emit("change");

            // Verify read stream was created with correct offsets
            expect(mockCreateReadStream).toHaveBeenCalledWith(
                expect.any(String),
                expect.objectContaining({ start: 100, end: 299 })
            );

            // Push some content with an error
            mockStream.emit("data", "Line 1: OK\nLine 2: ECONNREFUSED: connection refused\nLine 3: OK");
            mockStream.emit("end");

            // Verify clipboard command called
            expect(mockExecAsync).toHaveBeenCalledWith(
                expect.stringContaining("Set-Clipboard"),
                expect.any(Object)
            );
            expect(mockExecAsync).toHaveBeenCalledWith(
                expect.stringContaining("Kiểm tra service đích có đang chạy không"),
                expect.any(Object)
            );

            // Verify toast written to stdout
            expect(stdoutWriteSpy).toHaveBeenCalledWith(expect.stringContaining("error.log phát hiện lỗi"));

            stdoutWriteSpy.mockRestore();
        });

        it("should support custom patterns", async () => {
            mockAccessSync.mockImplementation(() => {});
            mockStatSync
                .mockReturnValueOnce({ size: 100 })
                .mockReturnValueOnce({ size: 200 });

            // Watch file with custom pattern
            await execute({
                action: "watch",
                filePath: "custom.log",
                patterns: ["CUSTOM_ALERT_PATTERN"],
            });

            const mockStream = new EventEmitter();
            mockCreateReadStream.mockReturnValueOnce(mockStream);

            mockExecAsync.mockResolvedValue({ stdout: "" });
            const stdoutWriteSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);

            activeWatchers[0].emit("change");

            mockStream.emit("data", "Line 1: some noise\nLine 2: CUSTOM_ALERT_PATTERN trigger\nLine 3: done");
            mockStream.emit("end");

            // Check that custom pattern caught the error
            expect(stdoutWriteSpy).toHaveBeenCalledWith(expect.stringContaining("custom.log phát hiện lỗi"));

            stdoutWriteSpy.mockRestore();
        });

        it("should handle log rotation / file truncation safely", async () => {
            mockAccessSync.mockImplementation(() => {});
            mockStatSync
                .mockReturnValueOnce({ size: 1000 })
                .mockReturnValueOnce({ size: 200 });

            await execute({ action: "watch", filePath: "rotated.log" });

            activeWatchers[0].emit("change");

            // No read stream should be created since it was truncated
            expect(mockCreateReadStream).not.toHaveBeenCalled();
        });
    });

    describe("input validation", () => {
        it("should reject action and output error when validation fails", async () => {
            const result = await execute({ action: "watch" }); // Missing filePath
            expect(result).toContain("[WARDEN ERROR] Cần cung cấp 'filePath'");
        });

        it("should validate action name parameter", async () => {
            const result = await execute({ action: "invalid_action" });
            expect(result).toContain("[WARDEN ERROR] Sai định dạng");
        });
    });
});
