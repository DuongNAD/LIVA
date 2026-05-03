import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import * as path from "node:path";
import * as fs from "node:fs/promises";
import Tesseract from "tesseract.js";
import clipboardy from "clipboardy";

const execAsync = promisify(exec);

const OCRSchema = z.object({
  lang: z.enum(["eng", "vie"]).optional().default("vie").describe("Ngôn ngữ cần nhận diện (eng hoặc vie)")
});

export const metadata = {
  name: "clipboard_ocr",
  description: "Mắt thần bóc tách chữ (OCR). Đọc hình ảnh hiện tại đang nằm trong Bộ nhớ đệm (Clipboard) và chuyển đổi thành văn bản Text. Rất tiện để lấy chữ từ màn hình không cho copy.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      lang: { type: "string", enum: ["eng", "vie"], description: "Ngôn ngữ để OCR (vie cho tiếng Việt, eng cho tiếng Anh)" }
    },
    required: [],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = OCRSchema.parse(argsObj);
        
        const tempDir = path.join(process.cwd(), "data", "temp");
        await fs.mkdir(tempDir, { recursive: true });
        const imgPath = path.join(tempDir, `ocr_clip_${Date.now()}.png`);

        // Dùng PowerShell để lấy ảnh từ Clipboard và lưu thành file PNG
        const psScript = `
            Add-Type -AssemblyName System.Windows.Forms
            Add-Type -AssemblyName System.Drawing
            if ([System.Windows.Forms.Clipboard]::ContainsImage()) {
                $image = [System.Windows.Forms.Clipboard]::GetImage()
                $image.Save("${imgPath}", [System.Drawing.Imaging.ImageFormat]::Png)
                Write-Output "SUCCESS"
            } else {
                Write-Output "NO_IMAGE"
            }
        `.replace(/\n/g, ';');

        logger.info(`[ClipboardOCR] Đang kiểm tra Clipboard xem có ảnh không...`);
        const { stdout } = await execAsync(`powershell.exe -NoProfile -Command "${psScript}"`);
        
        if (stdout.trim() !== "SUCCESS") {
            return `[OCR ERROR] Không tìm thấy hình ảnh nào trong Clipboard. Bạn hãy chụp một đoạn màn hình hoặc copy một tấm ảnh rồi gọi lại tôi nhé.`;
        }

        logger.info(`[ClipboardOCR] Bắt đầu chạy nhận diện OCR (Ngôn ngữ: ${parsed.lang})...`);
        
        // Chạy Tesseract OCR
        const workerData = await Tesseract.recognize(imgPath, parsed.lang);
        const textResult = workerData.data.text.trim();

        // Xóa file tạm
        await fs.unlink(imgPath).catch(() => {});

        if (!textResult) {
            return `[OCR RESULT] Đã quét ảnh nhưng không tìm thấy đoạn văn bản nào rõ ràng.`;
        }

        // Tự động ghi lại kết quả vào Clipboard cho user dán
        await clipboardy.write(textResult);

        return `[OCR SUCCESS] Đã nhận diện thành công văn bản và TỰ ĐỘNG CHÉP VÀO CLIPBOARD (Bạn có thể Ctrl+V ngay).
Nội dung nhận diện được:
-------------------
${textResult}
-------------------`;

    } catch (error: any) {
        logger.error(`[ClipboardOCR] Lỗi: ${error.message}`);
        if (error instanceof z.ZodError) {
            return `[OCR ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[OCR ERROR] Lỗi hệ thống: ${error.message}`;
    }
};
