import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import * as fs from "node:fs/promises";
import * as path from "node:path";

const execAsync = promisify(exec);

const RPASchema = z.object({
  action: z.enum(["mouse_move", "mouse_click", "type_text", "take_screenshot"]),
  x: z.number().optional().describe("Tọa độ X trên màn hình (cho mouse_move)"),
  y: z.number().optional().describe("Tọa độ Y trên màn hình (cho mouse_move)"),
  button: z.enum(["left", "right", "double"]).optional().default("left").describe("Nút chuột cần click"),
  text: z.string().optional().describe("Nội dung văn bản cần gõ (cho type_text)")
});

export const metadata = {
  name: "desktop_rpa",
  description: "Thao tác trực tiếp (Computer Use) trên màn hình OS Windows. LIVA hoạt động như một người thật: Di chuyển chuột, click, gõ bàn phím và chụp ảnh màn hình.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["mouse_move", "mouse_click", "type_text", "take_screenshot"] },
      x: { type: "number" },
      y: { type: "number" },
      button: { type: "string", enum: ["left", "right", "double"] },
      text: { type: "string" }
    },
    required: ["action"],
  },
};

// Hàm helper để chạy script PowerShell an toàn
async function runPowerShell(scriptContent: string): Promise<string> {
    const rpaDir = path.join(process.cwd(), "data", "rpa_scripts");
    await fs.mkdir(rpaDir, { recursive: true });
    
    // Tạo file tạm để tránh lỗi escape quotes khi chạy inline
    const tempFile = path.join(rpaDir, `temp_rpa_${Date.now()}.ps1`);
    await fs.writeFile(tempFile, scriptContent, "utf-8");
    
    try {
        const { stdout } = await execAsync(`powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${tempFile}"`);
        return stdout.trim();
    } finally {
        await fs.unlink(tempFile).catch(() => {});
    }
}

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = RPASchema.parse(argsObj);
        
        // Ghi chú: Dựa trên quyết định của User, kỹ năng này được thả tự do 100% (Không bọc HITLGuard)
        // để LIVA hoạt động trơn tru với tư cách là một trợ lý hoàn toàn đáng tin cậy.

        if (parsed.action === "take_screenshot") {
            const saveDir = path.join(process.cwd(), "data", "screenshots");
            await fs.mkdir(saveDir, { recursive: true });
            const filePath = path.join(saveDir, `desktop_${Date.now()}.png`);
            
            const psScript = `
                Add-Type -AssemblyName System.Windows.Forms
                Add-Type -AssemblyName System.Drawing
                $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
                $bitmap = New-Object System.Drawing.Bitmap $bounds.width, $bounds.height
                $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
                $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.size)
                $bitmap.Save("${filePath}", [System.Drawing.Imaging.ImageFormat]::Png)
                $graphics.Dispose()
                $bitmap.Dispose()
                Write-Output "${filePath}"
            `;
            await runPowerShell(psScript);
            logger.info(`[DesktopRPA] Đã chụp màn hình tại: ${filePath}`);
            return `[RPA SUCCESS] Đã chụp toàn bộ màn hình và lưu tại: ${filePath}\nBạn có thể đưa đường dẫn này vào VisionAnalyzer để nhờ não Vision đọc tọa độ nút bấm!`;
        }

        if (parsed.action === "mouse_move") {
            if (parsed.x === undefined || parsed.y === undefined) throw new Error("Cần truyền tọa độ x, y");
            
            const psScript = `
                $code = @'
                using System;
                using System.Runtime.InteropServices;
                public class Mouse {
                    [DllImport("user32.dll")]
                    public static extern bool SetCursorPos(int x, int y);
                }
'@
                Add-Type -TypeDefinition $code -Name "MouseMove" -Namespace "Win32" -PassThru | Out-Null
                [Win32.MouseMove]::SetCursorPos(${Math.round(parsed.x)}, ${Math.round(parsed.y)})
            `;
            await runPowerShell(psScript);
            logger.info(`[DesktopRPA] Đã dịch chuyển chuột tới (${parsed.x}, ${parsed.y})`);
            return `[RPA SUCCESS] Đã dịch chuyển con trỏ chuột tới (${parsed.x}, ${parsed.y}).`;
        }

        if (parsed.action === "mouse_click") {
            let flagDown = 0x0002; // LEFTDOWN
            let flagUp = 0x0004;   // LEFTUP
            if (parsed.button === "right") {
                flagDown = 0x0008; // RIGHTDOWN
                flagUp = 0x0010;   // RIGHTUP
            }
            
            const psScript = `
                $code = @'
                using System;
                using System.Runtime.InteropServices;
                public class MouseClick {
                    [DllImport("user32.dll")]
                    public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint cButtons, uint dwExtraInfo);
                }
'@
                Add-Type -TypeDefinition $code -Name "Clicker" -Namespace "Win32" -PassThru | Out-Null
                
                # Single Click
                [Win32.Clicker]::mouse_event(${flagDown}, 0, 0, 0, 0)
                [Win32.Clicker]::mouse_event(${flagUp}, 0, 0, 0, 0)
                
                ${parsed.button === "double" ? `
                Start-Sleep -Milliseconds 50
                [Win32.Clicker]::mouse_event(${flagDown}, 0, 0, 0, 0)
                [Win32.Clicker]::mouse_event(${flagUp}, 0, 0, 0, 0)
                ` : ""}
            `;
            await runPowerShell(psScript);
            logger.info(`[DesktopRPA] Đã click chuột (${parsed.button})`);
            return `[RPA SUCCESS] Đã thực hiện thao tác click chuột: ${parsed.button}.`;
        }

        if (parsed.action === "type_text") {
            if (!parsed.text) throw new Error("Cần truyền nội dung 'text' để gõ.");
            // Escape powershell string
            const safeText = parsed.text.replace(/'/g, "''");
            const psScript = `
                Add-Type -AssemblyName System.Windows.Forms
                [System.Windows.Forms.SendKeys]::SendWait('${safeText}')
            `;
            await runPowerShell(psScript);
            logger.info(`[DesktopRPA] Đã gõ đoạn văn bản: "${parsed.text}"`);
            return `[RPA SUCCESS] Đã gõ tự động đoạn văn bản vào màn hình.`;
        }

        return "Hành động không hợp lệ.";
    } catch (error: any) {
        logger.error(`[DesktopRPA] Lỗi: ${error.message}`);
        if (error instanceof z.ZodError) {
            return `[RPA ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[RPA ERROR] Lỗi hệ thống: ${error.message}`;
    }
};
