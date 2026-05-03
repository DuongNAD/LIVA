import { z } from "zod";
import { logger } from "@utils/logger";
import * as fs from "node:fs";
import * as crypto from "node:crypto";
import path from "node:path";

const HashSchema = z.object({
  filePath: z.string().min(1, "Thiếu đường dẫn tới file"),
  algorithm: z.enum(["md5", "sha1", "sha256", "sha512"]).optional().default("sha256"),
  verify: z.string().optional().describe("Hash kỳ vọng để so sánh (nếu cung cấp, sẽ kiểm tra tính toàn vẹn)"),
});

export const metadata = {
  name: "hash_checksum",
  description: "Tính toán Hash/Checksum của file (MD5, SHA1, SHA256, SHA512) bằng Stream không tốn RAM. Hỗ trợ xác minh tính toàn vẹn file bằng cách so sánh hash kỳ vọng.",
  kit: "DATA_KIT",
  search_keywords: ["hash", "checksum", "md5", "sha256", "verify", "integrity", "kiểm tra"],
  parameters: {
    type: "object",
    properties: {
      filePath: { type: "string", description: "Đường dẫn tới file cần tính hash" },
      algorithm: { type: "string", enum: ["md5", "sha1", "sha256", "sha512"], description: "Thuật toán hash (mặc định sha256)" },
      verify: { type: "string", description: "Hash kỳ vọng để so sánh — nếu cung cấp, sẽ xác minh tính toàn vẹn" },
    },
    required: ["filePath"],
  },
};

export const execute = async (argsObj: unknown): Promise<string> => {
    try {
        const parsed = HashSchema.parse(argsObj);
        const targetPath = path.resolve(process.cwd(), parsed.filePath);

        // Check file exists
        const stat = await fs.promises.stat(targetPath);
        if (!stat.isFile()) {
            return `[HASH ERROR] Đường dẫn không phải là một tập tin hợp lệ: ${targetPath}`;
        }

        logger.info(`[HashChecksum] Đang tính ${parsed.algorithm.toUpperCase()} cho file ${path.basename(targetPath)} (${(stat.size / 1024 / 1024).toFixed(2)} MB)...`);

        // Stream-based hashing — Zero RAM overhead cho file multi-GB
        const hash = await new Promise<string>((resolve, reject) => {
            const hasher = crypto.createHash(parsed.algorithm);
            const stream = fs.createReadStream(targetPath);

            stream.on("data", (chunk) => hasher.update(chunk));
            stream.on("end", () => resolve(hasher.digest("hex")));
            stream.on("error", (err) => reject(new Error(`Lỗi đọc file: ${err.message}`)));
        });

        let result = `[HASH RESULT]\n`;
        result += `- File: ${path.basename(targetPath)}\n`;
        result += `- Size: ${(stat.size / 1024 / 1024).toFixed(2)} MB\n`;
        result += `- Algorithm: ${parsed.algorithm.toUpperCase()}\n`;
        result += `- Hash: ${hash}\n`;

        // Integrity verification
        if (parsed.verify) {
            const expected = parsed.verify.toLowerCase().trim();
            const match = hash === expected;
            result += `\n--- Xác Minh Tính Toàn Vẹn ---\n`;
            result += `- Hash kỳ vọng: ${expected}\n`;
            result += `- Kết quả: ${match ? "✅ KHỚP — File toàn vẹn" : "❌ KHÔNG KHỚP — File có thể bị hỏng hoặc bị thay đổi!"}\n`;

            if (!match) {
                logger.warn(`[HashChecksum] ⚠️ Integrity mismatch cho file ${path.basename(targetPath)}`);
            }
        }

        logger.info(`[HashChecksum] Hoàn tất: ${parsed.algorithm.toUpperCase()} = ${hash.substring(0, 16)}...`);
        return result;

    } catch (error: unknown) {
        const msg = error instanceof z.ZodError
            ? `Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`
            : (error instanceof Error ? error.message : "Unknown error");
        logger.error(`[HashChecksum] Lỗi: ${msg}`);
        return `[HASH ERROR] ${msg}`;
    }
};
