import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("node:child_process", () => ({
    exec: vi.fn()
}));

import { execute, metadata } from "../../../src/skills/personal/MediaController";
import { exec } from "node:child_process";

describe("Skill - MediaController", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(exec).mockImplementation((cmd: any, cb: any) => {
            cb(null, { stdout: "", stderr: "" });
            return {} as any;
        });
    });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("media_controller");
    });

    it("should play/pause media", async () => {
        const result = await execute({ action: "play_pause" });
        expect(result).toContain("MEDIA SUCCESS");
        expect(result).toContain("Phát/Tạm dừng");
    });

    it("should skip to next track", async () => {
        const result = await execute({ action: "next_track" });
        expect(result).toContain("MEDIA SUCCESS");
        expect(result).toContain("Chuyển bài tiếp theo");
    });

    it("should go to previous track", async () => {
        const result = await execute({ action: "prev_track" });
        expect(result).toContain("MEDIA SUCCESS");
        expect(result).toContain("Quay lại bài trước");
    });

    it("should mute volume", async () => {
        const result = await execute({ action: "mute" });
        expect(result).toContain("MEDIA SUCCESS");
        expect(result).toContain("Tắt/Bật âm lượng");
    });

    it("should volume up", async () => {
        const result = await execute({ action: "volume_up" });
        expect(result).toContain("MEDIA SUCCESS");
        expect(result).toContain("Tăng âm lượng");
    });

    it("should volume down", async () => {
        const result = await execute({ action: "volume_down" });
        expect(result).toContain("MEDIA SUCCESS");
        expect(result).toContain("Giảm âm lượng");
    });

    it("should return ZodError for invalid action", async () => {
        const result = await execute({ action: "invalid_action_xyz" });
        expect(result).toContain("MEDIA ERROR");
        expect(result).toContain("Sai định dạng");
    });

    it("should handle exec error gracefully", async () => {
        vi.mocked(exec).mockImplementation((cmd: any, cb: any) => {
            cb(new Error("PowerShell not found"), { stdout: "", stderr: "" });
            return {} as any;
        });
        const result = await execute({ action: "play_pause" });
        expect(result).toContain("MEDIA ERROR");
        expect(result).toContain("PowerShell not found");
    });
});
