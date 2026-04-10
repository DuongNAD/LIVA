import { exec } from "child_process";
import { promisify } from "util";

const execAsync = promisify(exec);

export const metadata = {
  name: "execute_command",
  description:
    "Thực thi một lệnh trên Terminal/Command Prompt của hệ điều hành. Dùng để chạy script, kiểm tra mạng, hoặc khởi chạy các công cụ phân tích.",
  parameters: {
    type: "object",
    properties: {
      command: {
        type: "string",
        description: "Câu lệnh CLI cần thực thi (CLI command to execute).",
      },
    },
    required: ["command"],
  },
};

export const execute = async (args: { command: string }): Promise<string> => {
  try {
    console.log(
      `[Skill: execute_command] Đang phân tích lệnh (Analyzing): ${args.command}`,
    );

    // --- 🛡️ LỚP BẢO MẬT (SECURITY FILTER) 🛡️ ---
    // Danh sách các mẫu lệnh hoặc tiền tố mệnh lệnh đặc biệt nguy hiểm đối với hệ sinh thái Windows
    const DANGEROUS_PATTERNS = [
      /rmdir/i,
      /rd\s+\/s/i,
      /del\s+\/f/i,
      /Remove-Item/i, // Lệnh xóa/phá hủy tệp lớn
      /format\s+[a-z]:/i,
      /diskpart/i,
      /mkfs/i, // Lệnh can thiệp phân vùng ổ đĩa
      /shutdown/i,
      /Stop-Computer/i,
      /Restart-Computer/i, // Lệnh điều khiển nguồn
      /Uninstall/i,
      /msiexec\s+\/[xX]/i,
      /reg\s+delete/i, // Lệnh gỡ phần mềm/phá Registry
      /netsh/i,
      /ipconfig\s+\/(release|renew)/i,
      /route\s+delete/i,
      /Disable-NetAdapter/i, // Lệnh Phá Internet/Mạng
      /net\s+user/i,
      /Set-LocalUser/i,
      /syskey/i, // Lệnh Quản trị Account
      /taskkill\s+\/f/i,
      /Stop-Process/i, // Lệnh giết tiến trình cưỡng bức
      /Set-ExecutionPolicy/i,
      /Clear-EventLog/i, // Lệnh can thiệp lõi bảo mật
    ];

    for (const pattern of DANGEROUS_PATTERNS) {
      if (pattern.test(args.command)) {
        console.warn(
          `[SECURITY ALERT] Liva phát hiện và chặn lệnh sinh tử: ${args.command}`,
        );
        return `[HỆ THỐNG BẢO MẬT TỪ CHỐI]: Lệnh "${args.command}" chứa rủi ro can thiệp sâu vào hệ thống (Xóa, Format, Gỡ cài đặt). Yêu cầu đã bị hủy bỏ để bảo vệ an toàn cho máy tính. Phế duyệt từ con người là bắt buộc cho loại lệnh này.`;
      }
    }
    // ------------------------------------------

    console.log(
      `[Skill: execute_command] Lệnh an toàn. Đang chạy (Executing)...`,
    );
    const { stdout, stderr } = await execAsync(args.command);

    if (stderr && stderr.trim() !== "") {
      console.warn(
        `[Cảnh báo - Warning] Có thông báo từ luồng lỗi (Standard error stream):\n${stderr}`,
      );
    }

    return `Kết quả thực thi (Execution output):\n${stdout}`;
  } catch (error: any) {
    return `Thực thi thất bại (Execution failed): ${error.message}\nOutput: ${error.stdout || ""}`;
  }
};
