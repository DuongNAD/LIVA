import * as fs from "fs/promises";
import * as path from "node:path";
import { logger } from "../utils/logger";

export const metadata = {
  name: "delete_local_file",
  search_keywords: ["delete_local_file","delete local file","tệp","tài liệu","file"],
  description:
    "Xóa một tệp tin trên hệ thống (Delete a file). CẢNH BÁO: Chỉ sử dụng công cụ này khi người dùng yêu cầu xóa một cách rõ ràng.",
  parameters: {
    type: "object",
    properties: {
      filePath: {
        type: "string",
        description: "Đường dẫn tuyệt đối hoặc tương đối tới tệp tin cần xóa.",
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
        return `[LỖI BẢO MẬT]: Vùng \`${area}\` thuộc hệ thống lõi. Quyền xóa bị từ chối tuyệt đối để bảo vệ máy tính khỏi các hư hỏng tiềm ẩn.`;
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
      return `[LỖI BẢO MẬT]: Không được phép xóa tệp tin Boot của Windows!`;
    }
    // -----------------------------

    await fs.unlink(targetPath);
    return `Đã xóa tệp thành công (File deleted successfully): ${targetPath}`;
  } catch (error: any) {
    return `Lỗi khi xóa tệp (File deletion error): ${error.message}`;
  }
};
