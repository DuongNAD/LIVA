import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { HeartbeatManager } from "../../src/core/HeartbeatManager";
import { AgentLoop } from "../../src/core/AgentLoop";
import * as fs from "fs/promises";
import { logger } from "../../src/utils/logger";

vi.mock("fs/promises");
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn()
    }
}));

describe("HeartbeatManager", () => {
    let mockAgentLoop: Partial<AgentLoop>;
    let heartbeatManager: HeartbeatManager;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();

        mockAgentLoop = {
            handleUserInput: vi.fn(),
        };

        heartbeatManager = new HeartbeatManager(mockAgentLoop as AgentLoop);
    });

    afterEach(() => {
        vi.restoreAllMocks();
        vi.useRealTimers();
    });

    it("should start and set interval correctly", () => {
        heartbeatManager.start();
        expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Đã khởi động động cơ chủ động"));
        expect(vi.getTimerCount()).toBe(1);
    });

    it("should ignore subsequent start calls if already running", () => {
        heartbeatManager.start();
        expect(vi.getTimerCount()).toBe(1);
        
        heartbeatManager.start();
        expect(vi.getTimerCount()).toBe(1);
    });

    it("should stop and clear interval", () => {
        heartbeatManager.start();
        expect(vi.getTimerCount()).toBe(1);

        heartbeatManager.stop();
        expect(vi.getTimerCount()).toBe(0);
        expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Đã dừng nhịp đập"));
    });

    it("should trigger heartbeat, read HEARTBEAT.md and call agentLoop.handleUserInput", async () => {
        const mockFileContent = "HEARTBEAT_MOCK_CONTENT";
        vi.mocked(fs.readFile).mockResolvedValueOnce(mockFileContent);

        heartbeatManager.start();
        
        // Fast-forward 30 minutes
        await vi.advanceTimersByTimeAsync(30 * 60 * 1000);

        expect(fs.readFile).toHaveBeenCalledWith(expect.stringContaining("HEARTBEAT.md"), "utf-8");
        expect(mockAgentLoop.handleUserInput).toHaveBeenCalledWith(mockFileContent, true);
        expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Phát kích thích nhịp đập"));
    });

    it("should handle file read errors gracefully without crashing", async () => {
        vi.mocked(fs.readFile).mockRejectedValueOnce(new Error("File not found"));

        heartbeatManager.start();
        await vi.advanceTimersByTimeAsync(30 * 60 * 1000);

        expect(logger.error).toHaveBeenCalledWith(expect.stringContaining("Lỗi đọc tệp HEARTBEAT.md"));
        expect(mockAgentLoop.handleUserInput).not.toHaveBeenCalled();
    });

    it("should do nothing when stop() is called without start() (Line 24 false branch)", () => {
        heartbeatManager.stop(); // timer is null
        expect(logger.info).not.toHaveBeenCalledWith(expect.stringContaining("Đã dừng nhịp đập"));
    });
});
