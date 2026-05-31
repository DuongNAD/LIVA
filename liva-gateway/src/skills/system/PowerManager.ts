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
    "[AUTO_RUN][WINDOWS ONLY] Quản lý nguồn máy tính. Hỗ trợ các lệnh: Khoá máy (Lock), Chế độ ngủ (Sleep). Yêu cầu hệ điều hành Windows.",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["lock", "sleep"] }
    },
    required: ["action"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
  if (process.platform !== "win32") {
      return `[SYSTEM_ERROR] Kỹ năng power_manager hiện chỉ hỗ trợ hệ điều hành Windows. Hệ điều hành hiện tại là: ${process.platform}`;
  }

  try {
    const parsed = PowerSchema.parse(argsObj);
    logger.info(`[Skill: power_manager] Đang thực thi lệnh hệ thống: ${parsed.action}`);

    if (parsed.action === "lock") {
        await execAsync("rundll32.exe user32.dll,LockWorkStation");
        return `[Power Manager] ✅ Đã thực hiện lệnh KHOÁ MÁY (Lock PC) thành công.`;
    } 
    
    if (parsed.action === "sleep") {
        // Sleep command (Requires hibernation to be disabled for true sleep, otherwise it hibernates. 
        // We just use the standard SetSuspendState 0,1,0 which puts it to sleep/hibernate based on OS settings)
        // Note: Running this will immediately put the PC to sleep!
        // We shouldn't await it because the PC will sleep before the promise resolves.
        exec("rundll32.exe powrprof.dll,SetSuspendState 0,1,0", (error) => {
            if (error) {
                logger.error(`[power_manager] Lỗi khi sleep: ${error.message}`);
            }
        });
        return `[Power Manager] 💤 Đang chuyển máy tính sang chế độ Ngủ (Sleep)...`;
    }

    return `[SYSTEM_ERROR] Lệnh không hợp lệ: ${parsed.action}`;

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[power_manager] Error: ${errMsg}`);
    return `[SYSTEM_ERROR] Lỗi khi quản lý nguồn: ${errMsg}`;
  }
};
