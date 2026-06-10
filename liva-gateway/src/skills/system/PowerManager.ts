import { exec } from "child_process";
import { promisify } from "util";
import { logger } from "../../utils/logger";
import { z } from "zod";

const execAsync = promisify(exec);

const PowerSchema = z.object({
  action: z.enum(["lock", "sleep"]).describe("Hành động nguồn: lock (khoá máy) hoặc sleep (ngủ)"),
});

export const metadata = {
  name: "power_manager",
  search_keywords: ["khoá máy", "lock pc", "sleep máy", "ngủ máy", "tắt màn hình"],
  description:
    "[AUTO_RUN] Quản lý nguồn máy tính. Hỗ trợ các lệnh: Khoá máy (Lock), Chế độ ngủ (Sleep). Tương thích với Windows và macOS.",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["lock", "sleep"] }
    },
    required: ["action"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
  if (process.platform !== "win32" && process.platform !== "darwin") {
      return `[SYSTEM_ERROR] Kỹ năng power_manager hiện chỉ hỗ trợ Windows và macOS. Hệ điều hành hiện tại là: ${process.platform}`;
  }

  try {
    const parsed = PowerSchema.parse(argsObj);
    logger.info(`[Skill: power_manager] Đang thực thi lệnh hệ thống: ${parsed.action}`);

    if (parsed.action === "lock") {
        if (process.platform === "win32") {
            await execAsync("rundll32.exe user32.dll,LockWorkStation");
        } else if (process.platform === "darwin") {
            await execAsync("pmset displaysleepnow");
        }
        return `[Power Manager] ✅ Đã thực hiện lệnh KHOÁ MÁY (Lock Display) thành công.`;
    } 
    
    if (parsed.action === "sleep") {
        if (process.platform === "win32") {
            exec("rundll32.exe powrprof.dll,SetSuspendState 0,1,0", (error) => {
                if (error) {
                    logger.error(`[power_manager] Lỗi khi sleep: ${error.message}`);
                }
            });
        } else if (process.platform === "darwin") {
            exec("osascript -e 'tell application \"System Events\" to sleep'", (error) => {
                if (error) {
                    logger.error(`[power_manager] Lỗi khi sleep macOS: ${error.message}`);
                }
            });
        }
        return `[Power Manager] 💤 Đang chuyển máy tính sang chế độ Ngủ (Sleep)...`;
    }

    return `[SYSTEM_ERROR] Lệnh không hợp lệ: ${parsed.action}`;

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[power_manager] Error: ${errMsg}`);
    return `[SYSTEM_ERROR] Lỗi khi quản lý nguồn: ${errMsg}`;
  }
};
