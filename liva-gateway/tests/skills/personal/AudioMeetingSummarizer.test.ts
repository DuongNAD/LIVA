import { describe, it, expect, vi, beforeEach } from "vitest";
import { execute, metadata } from "../../../src/skills/personal/AudioMeetingSummarizer";
import { safeFetch } from "../../../src/utils/HttpClient";
import { safeRename } from "../../../src/utils/FileUtils";
import { promises as fsp } from "node:fs";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("@utils/HttpClient", () => ({
    safeFetch: vi.fn()
}));

vi.mock("@utils/FileUtils", () => ({
    safeRename: vi.fn().mockResolvedValue(undefined)
}));

vi.mock("node:fs", () => ({
    promises: {
        access: vi.fn(),
        readFile: vi.fn().mockResolvedValue(Buffer.from("mock_audio_content")),
        mkdir: vi.fn().mockResolvedValue(undefined),
        writeFile: vi.fn().mockResolvedValue(undefined),
        stat: vi.fn().mockResolvedValue({ size: 10 * 1024 * 1024 }), // default 10MB (under 20MB limit)
        readdir: vi.fn().mockResolvedValue([]),
        rm: vi.fn().mockResolvedValue(undefined)
    }
}));

vi.mock("node:child_process", () => ({
    exec: vi.fn((cmd, cb) => {
        cb(null, { stdout: "ffmpeg version 4.4" });
    })
}));

describe("Skill - AudioMeetingSummarizer", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(fsp.stat).mockResolvedValue({ size: 10 * 1024 * 1024 } as any);
    });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("audio_meeting_summarizer");
        expect(metadata.category).toBe("personal");
    });

    it("should fail if audio file does not exist", async () => {
        vi.mocked(fsp.access).mockRejectedValueOnce(new Error("File not found"));
        const result = await execute({ audio_path: "missing.wav" });
        expect(result).toContain("Error: Audio file not found");
    });

    it("should transcribe and summarize successfully for files under 20MB", async () => {
        vi.mocked(fsp.access).mockResolvedValueOnce(undefined);
        // Whisper mock
        vi.mocked(safeFetch).mockResolvedValueOnce({
            ok: true,
            text: () => Promise.resolve("Mock transcribed text from meeting.")
        } as any);
        // LLM mock
        vi.mocked(safeFetch).mockResolvedValueOnce({
            json: () => Promise.resolve({
                choices: [{ message: { content: "### Discussion Summary" } }]
            })
        } as any);

        const result = await execute({ audio_path: "exist.wav" });
        expect(result).toContain("### Discussion Summary");
    });

    it("should trigger audio chunking when file size exceeds 20MB", async () => {
        vi.mocked(fsp.access).mockResolvedValueOnce(undefined);
        // Overwrite size for this test
        vi.mocked(fsp.stat).mockResolvedValueOnce({ size: 25 * 1024 * 1024 } as any);
        vi.mocked(fsp.readdir).mockResolvedValueOnce(["exist_000.wav", "exist_001.wav"]);

        // Whisper mocks (two chunks)
        vi.mocked(safeFetch)
            .mockResolvedValueOnce({ ok: true, text: () => Promise.resolve("Chunk 1 transcript.") } as any)
            .mockResolvedValueOnce({ ok: true, text: () => Promise.resolve("Chunk 2 transcript.") } as any);
        // LLM mock
        vi.mocked(safeFetch).mockResolvedValueOnce({
            json: () => Promise.resolve({
                choices: [{ message: { content: "### Large Summary Report" } }]
            })
        } as any);

        const result = await execute({ audio_path: "exist.wav" });
        expect(fsp.readdir).toHaveBeenCalled();
        expect(result).toContain("### Large Summary Report");
    });

    it("should save summary to output_path and call safeRename", async () => {
        vi.mocked(fsp.access).mockResolvedValueOnce(undefined);
        // Whisper mock
        vi.mocked(safeFetch).mockResolvedValueOnce({
            ok: true,
            text: () => Promise.resolve("Meeting text")
        } as any);
        // LLM mock
        vi.mocked(safeFetch).mockResolvedValueOnce({
            json: () => Promise.resolve({
                choices: [{ message: { content: "Saved summary content" } }]
            })
        } as any);

        const result = await execute({ audio_path: "exist.wav", output_path: "test_output.md" });
        expect(fsp.writeFile).toHaveBeenCalled();
        expect(safeRename).toHaveBeenCalled();
        expect(result).toContain("test_output.md");
    });
});
