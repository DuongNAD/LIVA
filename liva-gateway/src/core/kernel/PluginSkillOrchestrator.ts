import type { SkillRegistry } from "../../SkillRegistry";
import type { UIController } from "../UIController";
import { logger } from "../../utils/logger";
import * as path from "node:path";
import * as fs from "node:fs";
import * as chokidar from "chokidar";

export class PluginSkillOrchestrator {
  private fileWatcher: chokidar.FSWatcher | null = null;

  constructor(
    private registry: SkillRegistry,
    private ui: UIController
  ) {}

  public startWatcher(): void {
    const skillsDir = path.join(process.cwd(), "src", "skills");
    if (!fs.existsSync(skillsDir)) return;

    const debounces = new Map<string, { timer: NodeJS.Timeout; event: "add" | "change" | "unlink" }>();

    this.fileWatcher = chokidar.watch(skillsDir, {
      ignored: (filePath: string) => {
        const filename = path.basename(filePath);
        if (filename.startsWith(".")) return true;
        if (
          ["SkillMetadata.ts", "SkillMetadata.js", "index.ts", "index.js", "BaseSkill.ts", "BaseSkill.js"].includes(
            filename
          )
        )
          return true;
        if (filename.includes(".test.")) return true;
        try {
          const stat = fs.statSync(filePath);
          if (stat.isFile()) {
            return !filePath.endsWith(".ts") && !filePath.endsWith(".js");
          }
        } catch {
          return true;
        }
        return false;
      },
      persistent: true,
      ignoreInitial: true,
    });

    this.fileWatcher.on("all", (event, filePath) => {
      if (event !== "add" && event !== "change" && event !== "unlink") return;

      const existing = debounces.get(filePath);
      if (existing) {
        clearTimeout(existing.timer);
      }

      const timer = setTimeout(() => {
        debounces.delete(filePath);
        logger.warn(`🔥 [DNA Hot-Swap] File mutation detected (${event}): ${filePath}`);
        this.registry.reloadLocalSkill(filePath, event).catch((e: Error) => logger.error(e, "Lỗi reloadLocalSkill:"));
      }, 1000);

      debounces.set(filePath, { timer, event });
    });
  }

  public async stopWatcher(): Promise<void> {
    if (this.fileWatcher) {
      try {
        await this.fileWatcher.close();
        logger.info("[CoreKernel] 🧹 FileWatcher đã được đóng an toàn.");
      } catch (e) {
        logger.error(`Error closing file watcher: ${e instanceof Error ? e.message : String(e)}`);
      }
      this.fileWatcher = null;
    }
  }

  public async performDiagnostic(
    skillName: string
  ): Promise<{ success: boolean; message: string; details: string }> {
    let testResult = {
      success: true,
      message: "Kĩ năng hoạt động tốt. Hệ thống kiểm định phản hồi thành công.",
      details: "",
    };

    try {
      const skill = this.registry.getAllSkills().find((s) => s.name === skillName);
      if (!skill) {
        throw new Error(`Không tìm thấy kĩ năng ${skillName} trong danh sách đăng ký.`);
      }

      // 1. Kiểm tra trạng thái Whitelist (nếu bị tắt bởi người dùng)
      const whitelistData = this.registry.whitelist.getAll();
      const wlEntry = whitelistData[skillName];
      const isEnabled = wlEntry ? wlEntry.enabled : true;

      if (!isEnabled) {
        return {
          success: false,
          message: "Kĩ năng hiện đang bị TẮT trong phần quản lý.",
          details: "Vui lòng BẬT kĩ năng trước khi kiểm tra.",
        };
      }

      // 2. Kiểm tra các biến môi trường đặc thù của từng kĩ năng quan trọng
      if (skillName === "read_emails" || skillName === "read_email_detail" || skillName === "send_email") {
        const host = process.env.EMAIL_IMAP_HOST || process.env.EMAIL_HOST;
        const user = process.env.EMAIL_IMAP_USER || process.env.EMAIL_USER;
        const pass = process.env.EMAIL_PASS;
        if (!host || !user || !pass) {
          throw new Error(
            "Thiếu cấu hình tài khoản Email (EMAIL_HOST / EMAIL_USER / EMAIL_PASS) trong két sắt liva_vault.json hoặc tệp .env!"
          );
        }
      }
      if (skillName === "obsidian_operator") {
        if (!process.env.OBSIDIAN_VAULT_PATH) {
          throw new Error("Thiếu cấu hình đường dẫn Obsidian vault (OBSIDIAN_VAULT_PATH) trong .env!");
        }
      }

      // 3. Thực hiện kiểm định nạp và executor liên kết
      try {
        if (skillName === "get_current_time") {
          // get_current_time là kĩ năng nội bộ đơn giản và không có side effects, thực thi trực tiếp để lấy phản hồi
          const res = await this.registry.executeSkill(skillName, {});
          testResult.details = `Phản hồi thực tế: ${res}`;
        } else {
          // Bỏ qua thực thi thực tế (executeSkill) cho các kĩ năng khác để tránh side effects (ví dụ: tạo file, gọi API, scraping...) và tránh timeout 4 giây gây chậm trễ.
          // Chỉ kiểm tra sự tồn tại của executor hoặc liên kết server mcp.
          if (typeof skill.execute !== "function" && !(skill as { _serverId?: string })._serverId) {
            throw new Error("Kĩ năng thiếu hàm execute hoặc chưa được đăng ký trên máy chủ MCP.");
          }
          testResult.message = "Kĩ năng đã được nạp thành công và sẵn sàng hoạt động.";
          testResult.details =
            "Trạng thái: Sẵn sàng thực thi (Đã bỏ qua chạy thử để bảo vệ hệ thống và tăng tốc kiểm tra).";
        }
      } catch (execErr: unknown) {
        const errStr = execErr instanceof Error ? execErr.message : String(execErr);
        if (
          errStr.includes("invalid") ||
          errStr.includes("required") ||
          errStr.includes("validation") ||
          errStr.includes("must not be empty") ||
          errStr.includes("ZodError")
        ) {
          testResult.message = "Kĩ năng nạp thành công và trình xác thực tham số hoạt động tốt.";
          testResult.details = `Trạng thái: Sẵn sàng nhận tham số. (Chi tiết kiểm thử: ${errStr})`;
        } else {
          throw execErr;
        }
      }

      // Reset circuit breaker về CLOSED nếu kiểm định thành công
      this.registry.circuitBreaker.recordSuccess(skillName);
    } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message : String(err);
      logger.error(`[UI/Diagnostic] Kiểm tra kĩ năng '${skillName}' thất bại: ${errMsg}`);

      // Kích hoạt circuit breaker để đổi đèn đỏ báo lỗi trên UI
      this.registry.circuitBreaker.recordFailure(skillName, errMsg);

      testResult = {
        success: false,
        message: `Kĩ năng bị lỗi hoặc cấu hình chưa đúng!`,
        details: errMsg,
      };
    }

    return testResult;
  }
}
