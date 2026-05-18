import { exec } from "node:child_process";
import { logger } from "@utils/logger";
import { promisify } from 'node:util';
import fs from "node:fs";
import path from "node:path";

const execAsync = promisify(exec);

export const metadata = {
  name: "git_sync_project",
  search_keywords: ["git_sync_project","git sync project"],
  description:
    "[AUTO_RUN] Auto-sync (Push) internal project source code to Git Repository (phân tích tin nhắn người dùng để lấy tên dự án và tạo commit message). Thường được gọi khi user nhắn tin qua Zalo/Mess nhờ đẩy code thay khi họ đang ở xa.",
  parameters: {
    type: "object",
    properties: {
      projectName: {
        type: "string",
        description:
          "Project directory name to push (e.g., LIVA, EduConnect, Sentinel). Extracted from user request.",
      },
      commitMessage: {
        type: "string",
        description:
          "Thông điệp ghi chú commit (Commit message) do AI tự phân tích ra từ ý định nhắn nhủ của user, nên là một câu tóm tắt chuyên nghiệp cho các thay đổi.",
      },
    },
    required: ["projectName", "commitMessage"],
  },
};

export const execute = async (args: {
  projectName: string;
  commitMessage: string;
}): Promise<string> => {
  try {
    const baseDir = process.env.PROJECTS_DIR || "E:\\Project";
    const projectPath = path.join(baseDir, args.projectName);

    if (!fs.existsSync(projectPath)) {
      return `[LỖI] Không tìm thấy dự án nào có tên "${args.projectName}" tại ổ cứng (${baseDir}). Hãy đọc danh sách các dự án khả dụng và nhờ User cung cấp lại đúng tên thư mục.`;
    }

    logger.info(
      `[Skill: git_sync_project] Đang xử lý tự động đồng bộ Git cho dự án: ${args.projectName}`,
    );

    let resultLog = `Báo cáo quá trình đẩy code (Push) thư mục ${projectPath}:\n`;
    const execOpts = { cwd: projectPath };

    // 1. Kiểm tra tồn tại thư mục .git
    if (!fs.existsSync(path.join(projectPath, ".git"))) {
      return `[LỖI HỆ THỐNG] Thư mục ${args.projectName} chưa được khởi tạo Git repository (không tìm thấy .git). AI không thể Push. Hãy phản hồi báo người dùng phải Add Remote Github vào thư mục này trước.`;
    }

    // 2. Git add .
    try {
      await execAsync("git add .", execOpts);
      resultLog += "✅ Đã thu thập toàn bộ các thay đổi (git add .)\n";
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      return `[LỖI] Xảy ra lỗi khi chạy lệnh git add: ${errMsg}`;
    }

    // 3. Git commit
    try {
      // Escape để tránh lỗi bash khi user message có ngoặc kép
      const safeMessage = args.commitMessage.replaceAll('"', '\\"');
      const { stdout } = await execAsync(
        `git commit -m "${safeMessage}"`,
        execOpts,
      );
      resultLog += `✅ Đã đánh dấu commit với lời nhắn: "${args.commitMessage}"\n`;
      resultLog += `[Chi tiết Git] -> ${stdout.trim()}\n`;
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      const execErr = e as { stdout?: string; stderr?: string };
      if (execErr.stdout && execErr.stdout.includes("nothing to commit")) {
        return `[THÔNG BÁO] Hiện tại project "${args.projectName}" không có thay đổi mã nguồn nào mới so với Git (Nothing to commit). Mã đồng nhất.`;
      }
      return `[LỖI] Xảy ra lỗi khi chạy lệnh git commit: ${errMsg}\n${execErr.stdout || ""}`;
    }

    // 4. Git push
    try {
      const { stderr, stdout } = await execAsync(`git push`, execOpts);
      resultLog += "✅ PUSH CODE THÀNH CÔNG LÊN REPOSITORY!\n";
      if (stdout) resultLog += `[Chi tiết Out] -> ${stdout.trim()}\n`;
      if (stderr) resultLog += `[Chi tiết Err] -> ${stderr.trim()}\n`; // Git push thường đẩy log dạng diag vào stderr
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      const execErr = e as { stdout?: string; stderr?: string };
      return `[LỖI PUSH] Quá trình push code thất bại (Có thể do lỗi mạng, chưa setup Remote hoặc có nhánh mới bị Conflict): ${errMsg}\n${execErr.stderr || ""}\n${execErr.stdout || ""}`;
    }

    return resultLog;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `[LỖI NGHIÊM TRỌNG] Xảy ra lỗi thư viện khi kích hoạt kỹ năng Git: ${errMsg}`;
  }
};
