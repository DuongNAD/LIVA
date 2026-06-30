import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

const MediaSchema = z.object({
  action: z.enum(["play_pause", "next_track", "prev_track", "mute", "volume_up", "volume_down"]).describe("Hành động điều khiển Media")
});

export const metadata = {
  name: "media_controller",
  search_keywords: ["phát nhạc", "dừng nhạc", "chuyển bài", "tắt tiếng", "tăng âm lượng", "giảm âm lượng", "âm thanh", "volume", "music", "next", "media"],
  description: "[AUTO_RUN] Control PC media (Spotify, Youtube, Volume...). Supports: Play/Pause, Next, Prev, Mute, Volume Up, Volume Down.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["play_pause", "next_track", "prev_track", "mute", "volume_up", "volume_down"] }
    },
    required: ["action"],
  },
};

const sendMediaKey = async (keyCode: number, count: number = 1) => {
    // Gọi Win32 API `keybd_event` qua PowerShell in-memory
    let loopCode = "";
    for(let i = 0; i < count; i++) {
        loopCode += `$api::keybd_event(${keyCode}, 0, 1, 0); $api::keybd_event(${keyCode}, 0, 3, 0); `;
    }
    const psCommand = `
        $code = '[DllImport(\\"user32.dll\\")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, uint dwExtraInfo);'
        $api = Add-Type -MemberDefinition $code -Name 'Win32' -Namespace 'API' -PassThru
        ${loopCode}
    `.replace(/\n/g, ';');
    
    await execAsync(`powershell.exe -NoProfile -Command "${psCommand}"`);
};

// Windows VK media-key codes + repeat count per action.
const WIN_VK: Record<string, [number, number]> = {
    play_pause: [179, 1], next_track: [176, 1], prev_track: [177, 1],
    mute: [173, 1], volume_up: [175, 5], volume_down: [174, 5],
};

const ACTION_LABEL: Record<string, string> = {
    play_pause: "Phát/Tạm dừng nhạc", next_track: "Chuyển bài tiếp theo",
    prev_track: "Quay lại bài trước", mute: "Tắt/Bật âm lượng",
    volume_up: "Tăng âm lượng", volume_down: "Giảm âm lượng",
};

// macOS: transport keys target the running media app (Spotify → Music);
// volume/mute go through system volume via AppleScript.
const MAC_TRANSPORT: Record<string, string> = {
    play_pause: "playpause", next_track: "next track", prev_track: "previous track",
};

async function sendMediaMac(action: string): Promise<void> {
    let cmd: string;
    if (action in MAC_TRANSPORT) {
        const verb = MAC_TRANSPORT[action];
        const lines = [
            `if application "Spotify" is running then`,
            `tell application "Spotify" to ${verb}`,
            `else if application "Music" is running then`,
            `tell application "Music" to ${verb}`,
            `end if`,
        ];
        cmd = `osascript -e '${lines.join("' -e '")}'`;
    } else if (action === "mute") {
        cmd = `osascript -e 'set volume output muted (not (output muted of (get volume settings)))'`;
    } else if (action === "volume_up") {
        cmd = `osascript -e 'set volume output volume (((output volume of (get volume settings)) + 10) as integer)'`;
    } else {
        cmd = `osascript -e 'set volume output volume (((output volume of (get volume settings)) - 10) as integer)'`;
    }
    await execAsync(cmd);
}

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = MediaSchema.parse(argsObj);
        const actionName = ACTION_LABEL[parsed.action];

        if (process.platform === "darwin") {
            await sendMediaMac(parsed.action);
        } else {
            const [vk, count] = WIN_VK[parsed.action];
            await sendMediaKey(vk, count);
        }

        logger.info(`[MediaController] Đã thực thi lệnh: ${actionName} (${process.platform})`);
        return `[MEDIA SUCCESS] Đã thực thi lệnh '${actionName}' thành công.`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[MediaController] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[MEDIA ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[MEDIA ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
