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
  search_keywords: ["đọc văn bản", "TTS", "text to speech", "nói", "giọng nói", "phát âm"],
  description: "[AUTO_RUN] Text-to-Speech via computer speakers. LIVA can read aloud any content the user requests.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {
      text: { type: "string", description: "Text content to read aloud" },
      volume: { type: "number", description: "Voice volume (0-100)" },
      rate: { type: "number", description: "Speech rate (-10 to 10)" }
    },
    required: ["text"],
  },
};

// Hàm helper chạy file script ngầm (Zero-Blocking) rồi dọn file tạm.
async function runDetached(command: string, tempFile: string) {
    // Không await tiến trình con để vòng lặp hội thoại tiếp tục ngay.
    exec(command, async (error) => {
        if (error) {
            logger.error(`[VoiceSpeaker] Lỗi tiến trình giọng nói: ${error.message}`);
        } else {
            logger.info(`[VoiceSpeaker] Đã phát âm thanh xong.`);
        }
        await fs.unlink(tempFile).catch(() => {});
    });
}

// Build lệnh TTS theo nền tảng. Văn bản luôn đi qua file tạm để tránh injection.
async function buildTtsCommand(text: string, volume: number, rate: number): Promise<{ command: string; tempFile: string }> {
    const rpaDir = path.join(process.cwd(), "data", "rpa_scripts");
    await fs.mkdir(rpaDir, { recursive: true });

    if (process.platform === "darwin") {
        // macOS `say -f <file>`: rate là words-per-minute; map -10..10 → ~60..300 wpm.
        const wpm = Math.max(60, Math.min(300, 180 + rate * 12));
        // `[[volm x]]` (0.0-1.0) điều khiển âm lượng nội tuyến trong file đầu vào của `say`.
        const tempFile = path.join(rpaDir, `voice_${Date.now()}.txt`);
        await fs.writeFile(tempFile, `[[volm ${(volume / 100).toFixed(2)}]]${text}`, "utf-8");
        return { command: `say -r ${wpm} -f "${tempFile}"`, tempFile };
    }

    // Windows: System.Speech qua file .ps1.
    const safeText = text.replace(/'/g, "''").replace(/\n/g, " ");
    const tempFile = path.join(rpaDir, `voice_${Date.now()}.ps1`);
    const psScript = `
            Add-Type -AssemblyName System.Speech
            $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
            $synth.Volume = ${volume}
            $synth.Rate = ${rate}
            $synth.Speak('${safeText}')
        `;
    await fs.writeFile(tempFile, psScript, "utf-8");
    return { command: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${tempFile}"`, tempFile };
}

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = VoiceSchema.parse(argsObj);

        logger.info(`[VoiceSpeaker] Yêu cầu đọc văn bản (${parsed.text.length} ký tự) trên ${process.platform}. Đang đẩy vào luồng ngầm...`);

        const { command, tempFile } = await buildTtsCommand(parsed.text, parsed.volume, parsed.rate);
        await runDetached(command, tempFile);

        // Trả về kết quả ngay lập tức để AI tiếp tục vòng lặp hội thoại
        return `[VOICE SUCCESS] Hệ thống đang phát âm thanh giọng đọc: "${parsed.text.substring(0, 50)}...". Bạn có thể tiếp tục trò chuyện trong lúc hệ thống đang nói.`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[VoiceSpeaker] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[VOICE ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[VOICE ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
