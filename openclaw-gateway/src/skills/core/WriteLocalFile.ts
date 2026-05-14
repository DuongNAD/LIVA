import * as fs from 'node:fs/promises';
import * as path from "node:path";
import { logger } from "@utils/logger";

export const metadata = {
  name: "write_local_file",
  search_keywords: ["write_local_file","write local file","tệp","tài liệu","file"],
  description:
    "[ASK_FIRST] Create a new file or overwrite an existing file on the local computer.",
  parameters: {
    type: "object",
    properties: {
      filePath: {
        type: "string",
        description:
          "Absolute or relative path to the file to write. Example: 'logs/report.txt'",
      },
      content: {
        type: "string",
        description:
          "Text content or source code to write into the file.",
      },
    },
    required: ["filePath", "content"],
  },
};

export const execute = async (args: {
  filePath: string;
  content: string;
}): Promise<string> => {
  try {
    const targetPath = path.resolve(process.cwd(), args.filePath);
    logger.info(`[Skill: write_local_file] Đang kiểm tra an ninh trước khi ghi dữ liệu vào: ${targetPath}`);

    // --- 🛡️ PATH GUARDRAILS 🛡️ ---
    const lowerPath = targetPath.toLowerCase();
    const forbiddenAreas = [
      "c:\\windows",
      "c:\\program files",
      "c:\\program files (x86)",
      "c:\\programdata",
      "c:\\users\\default",
    ];

    // Chặn ghi đè trực tiếp lên mâm đĩa C:\ (Cần ít nhất 1 cấp folder con)
    if (lowerPath === "c:\\" || lowerPath === "c:/") {
      return `[SECURITY_ERROR]: Writing directly to root drive is forbidden.`;
    }

    for (const area of forbiddenAreas) {
      if (lowerPath.startsWith(area)) {
        logger.warn(`[SECURITY ALERT] Lờ qua yêu cầu ghi file vào vùng cấm: ${area}`);
        return `[SECURITY_ERROR]: Path \`${area}\` is OS-protected. Write permission denied. Use project or Documents folder instead.`;
      }
    }
    // -----------------------------

    // Lấy thư mục chứa tệp để đảm bảo nó tồn tại
    const dirName = path.dirname(targetPath);
    await fs.mkdir(dirName, { recursive: true });

    // Atomic Write: .tmp + rename() prevents corrupt file on crash
    const tmpPath = `${targetPath}.tmp`;
    await fs.writeFile(tmpPath, args.content, "utf-8");
    await fs.rename(tmpPath, targetPath);
    return `File written successfully: ${targetPath}`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `File write error: ${errMsg}`;
  }
};
