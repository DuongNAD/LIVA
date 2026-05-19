import { z } from "zod";
import { logger } from "@utils/logger";
import * as fs from "node:fs/promises";
import * as fsSync from "node:fs";
import * as readline from "node:readline";
import path from "node:path";

// Zod Schema cho Log actions
const LogAnalyzerSchema = z.object({
  filePath: z.string().min(1, "Thiếu đường dẫn file log"),
  lines: z.number().optional().default(1000).describe("Số dòng cuối cần lấy (mặc định 1000)"),
  keyword: z.string().optional().describe("Từ khóa filter tùy chọn"),
});

export const metadata = {
  name: "log_analyzer",
  search_keywords: ["log", "nhật ký", "lỗi hệ thống", "error log", "debug", "xem log", "tail"],
  description: "[AUTO_RUN] Extract and analyze system logs. Ultra-fast tail reading of large log files without blocking RAM.",
  kit: "DEVOPS_KIT",
  parameters: {
    type: "object",
    properties: {
      filePath: {
        type: "string",
        description: "Absolute or relative path to log file (e.g., '/var/log/syslog' or 'logs/app.log').",
      },
      lines: {
        type: "number",
        description: "Number of last lines to read (default: 1000).",
      },
      keyword: {
        type: "string",
        description: "If provided, only return lines containing this keyword (case-insensitive).",
      }
    },
    required: ["filePath"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = LogAnalyzerSchema.parse(argsObj);
        const { filePath, lines, keyword } = parsed;
        const targetPath = path.resolve(process.cwd(), filePath);

        const stat = await fs.stat(targetPath);
        if (!stat.isFile()) {
            throw new Error(`Đường dẫn không phải là một tập tin hợp lệ: ${targetPath}`);
        }

        logger.info(`[LogAnalyzer] Đang đọc ${lines} dòng cuối từ ${targetPath}...`);

        return new Promise((resolve, reject) => {
            // Thuật toán: Ước lượng 250 bytes / dòng. Đọc từ vị trí bytes an toàn ở cuối file
            // Tránh đọc toàn bộ file 10GB từ đầu.
            const estimatedBytesPerLine = 250;
            const bytesToRead = lines * estimatedBytesPerLine;
            const startPos = Math.max(0, stat.size - bytesToRead);

            const stream = fsSync.createReadStream(targetPath, { 
                encoding: "utf-8", 
                start: startPos 
            });

            const rl = readline.createInterface({
                input: stream,
                crlfDelay: Infinity
            });

            const ringBuffer: string[] = [];
            const keywordLower = keyword ? keyword.toLowerCase() : null;

            // Xử lý stream bất đồng bộ
            rl.on('line', (line) => {
                // Nếu điểm bắt đầu stream không phải ở đầu file, dòng đầu tiên có thể bị vỡ (bị cắt giữa chừng).
                // Ta bỏ qua hoặc chấp nhận. Do lấy 1000 dòng cuối nên dòng vỡ ở đầu không sao.
                
                if (keywordLower) {
                    if (line.toLowerCase().includes(keywordLower)) {
                        ringBuffer.push(line);
                    }
                } else {
                    ringBuffer.push(line);
                }

                // Cắt tỉa mảng để không tràn RAM
                if (ringBuffer.length > lines * 2) { // Buffer an toàn
                    ringBuffer.splice(0, ringBuffer.length - lines);
                }
            });

            rl.on('close', () => {
                let resultLines = ringBuffer;
                if (resultLines.length > lines) {
                    resultLines = resultLines.slice(resultLines.length - lines);
                }

                if (startPos > 0 && resultLines.length > 0) {
                    // Cắt bỏ dòng đầu tiên vì nó có thể bị vỡ do startPos ngẫu nhiên ở giữa một dòng
                    resultLines.shift();
                }

                const resultStr = resultLines.join('\n');
                
                const outputMsg = `[LOG ANALYZER SUCCESS] File: ${path.basename(targetPath)}
Tổng dung lượng file: ${(stat.size / 1024 / 1024).toFixed(2)} MB
Trích xuất: ${resultLines.length} dòng cuối cùng.${keyword ? `\nBộ lọc từ khóa: "${keyword}"` : ""}
--------------------------------------------------\n
${resultStr}`;

                resolve(outputMsg);
            });

            rl.on('error', (err) => reject(new Error(`Lỗi đọc stream: ${err.message}`)));
            stream.on('error', (err) => reject(new Error(`Lỗi mở file: ${err.message}`)));
        });

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[LogAnalyzer] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[LOG ERROR] Sai định dạng tham số: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[LOG ERROR] Trích xuất thất bại: ${errMsg}`;
    }
};
