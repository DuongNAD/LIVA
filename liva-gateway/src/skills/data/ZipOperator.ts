import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import * as path from "node:path";

const execAsync = promisify(exec);

const ZipSchema = z.object({
  action: z.enum(["compress", "extract"]).describe("Hành động: Nén (compress) hoặc Giải nén (extract)"),
  sourcePath: z.string().describe("Đường dẫn file/thư mục nguồn"),
  destinationPath: z.string().describe("Đường dẫn đích (thư mục giải nén hoặc tên file .zip sẽ tạo ra)")
});

export const metadata = {
  name: "zip_operator",
  search_keywords: ["nén file", "giải nén", "zip", "unzip", "compress", "extract", "archive", "rar"],
  description: "[AUTO_RUN] Zip/Unzip: Compress or extract entire project/directory quickly. Supports backup or preparing files for submission.",
  kit: "DATA_KIT",
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["compress", "extract"] },
      sourcePath: { type: "string", description: "VD: data/Nop_bai" },
      destinationPath: { type: "string", description: "VD: data/Final.zip" }
    },
    required: ["action", "sourcePath", "destinationPath"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = ZipSchema.parse(argsObj);
        
        const source = path.resolve(process.cwd(), parsed.sourcePath);
        const destination = path.resolve(process.cwd(), parsed.destinationPath);

        let psScript = "";

        if (parsed.action === "compress") {
            logger.info(`[ZipOperator] Đang nén file từ ${source} thành ${destination}`);
            // Force parameter để ghi đè nếu file zip đã tồn tại
            psScript = `Compress-Archive -Path "${source}" -DestinationPath "${destination}" -Force`;
        } else {
            logger.info(`[ZipOperator] Đang giải nén file từ ${source} vào ${destination}`);
            psScript = `Expand-Archive -LiteralPath "${source}" -DestinationPath "${destination}" -Force`;
        }

        await execAsync(`powershell.exe -NoProfile -Command "${psScript}"`);
        
        return `[ZIP SUCCESS] Thao tác ${parsed.action === "compress" ? "Nén (Zip)" : "Giải nén (Extract)"} đã hoàn tất.\nNguồn: ${source}\nĐích: ${destination}`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[ZipOperator] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[ZIP ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[ZIP ERROR] Lỗi thao tác nén/giải nén: ${errMsg}`;
    }
};
