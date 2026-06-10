import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { promisify } from "node:util";
import { execute, metadata } from "../../../src/skills/personal/SpotifyController";

// Hoist mock references
const { mockSafeFetch, mockExecAsync } = vi.hoisted(() => ({
  mockSafeFetch: vi.fn(),
  mockExecAsync: vi.fn()
}));

vi.mock("@utils/HttpClient", () => ({
  safeFetch: mockSafeFetch
}));

vi.mock("@utils/logger", () => ({
  logger: {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn()
  }
}));

vi.mock("node:child_process", () => {
  const execFn = (...args: any[]) => {};
  (execFn as any)[promisify.custom] = mockExecAsync;
  return { exec: execFn };
});

describe("Skill - SpotifyController", () => {
  const originalEnv = process.env;

  beforeEach(() => {
    vi.clearAllMocks();
    process.env = { ...originalEnv };
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it("should export metadata", () => {
    expect(metadata.name).toBe("spotify_controller");
    expect(metadata.kit).toBe("PERSONAL_KIT");
  });

  describe("Web API Mode (SPOTIFY_ACCESS_TOKEN present)", () => {
    beforeEach(() => {
      process.env.SPOTIFY_ACCESS_TOKEN = "test-spotify-token";
    });

    it("should control play action", async () => {
      mockSafeFetch.mockResolvedValueOnce({
        status: 204,
        ok: true
      });

      const result = await execute({ action: "play" });
      expect(mockSafeFetch).toHaveBeenCalledWith(
        "https://api.spotify.com/v1/me/player/play",
        expect.objectContaining({ method: "PUT" })
      );
      expect(result).toContain("[SPOTIFY WEB API SUCCESS]");
      expect(result).toContain("play");
    });

    it("should control pause action", async () => {
      mockSafeFetch.mockResolvedValueOnce({
        status: 204,
        ok: true
      });

      const result = await execute({ action: "pause" });
      expect(mockSafeFetch).toHaveBeenCalledWith(
        "https://api.spotify.com/v1/me/player/pause",
        expect.objectContaining({ method: "PUT" })
      );
      expect(result).toContain("[SPOTIFY WEB API SUCCESS]");
      expect(result).toContain("pause");
    });

    it("should control next action", async () => {
      mockSafeFetch.mockResolvedValueOnce({
        status: 204,
        ok: true
      });

      const result = await execute({ action: "next" });
      expect(mockSafeFetch).toHaveBeenCalledWith(
        "https://api.spotify.com/v1/me/player/next",
        expect.objectContaining({ method: "POST" })
      );
      expect(result).toContain("[SPOTIFY WEB API SUCCESS]");
    });

    it("should control prev action", async () => {
      mockSafeFetch.mockResolvedValueOnce({
        status: 204,
        ok: true
      });

      const result = await execute({ action: "prev" });
      expect(mockSafeFetch).toHaveBeenCalledWith(
        "https://api.spotify.com/v1/me/player/previous",
        expect.objectContaining({ method: "POST" })
      );
      expect(result).toContain("[SPOTIFY WEB API SUCCESS]");
    });

    it("should control set_volume action", async () => {
      mockSafeFetch.mockResolvedValueOnce({
        status: 204,
        ok: true
      });

      const result = await execute({ action: "set_volume", volume: 75 });
      expect(mockSafeFetch).toHaveBeenCalledWith(
        "https://api.spotify.com/v1/me/player/volume?volume_percent=75",
        expect.objectContaining({ method: "PUT" })
      );
      expect(result).toContain("[SPOTIFY WEB API SUCCESS]");
    });

    it("should fail set_volume action if volume is missing", async () => {
      const result = await execute({ action: "set_volume" });
      expect(result).toContain("[SPOTIFY ERROR] Volume is required");
    });

    it("should control play_track action", async () => {
      mockSafeFetch.mockResolvedValueOnce({
        status: 204,
        ok: true
      });

      const result = await execute({ action: "play_track", track_uri: "spotify:track:123" });
      expect(mockSafeFetch).toHaveBeenCalledWith(
        "https://api.spotify.com/v1/me/player/play",
        expect.objectContaining({
          method: "PUT",
          body: JSON.stringify({ uris: ["spotify:track:123"] })
        })
      );
      expect(result).toContain("[SPOTIFY WEB API SUCCESS]");
    });

    it("should fail play_track action if track_uri is missing", async () => {
      const result = await execute({ action: "play_track" });
      expect(result).toContain("[SPOTIFY ERROR] track_uri is required");
    });

    it("should retrieve status with get_status action", async () => {
      const mockStatus = {
        item: {
          name: "Bohemian Rhapsody",
          artists: [{ name: "Queen" }]
        },
        shuffle_state: true,
        repeat_state: "context"
      };
      mockSafeFetch.mockResolvedValueOnce({
        status: 200,
        ok: true,
        json: async () => mockStatus
      });

      const result = await execute({ action: "get_status" });
      expect(mockSafeFetch).toHaveBeenCalledWith(
        "https://api.spotify.com/v1/me/player",
        expect.objectContaining({ method: "GET" })
      );
      expect(result).toContain("[SPOTIFY WEB API SUCCESS]");
      expect(result).toContain("Bohemian Rhapsody");
      expect(result).toContain("Queen");
    });
  });

  describe("Local PowerShell Fallback Mode (SPOTIFY_ACCESS_TOKEN not present)", () => {
    beforeEach(() => {
      delete process.env.SPOTIFY_ACCESS_TOKEN;
      mockExecAsync.mockResolvedValue({ stdout: "", stderr: "" });
    });

    it("should fall back to local play/pause for play action", async () => {
      const result = await execute({ action: "play" });
      expect(mockExecAsync).toHaveBeenCalledTimes(1);
      expect(mockExecAsync.mock.calls[0][0]).toContain("powershell.exe");
      expect(mockExecAsync.mock.calls[0][0]).toContain("179"); // VK_MEDIA_PLAY_PAUSE
      expect(result).toContain("[SPOTIFY LOCAL SUCCESS]");
      expect(result).toContain("Phát/Tạm dừng nhạc");
    });

    it("should fall back to local play/pause for pause action", async () => {
      const result = await execute({ action: "pause" });
      expect(mockExecAsync).toHaveBeenCalledTimes(1);
      expect(mockExecAsync.mock.calls[0][0]).toContain("powershell.exe");
      expect(mockExecAsync.mock.calls[0][0]).toContain("179");
      expect(result).toContain("[SPOTIFY LOCAL SUCCESS]");
    });

    it("should fall back to local next for next action", async () => {
      const result = await execute({ action: "next" });
      expect(mockExecAsync).toHaveBeenCalledTimes(1);
      expect(mockExecAsync.mock.calls[0][0]).toContain("176"); // VK_MEDIA_NEXT_TRACK
      expect(result).toContain("[SPOTIFY LOCAL SUCCESS]");
    });

    it("should fall back to local prev for prev action", async () => {
      const result = await execute({ action: "prev" });
      expect(mockExecAsync).toHaveBeenCalledTimes(1);
      expect(mockExecAsync.mock.calls[0][0]).toContain("177"); // VK_MEDIA_PREV_TRACK
      expect(result).toContain("[SPOTIFY LOCAL SUCCESS]");
    });

    it("should fall back to local volume change for set_volume action", async () => {
      const result = await execute({ action: "set_volume", volume: 80 });
      expect(mockExecAsync).toHaveBeenCalledTimes(1);
      expect(mockExecAsync.mock.calls[0][0]).toContain("175"); // VK_VOLUME_UP (since volume > 50)
      expect(result).toContain("[SPOTIFY LOCAL SUCCESS]");
    });

    it("should fall back to local play/pause for play_track action", async () => {
      const result = await execute({ action: "play_track", track_uri: "spotify:track:123" });
      expect(mockExecAsync).toHaveBeenCalledTimes(1);
      expect(mockExecAsync.mock.calls[0][0]).toContain("179");
      expect(result).toContain("[SPOTIFY LOCAL SUCCESS]");
      expect(result).toContain("Lưu ý: Việc chọn track cụ thể");
    });

    it("should return status message for get_status action", async () => {
      const result = await execute({ action: "get_status" });
      expect(mockExecAsync).not.toHaveBeenCalled();
      expect(result).toContain("[SPOTIFY LOCAL SUCCESS]");
      expect(result).toContain("không có Token để lấy trạng thái chi tiết");
    });
  });
});
