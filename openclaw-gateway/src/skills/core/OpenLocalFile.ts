import { exec } from "node:child_process";
import { promisify } from "node:util";
import * as os from "node:os";
import * as path from "node:path";
import { z } from "zod";
import { logger } from "@utils/logger";

const execAsync = promisify(exec);

const OpenFileSchema = z.object({
  targetPath: z.string().min(1, "targetPath is required"),
});

export const metadata = {
  name: "open_local_file",
  search_keywords: ["open_local_file","open local file","tệp","tài liệu","file"],
  description:
    "[AUTO_RUN] Open a local file (or directory) using the default Windows application (e.g., Open Word file, browser, image, or File Explorer).",
  parameters: {
    type: "object",
    properties: {
      targetPath: {
        type: "string",
        description:
          "Absolute or relative path to the file/directory to open. Example: 'D:/LIVA/package.json' or 'D:/Downloads'",
      },
    },
    required: ["targetPath"],
  },
};

export const execute = async (rawArgs: unknown): Promise<string> => {
  const parsed = OpenFileSchema.safeParse(rawArgs);
  if (!parsed.success) {
    return `[ValidationError] Invalid input: ${parsed.error.issues.map(i => i.message).join("; ")}`;
  }
  const args = parsed.data;
  try {
    const absolutePath = path.resolve(process.cwd(), args.targetPath);
    logger.info(
      `[Skill: open_local_file] Đang kích hoạt mở (Opening): ${absolutePath}`,
    );

    // Cú pháp đặc thù của Windows để mở native file dựa trên default app
    // Lưu ý: Dùng "" trống ban đầu làm title cho cmd tránh lỗi khi path có dấu cách
    const command = `start "" "${absolutePath}"`;

    const { stderr } = await execAsync(command);

    if (stderr && stderr.trim() !== "") {
      return `Command executed with OS warning: ${stderr}`;
    }

    return `File opened successfully: ${absolutePath}`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `Failed to open file: ${errMsg}`;
  }
};
