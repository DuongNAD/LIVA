import { exec } from "child_process";
import { promisify } from "util";
import * as path from "path";

const execAsync = promisify(exec);

export const metadata = {
  name: "open_local_file",
  search_keywords: ["open_local_file","open local file","tệp","tài liệu","file"],
  description:
    "Mở một tệp tin (hoặc thư mục) cục bộ bằng phần mềm mặc định của hệ điều hành Windows (Ví dụ: Mở file Word, trình duyệt, hình ảnh, hoặc File Explorer).",
  parameters: {
    type: "object",
    properties: {
      targetPath: {
        type: "string",
        description:
          "Đường dẫn tuyệt đối hoặc tương đối tới tệp tin/thư mục cần mở. Ví dụ: 'D:/LIVA/package.json' hoặc 'D:/Downloads'",
      },
    },
    required: ["targetPath"],
  },
};

export const execute = async (args: {
  targetPath: string;
}): Promise<string> => {
  try {
    const absolutePath = path.resolve(process.cwd(), args.targetPath);
    console.log(
      `[Skill: open_local_file] Đang kích hoạt mở (Opening): ${absolutePath}`,
    );

    // Cú pháp đặc thù của Windows để mở native file dựa trên default app
    // Lưu ý: Dùng "" trống ban đầu làm title cho cmd tránh lỗi khi path có dấu cách
    const command = `start "" "${absolutePath}"`;

    const { stderr } = await execAsync(command);

    if (stderr && stderr.trim() !== "") {
      return `Lệnh thực thi chạy (Command executed), nhưng có cảnh báo OS: ${stderr}`;
    }

    return `Đã ra lệnh mở thành công (Successfully requested OS to open file): ${absolutePath}`;
  } catch (error: any) {
    return `Mở thất bại (Failed to open file): ${error.message}`;
  }
};
