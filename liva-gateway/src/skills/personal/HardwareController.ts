import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

const HardwareSchema = z.object({
  action: z.enum(["set_volume", "set_brightness"]),
  level: z.number().min(0).max(100).describe("Mức độ (từ 0 đến 100)")
});

export const metadata = {
  name: "hardware_controller",
  search_keywords: ["phần cứng", "hardware", "độ sáng", "brightness", "wifi", "bluetooth"],
  description: "[ASK_FIRST] Hardware control (Monitor & Speakers). Use to adjust volume or screen brightness by percentage (0-100).",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["set_volume", "set_brightness"] },
      level: { type: "number", description: "Level from 0 to 100%" }
    },
    required: ["action", "level"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = HardwareSchema.parse(argsObj);

        if (parsed.action === "set_brightness") {
            const script = `(Get-WmiObject -Namespace root/WMI -Class WmiMonitorBrightnessMethods).WmiSetBrightness(1, ${parsed.level})`;
            await execAsync(`powershell.exe -Command "${script}"`);
            logger.info(`[Hardware] Đã chỉnh độ sáng màn hình thành ${parsed.level}%`);
            return `[HARDWARE SUCCESS] Đã chỉnh độ sáng màn hình thành ${parsed.level}%.`;
        }

        if (parsed.action === "set_volume") {
            // Trick zero-dependency: Gửi phím VolumeDown 50 lần để về 0, sau đó gửi VolumeUp để đạt mức mong muốn
            // Mỗi lần bấm phím ảo thường tăng/giảm 2% âm lượng
            const upSteps = Math.round(parsed.level / 2);
            
            const psScript = `
                $obj = new-object -com wscript.shell
                for ($i = 0; $i -lt 50; $i++) { $obj.SendKeys([char]174) }
                for ($i = 0; $i -lt ${upSteps}; $i++) { $obj.SendKeys([char]175) }
            `.replace(/\n/g, ';');
            
            await execAsync(`powershell.exe -Command "${psScript}"`);
            logger.info(`[Hardware] Đã chỉnh âm lượng hệ thống thành ${parsed.level}%`);
            return `[HARDWARE SUCCESS] Đã chỉnh âm lượng hệ thống về khoảng ${parsed.level}%.`;
        }

        return "Hành động không hợp lệ.";
    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[Hardware] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[HARDWARE ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[HARDWARE ERROR] Lỗi hệ thống: ${errMsg}. (Lưu ý: Màn hình rời có thể không hỗ trợ WMI Brightness)`;
    }
};
