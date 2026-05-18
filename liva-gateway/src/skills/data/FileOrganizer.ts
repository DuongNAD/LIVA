import { safeRename } from '../../utils/FileUtils';
import { z } from "zod";
import { logger } from "@utils/logger";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import os from "node:os";

const OrganizerSchema = z.object({
  targetDirectory: z.string().describe("Đường dẫn thư mục cần dọn dẹp (Mặc định là thư mục Downloads)"),
});

export const metadata = {
  name: "file_organizer",
  description: "[AUTO_RUN] Auto-organize files in a directory (e.g., Downloads, Desktop). Files sorted into: Images, Documents, Media, Archives, Setups.",
  kit: "DATA_KIT",
  parameters: {
    type: "object",
    properties: {
      targetDirectory: { type: "string" }
    },
    required: ["targetDirectory"],
  },
};

const CATEGORIES: Record<string, string[]> = {
    "Images": [".jpg", ".jpeg", ".png", ".gif", ".bmp", ".svg", ".webp"],
    "Documents": [".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".txt", ".csv", ".md"],
    "Media": [".mp4", ".mkv", ".avi", ".mp3", ".wav", ".flac"],
    "Archives": [".zip", ".rar", ".7z", ".tar", ".gz"],
    "Setups": [".exe", ".msi", ".bat", ".sh", ".dmg"],
    "Code": [".js", ".ts", ".py", ".html", ".css", ".json"]
};

function getCategory(ext: string): string {
    const lowerExt = ext.toLowerCase();
    for (const [category, exts] of Object.entries(CATEGORIES)) {
        if (exts.includes(lowerExt)) return category;
    }
    return "Others";
}

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = OrganizerSchema.parse(argsObj);
        
        let targetDir = parsed.targetDirectory;
        // Hỗ trợ alias "Downloads" hoặc "Desktop"
        if (targetDir.toLowerCase() === "downloads") targetDir = path.join(os.homedir(), "Downloads");
        if (targetDir.toLowerCase() === "desktop") targetDir = path.join(os.homedir(), "Desktop");

        targetDir = path.resolve(targetDir);

        try {
            const stat = await fs.stat(targetDir);
            if (!stat.isDirectory()) throw new Error("Đường dẫn không phải là thư mục.");
        } catch (e) {
            return `[ORGANIZER ERROR] Không tìm thấy thư mục: ${targetDir}`;
        }

        const files = await fs.readdir(targetDir);
        let movedCount = 0;
        const stats: Record<string, number> = {};

        for (const file of files) {
            const filePath = path.join(targetDir, file);
            const stat = await fs.stat(filePath).catch(() => null);
            
            // Bỏ qua thư mục và file ẩn
            if (!stat || !stat.isFile() || file.startsWith(".")) continue;

            const ext = path.extname(file);
            if (!ext) continue;

            const category = getCategory(ext);
            const categoryDir = path.join(targetDir, category);
            
            await fs.mkdir(categoryDir, { recursive: true });
            
            const newPath = path.join(categoryDir, file);
            await safeRename(filePath, newPath);
            
            movedCount++;
            stats[category] = (stats[category] || 0) + 1;
        }

        if (movedCount === 0) {
            return `[ORGANIZER RESULT] Thư mục ${targetDir} đã gọn gàng, không có file nào cần phân loại thêm.`;
        }

        const summary = Object.entries(stats).map(([k, v]) => `- ${k}: ${v} files`).join("\n");
        logger.info(`[FileOrganizer] Đã dọn dẹp ${movedCount} files tại ${targetDir}`);
        
        return `[ORGANIZER SUCCESS] Đã dọn dẹp thành công thư mục ${targetDir}\nTổng số file đã di chuyển: ${movedCount}\nChi tiết:\n${summary}`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[FileOrganizer] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[ORGANIZER ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[ORGANIZER ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
