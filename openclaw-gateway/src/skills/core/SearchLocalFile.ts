import { exec } from "node:child_process";
import { promisify } from "node:util";
import * as os from "node:os";
import * as path from "node:path";
import { z } from "zod";
import { logger } from "@utils/logger";

const execAsync = promisify(exec);

// Allow only safe filename characters to prevent PowerShell command injection
const SANE_FILENAME_REGEX = /^[a-zA-Z0-9À-ỹà-ỹ.\-_ ]+$/u;

const SearchFileSchema = z.object({
  fileName: z.string().min(1, "fileName is required").max(200),
});

export const metadata = {
  name: "search_local_file",
  search_keywords: ["search_local_file","tìm file","tìm kiếm file","ở đâu","where is","find file","search file","tìm cv","file cv"],
  isCoreSkill: true,
  description:
    "[AUTO_RUN] CÔNG CỤ TÌM KIẾM HỆ THỐNG. LIVA CÓ TOÀN QUYỀN TÌM KIẾM FILE. Dùng công cụ này NGAY LẬP TỨC khi user yêu cầu tìm file (PDF, DOCX, JPG...) ở đâu. Công cụ sẽ tự động quét Downloads, Desktop, Documents, D:, E:.",
  short_desc: "TÌM KIẾM HỆ THỐNG: LIVA có TOÀN QUYỀN quét ổ cứng để tìm file (PDF, CV, v.v)",
  parameters: {
    type: "object",
    properties: {
      fileName: {
        type: "string",
        description: "Từ khóa tên file cần tìm (ví dụ: 'CV', 'baocao.pdf'). Không cần gõ chính xác 100%.",
      }
    },
    required: ["fileName"],
  },
};

export const execute = async (rawArgs: unknown): Promise<string> => {
  const parsed = SearchFileSchema.safeParse(rawArgs);
  if (!parsed.success) {
    return `[ValidationError] Invalid input: ${parsed.error.issues.map(i => i.message).join("; ")}`;
  }
  const rawKeyword = parsed.data.fileName;
  if (!SANE_FILENAME_REGEX.test(rawKeyword)) {
    return `[SecurityError] fileName contains invalid characters. Only letters, numbers, dots, hyphens, underscores, Vietnamese characters, and spaces are allowed.`;
  }
  const keyword = rawKeyword;
  try {
    logger.info(`[Skill: search_local_file] Đang tìm kiếm file chứa từ khóa: "${keyword}"...`);

    const userInfo = os.userInfo();
    const userHome = userInfo.homedir; // C:\Users\Admin

    // Danh sách các thư mục cần quét nhanh
    const quickDirs = [
      path.join(userHome, "Downloads"),
      path.join(userHome, "Documents"),
      path.join(userHome, "Desktop"),
      "D:\\",
      "E:\\"
    ];

    let allResults = "";

    // Sử dụng PowerShell để tìm kiếm đa luồng (song song) hoặc tuần tự
    for (const dir of quickDirs) {
      try {
        // Dùng where.exe hoặc Get-ChildItem. 
        // Get-ChildItem -Filter "*keyword*" -Recurse -File -ErrorAction SilentlyContinue
        // Thêm timeout để tránh treo hệ thống
        const psCommand = `Get-ChildItem -Path "${dir}" -Filter "*${keyword}*" -Recurse -File -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName | Select-Object -First 5`;
        
        // Timeout 15 giây cho mỗi thư mục
        const { stdout } = await execAsync(`powershell -Command "${psCommand}"`, { timeout: 15000 });
        
        if (stdout.trim()) {
            allResults += stdout.trim() + "\n";
        }
      } catch (e: any) {
        // Ignore timeout or permission errors for individual directories
        if (e.killed) {
            logger.warn(`[Skill: search_local_file] Timeout khi quét thư mục: ${dir}`);
        }
      }
    }

    if (!allResults.trim()) {
      return `Em đã tìm qua các thư mục phổ biến (Downloads, Documents, Desktop, D:, E:) nhưng không thấy file nào chứa từ khóa "${keyword}". Anh Dương vui lòng kiểm tra lại tên file hoặc để file ở nơi dễ tìm hơn ạ!`;
    }

    return `Đã tìm thấy các file có chứa từ khóa "${keyword}":\n${allResults.trim()}`;
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `Lỗi khi tìm kiếm file: ${errMsg}`;
  }
};
