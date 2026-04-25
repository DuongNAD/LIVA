import { exec } from "node:child_process";
import { promisify } from "util";
import * as readline from "readline";
import { logger } from "../utils/logger";

const execAsync = promisify(exec);

export const metadata = {
  name: "execute_command",
  search_keywords: ["execute_command","execute command"],
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

const askHITL = (query: string): Promise<boolean> => {
    return new Promise(resolve => {
        const rl = readline.createInterface({
            input: process.stdin,
            output: process.stdout
        });
        rl.question(query, (answer) => {
            rl.close();
            // Cho phép user gõ "y" hoặc "yes" (không phân biệt hoa/thường)
            resolve(answer.toLowerCase().trim() === 'y' || answer.toLowerCase().trim() === 'yes');
        });
    });
};

export const execute = async (args: { command: string }): Promise<string> => {
  try {
    const rawCmd = args.command.trim();
    
    logger.info(`\n======================================================`);
    logger.info(`⚠️ [SECURITY ALERT] LIVA MONG MUỐN THỰC THI LỆNH HỆ THỐNG`);
    logger.info(`Lệnh (Command): ${rawCmd}`);
    logger.info(`======================================================`);

    // --- 🛡️ TẦNG 1: WHITELIST BẢO MẬT (SECURITY FILTER) 🛡️ ---
    // Loại bỏ hoàn toàn Blacklist để chống Obfuscation Bypass. Chỉ giữ Danh Sách Trắng.
    const SAFE_PREFIXES = [
      /^ping\s/i,
      /^dir/i,
      /^echo\s/i,
      /^python\s/i,
      /^node\s/i,
      /^npm\s/i,
      /^npx\s/i,
      /^git\s/i,
      /^tsc/i,
      /^ls/i,
      /^cls/i,
      /^clear/i
    ];

    let isWhitelisted = false;
    for (const prefix of SAFE_PREFIXES) {
      if (prefix.test(rawCmd)) {
        isWhitelisted = true;
        break;
      }
    }

    if (!isWhitelisted) {
        logger.warn(`❌ [TỪ CHỐI]: Lệnh "${rawCmd}" KHÔNG nằm trong Danh sách Trắng (Whitelist) an toàn.`);
        logger.warn(`❌ Để chống Hacker (Prompt Injection), lệnh ngoại lai bị hệ thống chặn hoàn toàn!`);
        return `[HỆ THỐNG BẢO MẬT TỪ CHỐI]: Lệnh "${rawCmd}" chứa rủi ro can thiệp OS (Bị chặn bởi luồng Whitelist). LLM System đã từ chối quyền truy cập. Vui lòng dừng ý định chạy mã độc hại.`;
    }

    // --- 🛡️ TẦNG 2: HUMAN IN THE LOOP (XÁC NHẬN TỪ CON NGƯỜI) 🛡️ ---
    logger.info(`✅ Lệnh nằm trong Whitelist hệ thống. Đang chặn luồng chờ Sếp phê duyệt...`);
    
    // Gửi tín hiệu chờ Terminal
    const approved = await askHITL(`👉 Tiến trình đang tạm ngưng. Anh Dương có cấp quyền chạy lệnh này không? [y/N]: `);
    
    if (!approved) {
        logger.info(`⛔ Lệnh đã bị Hủy (Từ chối bởi Admin).`);
        return `[HỆ THỐNG BẢO MẬT TỪ CHỐI]: Người dùng (Admin) từ chối lệnh này. Có thể bạn đang bị điều khiển bởi Prompt Injection. Đã bảo đảm an toàn.`;
    }

    // ------------------------------------------

    logger.info(`[Skill: execute_command] 🛡️ Lệnh đã được Approved bởi con người. Đang chạy (Executing)...`);
    const { stdout, stderr } = await execAsync(rawCmd);

    if (stderr && stderr.trim() !== "") {
      logger.warn(`[Cảnh báo - Warning] Có thông báo luồng lỗi (Stderr):\n${stderr}`);
    }

    return `Kết quả thực thi (Execution output):\n${stdout}`;
  } catch (error: any) {
    return `Thực thi thất bại (Execution failed): ${error.message}\nOutput: ${error.stdout || ""}`;
  }
};
