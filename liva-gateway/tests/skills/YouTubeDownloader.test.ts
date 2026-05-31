import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("node:child_process", () => ({
    spawn: vi.fn().mockImplementation(() => {
        const { PassThrough } = require("node:stream");
        const stdout = new PassThrough();
        const stderr = new PassThrough();
        const child: any = { stdout, stderr, on: vi.fn((evt: string, cb: Function) => { if (evt === "close") setTimeout(() => cb(0), 10); return child; }) };
        // End stdout immediately so pipeline resolves
        setTimeout(() => stdout.end(), 5);
        return child;
    }),
    exec: vi.fn((...fnArgs: any[]) => {
        const cmd: string = fnArgs[0];
        const cb: Function = typeof fnArgs[1] === "function" ? fnArgs[1] : fnArgs[2];
        if (cmd.includes("where yt-dlp") || cmd.includes("which yt-dlp")) {
            cb(null, "C:\\tools\\yt-dlp.exe\n", "");
        } else if (cmd.includes("--get-title")) {
            cb(null, "Test Video\n", "");
        } else {
            cb(null, "", "");
        }
    }),
}));

vi.mock("node:fs", async () => {
    const actual: any = await vi.importActual("node:fs");
    const { PassThrough } = require("node:stream");
    return {
        ...actual,
        promises: {
            ...actual.promises,
            mkdir: vi.fn().mockResolvedValue(undefined),
        },
        createWriteStream: vi.fn().mockReturnValue(new PassThrough()),
    };
});

import { metadata, execute } from "../../src/skills/web/YouTubeDownloader";

describe("YouTubeDownloader", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should have correct metadata", () => {
        expect(metadata.name).toBe("youtube_downloader");
        expect(metadata.parameters.required).toContain("url");
    });

    it("should reject empty URL", async () => {
        const result = await execute({ url: "" });
        expect(result).toContain("Error");
    });

    it("should reject invalid YouTube URL", async () => {
        const result = await execute({ url: "https://facebook.com/video" });
        expect(result).toContain("Invalid YouTube URL");
    });

    it("should download MP4 video", async () => {
        const result = await execute({ url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ" });
        expect(result).toContain("✅");
        expect(result).toContain("Download completed");
    });

    it("should download MP3 audio", async () => {
        const result = await execute({ url: "https://youtu.be/dQw4w9WgXcQ", format: "mp3" });
        expect(result).toContain("✅");
    });

    it("should accept YouTube Shorts URL", async () => {
        const result = await execute({ url: "https://www.youtube.com/shorts/abc123" });
        expect(result).toContain("✅");
    });
});
