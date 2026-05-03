import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import * as fs from "node:fs/promises";
import * as path from "node:path";

const VoiceSchema = z.object({
  text: z.string().describe("Đoạn văn bản cần đọc thành tiếng"),
  volume: z.number().min(0).max(100).optional().default(100).describe("Âm lượng giọng đọc (0-100)"),
  rate: z.number().min(-10).max(10).optional().default(0).describe("Tốc độ đọc (-10 là cực chậm, 10 là cực nhanh)")
});

export const metadata = {
  name: "voice_speaker",
  description: "Cấp cho LIVA khả năng lên tiếng nói (Voice/Text-to-Speech) thông qua loa máy tính. LIVA có thể đọc to mọi thứ người dùng yêu cầu.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      text: { type: "string", description: "Nội dung cần đọc" },
      volume: { type: "number", description: "Âm lượng giọng (0-100)" },
      rate: { type: "number", description: "Tốc độ nói (-10 đến 10)" }
    },
    required: ["text"],
  },
};

// Hàm helper chạy lệnh PowerShell ngầm
async function runPowerShellDetached(scriptContent: string) {
    const rpaDir = path.join(process.cwd(), "data", "rpa_scripts");
    await fs.mkdir(rpaDir, { recursive: true });
    
    const tempFile = path.join(rpaDir, `voice_${Date.now()}.ps1`);
    await fs.writeFile(tempFile, scriptContent, "utf-8");
    
    // Sử dụng exec nhưng không await để tiến trình chạy ngầm (Zero-Blocking)
    exec(`powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${tempFile}"`, async (error) => {
        if (error) {
            logger.error(`[VoiceSpeaker] Lỗi tiến trình giọng nói: ${error.message}`);
        } else {
            logger.info(`[VoiceSpeaker] Đã phát âm thanh xong.`);
        }
        // Xoá file rác sau khi đọc xong
        await fs.unlink(tempFile).catch(() => {});
    });
}

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = VoiceSchema.parse(argsObj);

        // Chuẩn hóa text để an toàn (thoát chuỗi nháy đơn)
        const safeText = parsed.text.replace(/'/g, "''").replace(/\n/g, " ");

        const psScript = `
            Add-Type -AssemblyName System.Speech
            $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
            $synth.Volume = ${parsed.volume}
            $synth.Rate = ${parsed.rate}
            $synth.Speak('${safeText}')
        `;

        logger.info(`[VoiceSpeaker] Yêu cầu đọc văn bản (${safeText.length} ký tự). Đang đẩy vào luồng ngầm...`);
        
        // Kích hoạt tiến trình ngầm
        await runPowerShellDetached(psScript);

        // Trả về kết quả ngay lập tức để AI tiếp tục vòng lặp hội thoại
        return `[VOICE SUCCESS] Hệ thống đang phát âm thanh giọng đọc: "${parsed.text.substring(0, 50)}...". Bạn có thể tiếp tục trò chuyện trong lúc hệ thống đang nói.`;

    } catch (error: any) {
        logger.error(`[VoiceSpeaker] Lỗi: ${error.message}`);
        if (error instanceof z.ZodError) {
            return `[VOICE ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[VOICE ERROR] Lỗi hệ thống: ${error.message}`;
    }
};
