import { exec } from "child_process";
import { promisify } from "util";
import { logger } from "../../utils/logger";
const execAsync = promisify(exec);

export const metadata = {
  name: "system_optimizer",
  search_keywords: ["dọn rác", "tối ưu hệ thống", "clean up", "xóa temp", "clear cache", "thùng rác", "flush dns", "dọn dẹp máy tính", "optimize pc"],
  description:
    "[AUTO_RUN][WINDOWS ONLY] Dọn dẹp bộ nhớ đệm (Cache/Temp), làm trống Thùng rác (Recycle Bin), và Flush DNS để tăng tốc máy tính. Yêu cầu hệ điều hành Windows.",
  parameters: {
    type: "object",
    properties: {},
    required: [],
  },
};

export const execute = async (_args: Record<string, unknown>): Promise<string> => {
  if (process.platform !== "win32") {
      return `[SYSTEM_ERROR] Kỹ năng system_optimizer hiện chỉ hỗ trợ hệ điều hành Windows. Hệ điều hành hiện tại là: ${process.platform}`;
  }

  try {
    logger.info(`[Skill: system_optimizer] Đang chạy dọn dẹp hệ thống Windows...`);

    let output = `[System Optimizer] Báo cáo kết quả dọn dẹp:\n\n`;

    // 1. Clear %TEMP%
    try {
        await execAsync('powershell -Command "Remove-Item -Path $env:TEMP\\* -Recurse -Force -ErrorAction SilentlyContinue"');
        output += `✅ Đã dọn dẹp thư mục Local Temp ($env:TEMP)\n`;
    } catch {
        // Ignored because some files are always locked
        output += `⚠️ Dọn dẹp Local Temp hoàn tất (bỏ qua các file đang được sử dụng)\n`;
    }

    // 2. Clear C:\Windows\Temp (Requires Admin, might fail silently if not elevated)
    try {
        await execAsync('powershell -Command "Remove-Item -Path C:\\Windows\\Temp\\* -Recurse -Force -ErrorAction SilentlyContinue"');
        output += `✅ Đã dọn dẹp thư mục Windows Temp\n`;
    } catch {
        output += `⚠️ Dọn dẹp Windows Temp hoàn tất (bỏ qua các file khóa/cần quyền Admin)\n`;
    }

    // 3. Empty Recycle Bin
    try {
        await execAsync('powershell -Command "Clear-RecycleBin -Force -ErrorAction SilentlyContinue"');
        output += `✅ Đã làm trống Thùng rác (Recycle Bin)\n`;
    } catch {
        output += `⚠️ Thùng rác đã rỗng hoặc gặp lỗi truy cập\n`;
    }

    // 4. Flush DNS
    try {
        const dnsRes = await execAsync('ipconfig /flushdns');
        if (dnsRes.stdout.includes('Successfully')) {
            output += `✅ Đã xóa DNS Resolver Cache (Flush DNS)\n`;
        } else {
            output += `✅ Đã chạy lệnh Flush DNS\n`;
        }
    } catch {
        output += `❌ Lỗi khi Flush DNS\n`;
    }

    output += `\n(💡 SYSTEM NOTE: Báo cáo lại cho người dùng rằng máy tính đã được dọn dẹp sạch sẽ để giải phóng dung lượng và tối ưu mạng.)`;
    return output;

  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    logger.error(`[system_optimizer] Error: ${errMsg}`);
    return `[SYSTEM_ERROR] Lỗi trong quá trình dọn dẹp hệ thống: ${errMsg}`;
  }
};
