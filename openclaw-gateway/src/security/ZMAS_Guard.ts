import { logger } from "../utils/logger";
import { RPAGuardrails } from "./RPAGuardrails";
import type { ISecurityGuard } from "../types/Contracts";

/**
 * Z-MAS Guard (Zero-Trust Multi-Layer Architecture Security)
 * ===========================================================
 * Lớp rào chắn độc lập chuyên đánh giá kết quả từ các Tool.
 * 
 * V3 Upgrades:
 *   - Chuyển từ static sang instance methods (implements ISecurityGuard)
 *   - Cho phép mock injection trong Unit Tests
 *   - Layer 1: URL Whitelist Filtering (giữ nguyên logic cũ)
 *   - Layer 2: PII Detection via RPAGuardrails
 *   - Layer 3: Credential Leak Prevention
 *   - Layer 4: Prompt Injection Guard
 */
export class ZMAS_Guard implements ISecurityGuard {
  // Danh sách các Domain an toàn tuyệt đối (Được phép giữ lại Link)
  private readonly WHITELISTED_DOMAINS = [
    "google.com",
    "youtube.com",
    "github.com",
    "liva.ai",
    "facebook.com",
    "messenger.com",
    "zalo.me",
    "stackoverflow.com",
    "npmjs.com",
    "wikipedia.org"
  ];

  // Regex để vét bắt URL trong chuỗi thô
  private readonly URL_REGEX = /(https?:\/\/[^\s]+)/g;

  // Tools cục bộ không cần quét (tối ưu tốc độ)
  private readonly LOCAL_ONLY_TOOLS = [
    "get_current_time", "get_system_info", "read_local_file",
    "write_local_file", "list_directory", "update_core_profile"
  ];

  /**
   * Phương thức tự động khắc phục đa tầng (Multi-Layer Auto-Remediation)
   * Kiểm tra chuỗi đầu ra của Tool qua 4 lớp bảo vệ.
   */
  public executeAutoRemediation(toolOutput: string, sourceToolName: string): string {
    if (!toolOutput) return toolOutput;

    // Nếu là công cụ gọi cục bộ rõ ràng thì bỏ qua để tối ưu tốc độ
    if (this.LOCAL_ONLY_TOOLS.includes(sourceToolName)) return toolOutput;

    let sanitizedOutput = toolOutput;
    let totalAnomalies = 0;
    const alerts: string[] = [];

    // ==========================================
    // LAYER 1: URL Whitelist Filtering (Legacy)
    // ==========================================
    const detectedUrls = toolOutput.match(this.URL_REGEX);
    if (detectedUrls) {
      for (const urlStr of detectedUrls) {
        try {
          const urlObj = new URL(urlStr);
          const host = urlObj.hostname.toLowerCase();

          const isSafe = this.WHITELISTED_DOMAINS.some(allowedDomain => 
              host === allowedDomain || host.endsWith(`.${allowedDomain}`)
          );

          if (!isSafe) {
            totalAnomalies++;
            sanitizedOutput = sanitizedOutput.replace(
              urlStr, 
              `[Z-MAS BẢO VỆ: ĐÃ KHÓA URL KHÔNG XÁC ĐỊNH (${host})]`
            );
            alerts.push(`URL lạ: ${host}`);
          }
        } catch {
          totalAnomalies++;
          sanitizedOutput = sanitizedOutput.replace(
              urlStr, 
              `[Z-MAS BẢO VỆ: ĐÃ KHÓA URL DỊ DẠNG]`
          );
        }
      }
    }

    // ==========================================
    // LAYER 2: PII Detection
    // ==========================================
    const piiResult = RPAGuardrails.scanForPII(sanitizedOutput);
    if (piiResult.hasPII) {
      totalAnomalies += piiResult.detectedTypes.length;
      sanitizedOutput = piiResult.redactedText;
      alerts.push(`PII: ${piiResult.detectedTypes.join(", ")}`);
      logger.warn(`🛡️ [ZMAS_Guard/PII] Phát hiện thông tin nhạy cảm từ ${sourceToolName}: ${piiResult.detectedTypes.join(", ")}`);
    }

    // ==========================================
    // LAYER 3: Credential Leak Prevention
    // ==========================================
    const credResult = RPAGuardrails.scanForCredentials(sanitizedOutput);
    if (credResult.hasCredentials) {
      totalAnomalies += credResult.types.length;
      alerts.push(`Credentials: ${credResult.types.join(", ")}`);
      logger.warn(`🔐 [ZMAS_Guard/Cred] Phát hiện rò rỉ xác thực từ ${sourceToolName}: ${credResult.types.join(", ")}`);
      // Redact the entire output if credentials found
      sanitizedOutput = `[Z-MAS BẢO VỆ: ĐÃ ẨN NỘI DUNG CHỨA THÔNG TIN XÁC THỰC (${credResult.types.join(", ")})]`;
    }

    // ==========================================
    // LAYER 4: Prompt Injection Guard
    // ==========================================
    const injResult = RPAGuardrails.detectPromptInjection(sanitizedOutput);
    if (injResult.isInjection) {
      totalAnomalies++;
      alerts.push(`Prompt Injection detected`);
      logger.warn(`⚠️ [ZMAS_Guard/Injection] Phát hiện mẫu tấn công injection từ ${sourceToolName}`);
      sanitizedOutput = `[Z-MAS BẢO VỆ: ĐÃ VÔ HIỆU HÓA NỘI DUNG CHỨA MẪU TẤN CÔNG]\n` + 
        sanitizedOutput.replaceAll(/IGNORE\s+(ALL\s+)?PREVIOUS\s+INSTRUCTIONS/gi, "[BLOCKED]")
                       .replaceAll(/IGNORE\s+(ALL\s+)?ABOVE/gi, "[BLOCKED]")
                       .replaceAll(/<\s*system\s*>/gi, "[BLOCKED]");
    }

    // ==========================================
    // Tổng kết cảnh báo
    // ==========================================
    if (totalAnomalies > 0) {
      logger.warn(`🛡️ [ZMAS_Guard] Tổng cộng ${totalAnomalies} mối đe dọa từ ${sourceToolName}: ${alerts.join(" | ")}`);
      sanitizedOutput = `\n[CẢNH BÁO AN NINH Z-MAS]: Phát hiện ${totalAnomalies} mối đe dọa (${alerts.join(", ")}). Hệ thống đã tự động xử lý.\n` + sanitizedOutput;
    }

    return sanitizedOutput;
  }
}
