import { logger } from "../utils/logger";
import { RPAGuardrails } from "./RPAGuardrails";
import type { ISecurityGuard } from "../types/Contracts";

/**
 * Z-MAS Guard (Zero-Trust Multi-Layer Architecture Security)
 * ===========================================================
 * Multi-layer output filter and security gate for all AI tool executions.
 * 
 * V3 Instance-Based Architecture:
 *   - Layer 1: URL Whitelist Filtering
 *   - Layer 2: PII Detection via RPAGuardrails
 *   - Layer 3: Credential Leak Prevention
 *   - Layer 4: Prompt Injection Guard
 *   - Layer 5: Shell Command Allowlist (restored from v2 static)
 *   - Layer 6: Skill Risk Classification (restored from v2 static)
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

  // ===========================
  // Layer 5: Shell Command Allowlist
  // ===========================

  /** Read-only safe commands — no approval needed */
  private readonly SAFE_COMMANDS: RegExp[] = [
    /^dir\b/i,
    /^ls\b/i,
    /^echo\b/i,
    /^hostname\b/i,
    /^ipconfig\b/i,
    /^ifconfig\b/i,
    /^whoami\b/i,
    /^ping\b/i,
    /^date\b/i,
    /^time\b/i,
    /^cat\b/i,
    /^type\b/i,
    /^head\b/i,
    /^tail\b/i,
    /^wc\b/i,
    /^pwd\b/i,
    /^uname\b/i,
    /^git\s+(status|log|diff|branch|show|remote|tag)\b/i,
    /^npm\s+(list|ls|view|info|outdated|pack)\b/i,
    /^npx\s+tsc\b/i,
    /^npx\s+vitest\b/i,
    /^node\s+--version\b/i,
    /^npm\s+--version\b/i,
  ];

  /** Destructive commands — HARD BLOCK, no approval possible */
  private readonly DESTRUCTIVE_PATTERNS: RegExp[] = [
    /\brm\s+-rf\b/i,
    /\bRemove-Item\b/i,
    /\brmdir\s+\/s/i,
    /\bformat\s+[a-zA-Z]:/i,
    /\bshutdown\b/i,
    /\brestart\b/i,
    /\bdel\s+\/[fqs]/i,
    /\bInvoke-Expression\b/i,
    /\bInvoke-WebRequest\b/i,
    /\bStart-Process\b/i,
    /\biex\b/i,
    /\breg\s+(delete|add)\b/i,
    /\bnet\s+(user|localgroup)\b/i,
    /\bschtasks\b/i,
    /\bsc\s+(delete|create|config)\b/i,
    /\btaskkill\b/i,
    /\bkill\s+-9\b/i,
    /\bchmod\s+777\b/i,
    /\bchown\b/i,
    /\bdd\s+if=/i,
    /\bmkfs\b/i,
  ];

  /** Network exfiltration patterns — HARD BLOCK */
  private readonly EXFIL_PATTERNS: RegExp[] = [
    /\bwget\b/i,
    /\bcurl\s+.*-[dX]\b/i,
    /\bscp\b/i,
    /\bsftp\b/i,
    /\bftp\b/i,
    /\bnc\s+-/i,
    /\bnetcat\b/i,
    /\bpowershell\s+.*Invoke-/i,
  ];

  // ===========================
  // Layer 6: Skill Risk Classification
  // ===========================

  private readonly SKILL_RISK_MAP: Record<string, "LOW" | "MEDIUM" | "HIGH" | "CRITICAL"> = {
    // LOW — Read-only, no side effects
    get_current_time: "LOW",
    get_system_info: "LOW",
    read_local_file: "LOW",
    list_directory: "LOW",
    get_weather: "LOW",
    translate_text: "LOW",
    hash_checksum: "LOW",
    json_yaml_converter: "LOW",
    summarize_content: "LOW",
    qr_code_tool: "LOW",
    screenshot_capture: "LOW",
    network_diagnostics: "LOW",

    // MEDIUM — Data access, potential privacy impact
    read_emails: "MEDIUM",
    read_email_detail: "MEDIUM",
    web_search: "MEDIUM",
    browser_automation: "MEDIUM",
    image_manipulator: "MEDIUM",
    pdf_generator: "MEDIUM",
    manage_memory: "MEDIUM",
    youtube_downloader: "MEDIUM",
    process_manager: "MEDIUM",

    // HIGH — Write operations, external communication
    write_local_file: "HIGH",
    delete_local_file: "HIGH",
    send_zalo_bot: "HIGH",
    send_zalo_rpa: "HIGH",
    send_messenger_rpa: "HIGH",
    send_telegram: "HIGH",
    send_email: "HIGH",
    append_google_doc: "HIGH",
    create_google_doc: "HIGH",
    auto_backup: "HIGH",
    expense_tracker: "HIGH",
    code_runner: "HIGH",

    // CRITICAL — System-level, self-modification, unrestricted execution
    execute_command: "CRITICAL",
    liva_ai_scientist: "CRITICAL",
    research_ideation: "CRITICAL",
    gitnexus_query: "CRITICAL",
  };

  /**
   * Layer 1-4: Multi-Layer Auto-Remediation
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
    const isWebTool = ["web_search", "gemini_surfer", "web_browser", "summarize_content", "youtube_downloader", "read_emails", "read_email_detail"].includes(sourceToolName);
    if (!isWebTool) {
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
                `[Z-MAS GUARD: BLOCKED UNKNOWN URL (${host})]`
              );
              alerts.push(`Unknown URL: ${host}`);
            }
          } catch {
            totalAnomalies++;
            sanitizedOutput = sanitizedOutput.replace(
                urlStr, 
                `[Z-MAS GUARD: BLOCKED MALFORMED URL]`
            );
          }
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
      logger.warn(`🛡️ [ZMAS_Guard/PII] Detected sensitive PII from ${sourceToolName}: ${piiResult.detectedTypes.join(", ")}`);
    }

    // ==========================================
    // LAYER 3: Credential Leak Prevention
    // ==========================================
    const credResult = RPAGuardrails.scanForCredentials(sanitizedOutput);
    if (credResult.hasCredentials) {
      totalAnomalies += credResult.types.length;
      alerts.push(`Credentials: ${credResult.types.join(", ")}`);
      logger.warn(`🔐 [ZMAS_Guard/Cred] Detected credential leak from ${sourceToolName}: ${credResult.types.join(", ")}`);
      // Redact the entire output if credentials found
      sanitizedOutput = `[Z-MAS GUARD: REDACTED CONTENT CONTAINING CREDENTIALS (${credResult.types.join(", ")})]`;
    }

    // ==========================================
    // LAYER 4: Prompt Injection Guard
    // ==========================================
    const injResult = RPAGuardrails.detectPromptInjection(sanitizedOutput);
    if (injResult.isInjection) {
      totalAnomalies++;
      alerts.push(`Prompt Injection detected`);
      logger.warn(`⚠️ [ZMAS_Guard/Injection] Detected prompt injection attack pattern from ${sourceToolName}`);
      sanitizedOutput = `[Z-MAS GUARD: DISABLED CONTENT CONTAINING ATTACK PATTERN]\n` + 
        sanitizedOutput.replaceAll(/IGNORE\s+(ALL\s+)?PREVIOUS\s+INSTRUCTIONS/gi, "[BLOCKED]")
                       .replaceAll(/IGNORE\s+(ALL\s+)?ABOVE/gi, "[BLOCKED]")
                       .replaceAll(/<\s*system\s*>/gi, "[BLOCKED]");
    }

    // ==========================================
    // Tổng kết cảnh báo
    // ==========================================
    if (totalAnomalies > 0) {
      logger.warn(`🛡️ [ZMAS_Guard] Total ${totalAnomalies} threats from ${sourceToolName}: ${alerts.join(" | ")}`);
      sanitizedOutput = `\n[Z-MAS SECURITY ALERT]: Detected ${totalAnomalies} threats (${alerts.join(", ")}). System auto-remediated.\n` + sanitizedOutput;
    }

    return sanitizedOutput;
  }

  // ===========================
  // Layer 5: Shell Command Validation
  // ===========================

  /**
   * Validate a shell command against the security allowlist.
   * Returns { allowed, requiresApproval } — three outcomes:
   *   1. allowed=true, requiresApproval=false → Safe read-only command
   *   2. allowed=false, requiresApproval=false → HARD BLOCK (destructive/exfil)
   *   3. allowed=false, requiresApproval=true → Unknown command, needs HITL approval
   */
  public validateShellCommand(command: string): { allowed: boolean; requiresApproval: boolean; reason?: string } {
    if (!command || !command.trim()) {
      return { allowed: false, requiresApproval: false, reason: "Empty command" };
    }

    const trimmed = command.trim();

    // HARD BLOCK: Destructive commands (no approval possible)
    for (const pattern of this.DESTRUCTIVE_PATTERNS) {
      if (pattern.test(trimmed)) {
        logger.warn(`🛡️ [ZMAS_Guard/Shell] HARD BLOCKED destructive command: "${trimmed}"`);
        return { allowed: false, requiresApproval: false, reason: `Destructive command blocked: ${pattern.source}` };
      }
    }

    // HARD BLOCK: Network exfiltration
    for (const pattern of this.EXFIL_PATTERNS) {
      if (pattern.test(trimmed)) {
        logger.warn(`🛡️ [ZMAS_Guard/Shell] HARD BLOCKED exfiltration attempt: "${trimmed}"`);
        return { allowed: false, requiresApproval: false, reason: `Network exfiltration blocked: ${pattern.source}` };
      }
    }

    // ALLOW: Safe read-only commands
    for (const pattern of this.SAFE_COMMANDS) {
      if (pattern.test(trimmed)) {
        return { allowed: true, requiresApproval: false };
      }
    }

    // UNKNOWN: Requires human approval (HITL)
    logger.info(`[ZMAS_Guard/Shell] Unknown command requires approval: "${trimmed}"`);
    return { allowed: false, requiresApproval: true, reason: "Unknown command — requires human approval" };
  }

  // ===========================
  // Layer 6: Skill Risk Classification
  // ===========================

  /**
   * Get the risk level of a registered skill.
   * Returns "UNKNOWN" for unregistered skills (treated as HIGH risk).
   */
  public getSkillRiskLevel(skillName: string): "LOW" | "MEDIUM" | "HIGH" | "CRITICAL" | "UNKNOWN" {
    return this.SKILL_RISK_MAP[skillName] ?? "UNKNOWN";
  }

  /**
   * Determine whether a skill requires HITL approval before execution.
   * CRITICAL and UNKNOWN skills always require approval.
   * HIGH skills require approval only for specific dangerous actions.
   */
  public shouldRequireApproval(skillName: string): boolean {
    const risk = this.getSkillRiskLevel(skillName);
    return risk === "CRITICAL" || risk === "UNKNOWN";
  }
}
