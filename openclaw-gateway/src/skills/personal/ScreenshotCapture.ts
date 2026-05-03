import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import * as fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";

const execAsync = promisify(exec);

const ScreenshotSchema = z.object({
  outputPath: z.string().optional().describe("Đường dẫn lưu ảnh PNG (mặc định: Desktop/screenshot_<timestamp>.png)"),
  region: z.enum(["full", "active"]).optional().default("full").describe("Chụp toàn màn hình (full) hoặc cửa sổ đang active (active)"),
});

export const metadata = {
  name: "screenshot_capture",
  description: "Chụp ảnh màn hình Desktop (toàn bộ hoặc cửa sổ đang active) và lưu ra file PNG. Sử dụng PowerShell tích hợp, không cần thư viện ngoài.",
  kit: "PERSONAL_KIT",
  search_keywords: ["screenshot", "capture", "chụp màn hình", "screen", "ảnh"],
  parameters: {
    type: "object",
    properties: {
      outputPath: { type: "string", description: "Đường dẫn file ảnh đầu ra" },
      region: { type: "string", enum: ["full", "active"], description: "Chế độ: toàn bộ màn hình hoặc cửa sổ active" },
    },
    required: [],
  },
};

export const execute = async (argsObj: unknown): Promise<string> => {
    try {
        const parsed = ScreenshotSchema.parse(argsObj);

        const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
        const defaultPath = path.join(os.homedir(), "Desktop", `screenshot_${timestamp}.png`);
        const outputPath = parsed.outputPath
            ? path.resolve(process.cwd(), parsed.outputPath)
            : defaultPath;

        // Ensure output directory exists
        await fs.mkdir(path.dirname(outputPath), { recursive: true });

        logger.info(`[ScreenshotCapture] Chụp màn hình (${parsed.region}), lưu tại: ${outputPath}`);

        if (parsed.region === "active") {
            // Capture active window only using .NET Alt+PrintScreen equivalent
            const psScript = `
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# Get active window bounds
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinAPI {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

$hwnd = [WinAPI]::GetForegroundWindow()
$rect = New-Object WinAPI+RECT
[void][WinAPI]::GetWindowRect($hwnd, [ref]$rect)
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top

$bitmap = New-Object System.Drawing.Bitmap($width, $height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($width, $height)))
$bitmap.Save('${outputPath.replace(/\\/g, "\\\\")}', [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
Write-Output "OK"
            `.trim();

            await execAsync(
                `powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "${psScript.replace(/"/g, '\\"').replace(/\n/g, "; ")}"`,
                { timeout: 15000 }
            );
        } else {
            // Full screen capture
            const psScript = `
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bitmap = New-Object System.Drawing.Bitmap($screen.Width, $screen.Height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
$bitmap.Save('${outputPath.replace(/\\/g, "\\\\")}', [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
Write-Output "OK"
            `.trim();

            await execAsync(
                `powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "${psScript.replace(/"/g, '\\"').replace(/\n/g, "; ")}"`,
                { timeout: 15000 }
            );
        }

        // Verify file was created
        const stat = await fs.stat(outputPath);

        logger.info(`[ScreenshotCapture] ✅ Hoàn tất: ${outputPath} (${(stat.size / 1024).toFixed(1)} KB)`);

        return `[SCREENSHOT SUCCESS] Đã chụp ảnh màn hình thành công!\n- Chế độ: ${parsed.region === "active" ? "Cửa sổ active" : "Toàn màn hình"}\n- File: ${outputPath}\n- Kích thước: ${(stat.size / 1024).toFixed(1)} KB`;

    } catch (error: unknown) {
        const msg = error instanceof z.ZodError
            ? `Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`
            : (error instanceof Error ? error.message : "Unknown error");
        logger.error(`[ScreenshotCapture] Lỗi: ${msg}`);
        return `[SCREENSHOT ERROR] ${msg}`;
    }
};
