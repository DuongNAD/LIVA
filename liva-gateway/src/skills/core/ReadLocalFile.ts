import * as fs from 'node:fs/promises';
import * as path from "node:path";
import { z } from "zod";
import { logger } from "@utils/logger";

const FilePathSchema = z.object({
  filePath: z.string().min(1, "filePath is required"),
});

// Đây chính là phần Metadata (đóng vai trò như YAML frontmatter)
// để mô tả cho AI biết kỹ năng này dùng để làm gì và cần tham số nào.
export const metadata = {
  name: "read_local_file",
  search_keywords: ["read_local_file","read local file","tệp","tài liệu","file"],
  description:
    "[AUTO_RUN] Read the content of a file on the local computer. Use this skill when the user asks to view source code or read documents.",
  parameters: {
    type: "object",
    properties: {
      filePath: {
        type: "string",
        description:
          "Absolute or relative path to the file to read. Example: 'package.json' or 'D:/project/README.md'",
      },
    },
    required: ["filePath"],
  },
};

// Đây là phần Execution Logic (Logic thực thi)
export const execute = async (rawArgs: unknown): Promise<string> => {
  const parsed = FilePathSchema.safeParse(rawArgs);
  if (!parsed.success) {
    return `[ValidationError] Invalid input: ${parsed.error.issues.map(i => i.message).join("; ")}`;
  }
  const args = parsed.data;
  try {
    const resolvedPath = path.resolve(process.cwd(), args.filePath);
    const workspaceRoot = path.resolve(process.cwd());
    if (!resolvedPath.startsWith(workspaceRoot + path.sep) && !resolvedPath.startsWith(workspaceRoot)) {
      logger.warn(`[SECURITY] Path traversal attempt blocked: ${resolvedPath}`);
      return `[SecurityError] Path must be within workspace directory.`;
    }

    const targetPath = resolvedPath;
    logger.info(
      `[Skill: read_local_file] Đang cố gắng đọc tệp (Attempting to read file) tại: ${targetPath}`,
    );

    // Prevent reading binary files that crash the LLM context
    const ext = path.extname(targetPath).toLowerCase();
    const binaryExts = ['.pdf', '.jpg', '.jpeg', '.png', '.gif', '.zip', '.exe', '.dll', '.mp4', '.mp3'];
    if (binaryExts.includes(ext)) {
        return `⚠️ Cảnh báo: Tệp "${targetPath}" là định dạng nhị phân/tài liệu đóng (${ext}). Không thể đọc trực tiếp văn bản! Vui lòng dùng lệnh \`open_local_file\` để mở tệp này cho Sếp tự xem.`;
    }

    const content = await fs.readFile(targetPath, "utf-8");
    return `File content:\n\n${content}`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `File read error: ${errMsg}`;
  }
};
