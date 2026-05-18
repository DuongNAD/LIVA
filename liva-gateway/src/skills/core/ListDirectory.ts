import * as fs from 'node:fs/promises';
import * as path from "node:path";
import { logger } from "@utils/logger";

// Khối Metadata định nghĩa (Definition) cho AI
export const metadata = {
  name: "list_directory",
  search_keywords: ["list_directory","list directory"],
  description:
    "[AUTO_RUN] List files and subdirectories inside a specific directory on the computer. This skill helps the AI understand project structure.",
  parameters: {
    type: "object",
    properties: {
      targetPath: {
        type: "string",
        description:
          "Absolute or relative path to the directory to list. Use '.' for the current directory.",
      },
    },
    required: ["targetPath"],
  },
};

// Khối Logic thực thi (Execution Logic)
export const execute = async (args: {
  targetPath: string;
}): Promise<string> => {
  try {
    const resolvedPath = path.resolve(process.cwd(), args.targetPath);
    logger.info(
      `[Skill: list_directory] Đang quét thư mục (Scanning directory): ${resolvedPath}`,
    );

    const items = await fs.readdir(resolvedPath, { withFileTypes: true });

    let result = `Directory listing of ${resolvedPath}:\n`;
    for (const item of items) {
      const type = item.isDirectory() ? "[DIR]" : "[FILE]";
      result += `- ${type} ${item.name}\n`;
    }
    return result;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `Directory scan error: ${errMsg}`;
  }
};
