import * as fs from "fs/promises";
import * as path from "path";
import { logger } from "../utils/logger";

// Đây chính là phần Metadata (đóng vai trò như YAML frontmatter)
// để mô tả cho AI biết kỹ năng này dùng để làm gì và cần tham số nào.
export const metadata = {
  name: "read_local_file",
  search_keywords: ["read_local_file","read local file","tệp","tài liệu","file"],
  description:
    "Đọc nội dung của một tệp tin trên máy tính cục bộ (Local computer). Sử dụng kỹ năng này khi người dùng yêu cầu xem mã nguồn hoặc đọc tài liệu.",
  parameters: {
    type: "object",
    properties: {
      filePath: {
        type: "string",
        description:
          "Đường dẫn tuyệt đối hoặc tương đối tới tệp tin cần đọc. Ví dụ: 'package.json' hoặc 'D:/project/README.md'",
      },
    },
    required: ["filePath"],
  },
};

// Đây là phần Execution Logic (Logic thực thi)
export const execute = async (args: { filePath: string }): Promise<string> => {
  try {
    const targetPath = path.resolve(process.cwd(), args.filePath);
    logger.info(
      `[Skill: read_local_file] Đang cố gắng đọc tệp (Attempting to read file) tại: ${targetPath}`,
    );

    const content = await fs.readFile(targetPath, "utf-8");
    return `Nội dung tệp tin:\n\n${content}`;
  } catch (error: any) {
    return `Lỗi khi đọc tệp (File read error): ${error.message}. Hãy thông báo cho người dùng biết.`;
  }
};
