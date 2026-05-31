import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

const LauncherSchema = z.object({
  appName: z.string().describe("Tên ứng dụng hoặc executable (VD: chrome, code, notepad, mspaint, spotify)")
});

export const metadata = {
  name: "app_launcher",
  search_keywords: ["mở app", "chạy ứng dụng", "launch", "open app", "khởi động", "start program"],
  description: "[ASK_FIRST] Ultra-fast launch of installed software/applications (e.g., VS Code, Chrome, Notepad, etc.).",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      appName: { type: "string", description: "Application process name (e.g., 'code' for VS Code, 'chrome' for Google Chrome)" }
    },
    required: ["appName"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = LauncherSchema.parse(argsObj);
        
        // Loại bỏ các ký tự có thể gây lỗi injection
        const safeAppName = parsed.appName.replace(/[^a-zA-Z0-9_\-.\s]/g, "");

        // Dùng PowerShell để start process, nếu lỗi dùng cmd start fallback
        const psScript = `
            try {
                Start-Process "${safeAppName}" -ErrorAction Stop
            } catch {
                cmd.exe /c start "" "${safeAppName}"
            }
        `.replace(/\n/g, ';');

        await execAsync(`powershell.exe -Command "${psScript}"`);
        
        logger.info(`[AppLauncher] Đã gửi lệnh khởi chạy ứng dụng: ${safeAppName}`);
        return `[LAUNCHER SUCCESS] Đã gửi tín hiệu khởi chạy ứng dụng "${safeAppName}". Giao diện phần mềm sẽ hiện lên trong giây lát.`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[AppLauncher] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[LAUNCHER ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[LAUNCHER ERROR] Không thể mở ứng dụng này. (Lỗi: ${errMsg})`;
    }
};
