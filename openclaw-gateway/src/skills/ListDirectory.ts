import * as fs from "fs/promises";
import * as path from "path";

// Khối Metadata định nghĩa (Definition) cho AI
export const metadata = {
  name: "list_directory",
  description:
    "Liệt kê danh sách các tệp và thư mục con bên trong một đường dẫn cụ thể trên máy tính. Kỹ năng này giúp AI hiểu rõ cấu trúc dự án (Project structure).",
  parameters: {
    type: "object",
    properties: {
      targetPath: {
        type: "string",
        description:
          "Đường dẫn tuyệt đối hoặc tương đối tới thư mục cần xem. Dùng '.' để xem thư mục hiện tại.",
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
    console.log(
      `[Skill: list_directory] Đang quét thư mục (Scanning directory): ${resolvedPath}`,
    );

    const items = await fs.readdir(resolvedPath, { withFileTypes: true });

    let result = `Cấu trúc thư mục tại ${resolvedPath}:\n`;
    for (const item of items) {
      const type = item.isDirectory() ? "[Thư mục - Folder]" : "[Tệp - File]";
      result += `- ${type} ${item.name}\n`;
    }
    return result;
  } catch (error: any) {
    return `Lỗi khi quét thư mục (Directory scan error): ${error.message}`;
  }
};
