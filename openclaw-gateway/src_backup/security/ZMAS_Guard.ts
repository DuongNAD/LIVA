import { logger } from "../utils/logger";

/**
 * Z-MAS Guard (Zero Trust Architecture)
 * Lớp rào chắn độc lập chuyên đánh giá kết quả từ các Tool (ví dụ: read_emails, web_search).
 * Nếu phát hiện siêu liên kết (Hyperlinks) lạ vượt qua Whitelist, nó sẽ tự động tước bỏ và thay thế bằng cảnh báo đỏ.
 */
export class ZMAS_Guard {
  // Danh sách các Domain an toàn tuyệt đối (Được phép giữ lại Link)
  private static readonly WHITELISTED_DOMAINS = [
    "google.com",
    "youtube.com",
    "github.com",
    "liva.ai",
    "facebook.com"
  ];

  // Regex để vét bắt URL trong chuỗi thô
  private static readonly URL_REGEX = /(https?:\/\/[^\s]+)/g;

  /**
   * Phương thức tự động khắc phục (Auto-Remediation)
   * Kiểm tra chuỗi đầu ra của Tool, nếu dính Link ngoài Whitelist sẽ cắt dán ngay lập tức.
   */
  public static executeAutoRemediation(toolOutput: string, sourceToolName: string): string {
    if (!toolOutput) return toolOutput;

    // Nếu là công cụ gọi cục bộ rõ ràng không sinh URL thì bỏ qua để tối ưu tốc độ
    if (["get_current_time", "get_system_info"].includes(sourceToolName)) return toolOutput;

    const detectedUrls = toolOutput.match(this.URL_REGEX);
    if (!detectedUrls) return toolOutput; // Sạch

    let sanitizedOutput = toolOutput;
    let anomalyCount = 0;

    for (const urlStr of detectedUrls) {
      try {
        const urlObj = new URL(urlStr);
        const host = urlObj.hostname.toLowerCase();

        // Kiểm tra xem Host có nằm trong mảng Whitelist hay không
        const isSafe = this.WHITELISTED_DOMAINS.some(allowedDomain => 
            host === allowedDomain || host.endsWith(`.${allowedDomain}`)
        );

        if (!isSafe) {
          anomalyCount++;
          // AUTO-REMEDIATION: Khóa đường link lại ngay lập tức
          sanitizedOutput = sanitizedOutput.replace(
            urlStr, 
            `[Z-MAS BẢO VỆ: ĐÃ KHÓA URL ĐỘC HẠI (${host})]`
          );
        }
      } catch (e) {
        // Link dị dạng hỏng hóc -> Khóa nốt
        anomalyCount++;
        sanitizedOutput = sanitizedOutput.replace(
            urlStr, 
            `[Z-MAS BẢO VỆ: ĐÃ KHÓA URL DỊ DẠNG]`
        );
      }
    }

    if (anomalyCount > 0) {
      logger.warn(`🛡️ [ZMAS_Guard] Phát hiện & Khắc phục ${anomalyCount} Link lạ từ công cụ ${sourceToolName}`);
      sanitizedOutput = `\n[CẢNH BÁO AN NINH Z-MAS]: Tool này có chứa ${anomalyCount} liên kết không xác định (Có thể là Phishing/Malware). Hệ thống đã tự động vô hiệu hóa các liên kết này.\n` + sanitizedOutput;
    }

    return sanitizedOutput;
  }
}
