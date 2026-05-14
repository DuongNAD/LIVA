import * as fs from 'node:fs/promises';
import * as path from "node:path";
import { logger } from "@utils/logger";

export const metadata = {
  name: "delete_local_file",
  search_keywords: ["delete_local_file","delete local file","tệp","tài liệu","file"],
  description:
    "[ASK_FIRST] Delete a file on the system. WARNING: Only use this tool when explicitly requested by the user.",
  parameters: {
    type: "object",
    properties: {
      filePath: {
        type: "string",
        description: "Absolute or relative path to the file to delete.",
      },
    },
    required: ["filePath"],
  },
};

export const execute = async (args: { filePath: string }): Promise<string> => {
  try {
    const targetPath = path.resolve(process.cwd(), args.filePath);
    logger.info(
      `[Skill: delete_local_file] Đang phân tích an ninh tệp trước khi xóa: ${targetPath}`,
    );

    // --- 🛡️ PATH GUARDRAILS 🛡️ ---
    const lowerPath = targetPath.toLowerCase();
    const forbiddenAreas = [
      "c:\\windows",
      "c:\\program files",
      "c:\\program files (x86)",
      "c:\\programdata",
      "c:\\users\\default",
    ];

    for (const area of forbiddenAreas) {
      if (lowerPath.startsWith(area)) {
        logger.warn(
          `[SECURITY ALERT] Lờ qua yêu cầu xóa file vùng hệ thống: ${area}`,
        );
        return `[SECURITY_ERROR]: Path \`${area}\` is a system-protected zone. Delete permission denied.`;
      }
    }

    // Chặn xóa các tệp tin nguy hiểm định dạng gốc (Boot logic)
    const rootCritical = [
      "c:\\bootmgr",
      "c:\\ntldr",
      "c:\\hiberfil.sys",
      "c:\\pagefile.sys",
    ];
    if (rootCritical.includes(lowerPath)) {
      return `[SECURITY_ERROR]: Deleting Windows boot files is strictly forbidden.`;
    }
    // -----------------------------

    await fs.unlink(targetPath);
    return `File deleted successfully: ${targetPath}`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `File deletion error: ${errMsg}`;
  }
};
