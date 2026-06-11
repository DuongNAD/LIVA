/* eslint-disable @typescript-eslint/no-explicit-any */
import { z } from "zod";
import { logger } from "@utils/logger";
import { safeFetch } from "@utils/HttpClient";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

const SpotifyControllerSchema = z.object({
  action: z.enum(["play", "pause", "next", "prev", "set_volume", "get_status", "play_track"]),
  volume: z.number().min(0).max(100).optional(),
  track_uri: z.string().optional()
});

export const metadata = {
  name: "spotify_controller",
  search_keywords: ["spotify", "music", "phát nhạc", "dừng nhạc", "volume", "âm lượng", "chuyển bài"],
  description: "[AUTO_RUN] Control Spotify music playback. Uses Spotify Web API if SPOTIFY_ACCESS_TOKEN is present; otherwise falls back to local Windows Media control keys.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["play", "pause", "next", "prev", "set_volume", "get_status", "play_track"],
        description: "Spotify action to perform."
      },
      volume: {
        type: "number",
        minimum: 0,
        maximum: 100,
        description: "Volume percentage to set (0-100) (required for set_volume)."
      },
      track_uri: {
        type: "string",
        description: "Spotify track URI to play (e.g., 'spotify:track:4iV5W9u5GxkCU37ytdsbZ5') (required for play_track)."
      }
    },
    required: ["action"]
  }
};

const sendMediaKey = async (keyCode: number, count: number = 1) => {
  let loopCode = "";
  for (let i = 0; i < count; i++) {
    loopCode += `$api::keybd_event(${keyCode}, 0, 1, 0); $api::keybd_event(${keyCode}, 0, 3, 0); `;
  }
  const psCommand = `
    $code = '[DllImport(\\"user32.dll\\")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, uint dwExtraInfo);'
    $api = Add-Type -MemberDefinition $code -Name 'Win32' -Namespace 'API' -PassThru
    ${loopCode}
  `.replace(/\n/g, ';');
  
  await execAsync(`powershell.exe -NoProfile -Command "${psCommand}"`);
};

export const execute = async (argsObj: any): Promise<string> => {
  try {
    const parsed = SpotifyControllerSchema.parse(argsObj);
    const { action, volume, track_uri } = parsed;

    const token = process.env.SPOTIFY_ACCESS_TOKEN;

    if (token) {
      logger.info(`[SpotifyController] Executing action '${action}' via Spotify Web API...`);
      const headers = {
        "Authorization": `Bearer ${token}`,
        "Accept": "application/json",
        "Content-Type": "application/json"
      };

      let url = "";
      let method = "PUT";
      let body: any = null;

      switch (action) {
        case "play":
          url = "https://api.spotify.com/v1/me/player/play";
          break;
        case "pause":
          url = "https://api.spotify.com/v1/me/player/pause";
          break;
        case "next":
          url = "https://api.spotify.com/v1/me/player/next";
          method = "POST";
          break;
        case "prev":
          url = "https://api.spotify.com/v1/me/player/previous";
          method = "POST";
          break;
        case "set_volume":
          if (volume === undefined) {
            return `[SPOTIFY ERROR] Volume is required for set_volume.`;
          }
          url = `https://api.spotify.com/v1/me/player/volume?volume_percent=${volume}`;
          break;
        case "play_track":
          if (!track_uri) {
            return `[SPOTIFY ERROR] track_uri is required for play_track.`;
          }
          url = "https://api.spotify.com/v1/me/player/play";
          body = { uris: [track_uri] };
          break;
        case "get_status":
          url = "https://api.spotify.com/v1/me/player";
          method = "GET";
          break;
      }

      const options: RequestInit = {
        method,
        headers
      };

      if (body) {
        options.body = JSON.stringify(body);
      }

      const response = await safeFetch(url, options);
      
      let data: any = null;
      if (response.status !== 204 && response.status !== 205) {
        try {
          data = await response.json();
        } catch {
          // Response body is empty or not JSON
        }
      }

      let statusMsg = "";
      if (action === "get_status") {
        if (data && data.item) {
          statusMsg = `Currently playing: "${data.item.name}" by ${data.item.artists.map((a: any) => a.name).join(", ")} (Shuffle: ${data.shuffle_state ? "ON" : "OFF"}, Repeat: ${data.repeat_state})`;
        } else {
          statusMsg = "Playback status retrieved, but no active device or track playing.";
        }
      } else {
        statusMsg = `Action '${action}' executed successfully via Web API.`;
      }

      return `[SPOTIFY WEB API SUCCESS] ${statusMsg}`;

    } else {
      logger.info(`[SpotifyController] SPOTIFY_ACCESS_TOKEN is not present. Falling back to local Windows Media Keys...`);
      
      let localMsg = "";
      switch (action) {
        case "play":
        case "pause":
          await sendMediaKey(179); // VK_MEDIA_PLAY_PAUSE
          localMsg = "Phát/Tạm dừng nhạc (Local)";
          break;
        case "next":
          await sendMediaKey(176); // VK_MEDIA_NEXT_TRACK
          localMsg = "Chuyển bài tiếp theo (Local)";
          break;
        case "prev":
          await sendMediaKey(177); // VK_MEDIA_PREV_TRACK
          localMsg = "Quay lại bài trước (Local)";
          break;
        case "set_volume":
          const direction = volume && volume > 50 ? 175 : 174;
          await sendMediaKey(direction, 3);
          localMsg = `Điều chỉnh âm lượng cục bộ (Local, hướng phím: ${direction === 175 ? 'Tăng' : 'Giảm'})`;
          break;
        case "play_track":
          await sendMediaKey(179); // VK_MEDIA_PLAY_PAUSE
          localMsg = `Phát nhạc (Local). Lưu ý: Việc chọn track cụ thể '${track_uri}' yêu cầu SPOTIFY_ACCESS_TOKEN.`;
          break;
        case "get_status":
          localMsg = "Đang chạy chế độ điều khiển phím Media cục bộ (không có Token để lấy trạng thái chi tiết từ Web API).";
          break;
      }

      return `[SPOTIFY LOCAL SUCCESS] ${localMsg}`;
    }

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[SpotifyController] Error: ${errMsg}`);
    if (error instanceof z.ZodError) {
      return `[SPOTIFY ERROR] Parameter validation failed: ${error.issues.map(e => e.message).join(", ")}`;
    }
    return `[SPOTIFY ERROR] Failed to execute Spotify action: ${errMsg}`;
  }
};
