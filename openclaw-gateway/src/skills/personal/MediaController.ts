import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

const MediaSchema = z.object({
  action: z.enum(["play_pause", "next_track", "prev_track", "mute"]).describe("Hành động điều khiển Media")
});

export const metadata = {
  name: "media_controller",
  description: "[AUTO_RUN] Media playback control (Spotify, Youtube, Apple Music). Commands: Play/Pause, Next, Prev, Mute.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["play_pause", "next_track", "prev_track", "mute"] }
    },
    required: ["action"],
  },
};

const sendMediaKey = async (keyCode: number) => {
    // Gọi Win32 API `keybd_event` qua PowerShell in-memory
    const psCommand = `
        $code = '[DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, uint dwExtraInfo);'
        $api = Add-Type -MemberDefinition $code -Name 'Win32' -Namespace 'API' -PassThru
        $api::keybd_event(${keyCode}, 0, 1, 0)
        $api::keybd_event(${keyCode}, 0, 3, 0)
    `.replace(/\n/g, ';');
    
    await execAsync(`powershell.exe -NoProfile -Command "${psCommand}"`);
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = MediaSchema.parse(argsObj);
        let actionName = "";

        switch (parsed.action) {
            case "play_pause":
                await sendMediaKey(179); // VK_MEDIA_PLAY_PAUSE
                actionName = "Phát/Tạm dừng nhạc";
                break;
            case "next_track":
                await sendMediaKey(176); // VK_MEDIA_NEXT_TRACK
                actionName = "Chuyển bài tiếp theo";
                break;
            case "prev_track":
                await sendMediaKey(177); // VK_MEDIA_PREV_TRACK
                actionName = "Quay lại bài trước";
                break;
            case "mute":
                await sendMediaKey(173); // VK_VOLUME_MUTE
                actionName = "Tắt/Bật âm lượng";
                break;
        }

        logger.info(`[MediaController] Đã thực thi lệnh: ${actionName}`);
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
