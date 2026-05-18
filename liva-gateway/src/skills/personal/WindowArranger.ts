import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

const WindowSchema = z.object({
  action: z.enum(["snap_left", "snap_right", "maximize"]).describe("Hành động điều khiển cửa sổ đang mở")
});

export const metadata = {
  name: "window_arranger",
  description: "[AUTO_RUN] Window management: Split screen (snap current window left/right) or maximize window for efficient multitasking.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["snap_left", "snap_right", "maximize"] }
    },
    required: ["action"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = WindowSchema.parse(argsObj);
        
        let operationCode = "";
        if (parsed.action === "snap_left") {
            operationCode = `
                [Util.Win]::ShowWindow($hwnd, 1) | Out-Null
                [Util.Win]::MoveWindow($hwnd, $area.X, $area.Y, $halfW, $area.Height, $true) | Out-Null
            `;
        } else if (parsed.action === "snap_right") {
            operationCode = `
                [Util.Win]::ShowWindow($hwnd, 1) | Out-Null
                [Util.Win]::MoveWindow($hwnd, $area.X + $halfW, $area.Y, $halfW, $area.Height, $true) | Out-Null
            `;
        } else if (parsed.action === "maximize") {
            operationCode = `[Util.Win]::ShowWindow($hwnd, 3) | Out-Null`; // 3 is SW_MAXIMIZE
        }

        const psScript = `
            Add-Type -AssemblyName System.Windows.Forms
            $code = @'
            using System;
            using System.Runtime.InteropServices;
            public class Win {
                [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
                [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);
                [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
            }
'@
            Add-Type -TypeDefinition $code -Name "WinAPI" -Namespace "Util" -PassThru | Out-Null
            
            $hwnd = [Util.Win]::GetForegroundWindow()
            $area = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
            $halfW = [math]::Round($area.Width / 2)
            
            ${operationCode}
        `;

        await execAsync(`powershell.exe -NoProfile -Command "${psScript}"`);
        
        logger.info(`[WindowArranger] Đã thực hiện thao tác: ${parsed.action}`);
        return `[WINDOW SUCCESS] Đã thực hiện thao tác '${parsed.action}' trên cửa sổ hiện tại thành công.`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[WindowArranger] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[WINDOW ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[WINDOW ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
