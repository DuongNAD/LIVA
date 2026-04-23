import * as chokidar from "chokidar";
import * as path from "path";
import * as os from "os";
import * as fs from "fs";
import { logger } from "../utils/logger";
import { MemoryManager } from "../MemoryManager";

export class AppWatcherService {
  private watcher: chokidar.FSWatcher | null = null;
  private skillMapper: Record<string, any> = {};
  private memoryManager: MemoryManager;
  private onAppDiscoveredCallback: ((appName: string, skillData: any) => void) | null = null;

  constructor(memoryManager: MemoryManager) {
    this.memoryManager = memoryManager;
    this.loadSkillMapper();
  }

  public setCallback(callback: (appName: string, skillData: any) => void) {
    this.onAppDiscoveredCallback = callback;
  }

  private loadSkillMapper() {
    try {
      // Vì đang dùng ES Modules/tsx, __dirname có thể không hoạt động. Dùng process.cwd()
      const mapperPath = path.join(process.cwd(), "src", "services", "SkillMapper.json");
      if (fs.existsSync(mapperPath)) {
        const data = fs.readFileSync(mapperPath, "utf-8");
        this.skillMapper = JSON.parse(data);
        logger.info(`[AppWatcher] Đã nạp SkillMapper với ${Object.keys(this.skillMapper).length} ứng dụng whitelisted.`);
      } else {
        logger.warn("[AppWatcher] Không tìm thấy SkillMapper.json");
      }
    } catch (e: any) {
      logger.error(`[AppWatcher] Lỗi khi nạp SkillMapper: ${e.message}`);
    }
  }

  public start() {
    // Các đường dẫn phổ biến chứa shortcut (.lnk) trên Windows
    const watchPaths = [
      path.join(os.homedir(), "Desktop"),
      `C:\\Users\\Public\\Desktop`,
      path.join(os.homedir(), "AppData", "Roaming", "Microsoft", "Windows", "Start Menu", "Programs"),
      `C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs`
    ].filter(p => fs.existsSync(p));

    logger.info(`[AppWatcher] Đang theo dõi (0% CPU Event-Driven) tại ${watchPaths.length} thư mục shortcut...`);

    // Khởi tạo chokidar với ignoreInitial: true để không báo cáo lại các app cũ lúc mới chạy
    this.watcher = chokidar.watch(watchPaths, {
      ignored: /(^|[\/\\])\../, // ignore hidden files
      persistent: true,
      ignoreInitial: true,
      depth: 2
    });

    this.watcher
      .on("add", (filePath) => this.handleFileEvent(filePath, "add"))
      .on("unlink", (filePath) => this.handleFileEvent(filePath, "unlink"))
      .on("error", (error) => logger.error(`[AppWatcher] Lỗi theo dõi: ${error}`));
  }

  public stop() {
    if (this.watcher) {
      this.watcher.close();
      logger.info("[AppWatcher] Đã dừng theo dõi shortcut.");
    }
  }

  private handleFileEvent(filePath: string, eventType: "add" | "unlink") {
    // Chỉ quan tâm file .lnk
    if (!filePath.toLowerCase().endsWith(".lnk")) return;

    // Trích xuất tên ứng dụng (Ví dụ: "Spotify.lnk" -> "Spotify")
    const fileName = path.basename(filePath);
    const appName = fileName.replace(/\.lnk$/i, "").trim();

    // Đối chiếu với SkillMapper
    const matchedSkill = this.findMatchingSkill(appName);

    if (matchedSkill && eventType === "add") {
      logger.info(`[AppWatcher] Phát hiện cài đặt mới hợp lệ: ${appName} -> Map với kỹ năng [${matchedSkill.type}]`);
      this.notifyLivaNewApp(appName, matchedSkill);
    } else if (matchedSkill && eventType === "unlink") {
      logger.info(`[AppWatcher] User đã gỡ/xoá shortcut ${appName}. Có thể thu hồi kỹ năng tương ứng.`);
      // Tích hợp revoke skill sau
    }
  }

  private findMatchingSkill(appName: string): any | null {
    // Tìm kiếm tương đối (Ví dụ: "Spotify" match "Spotify")
    const lowerApp = appName.toLowerCase();
    for (const [key, value] of Object.entries(this.skillMapper)) {
      if (lowerApp.includes(key.toLowerCase())) {
        return { name: key, ...value };
      }
    }
    return null;
  }

  private notifyLivaNewApp(appName: string, skillData: any) {
    logger.info(`[AppWatcher] Gửi Cognitive Event cho LIVA: Ứng dụng ${appName} đã được cài đặt!`);
    
    // Gửi event ẩn vào bộ nhớ của LLM
    const eventContext = `[System Event]: Hệ điều hành phát hiện người dùng vừa cài đặt ứng dụng '${appName}'. Bạn hiện đã được cấp quyền truy cập công cụ '${skillData.type}' (${skillData.description || "Điều khiển ứng dụng"}). Hãy RẤT HÀO HỨNG thông báo điều này cho người dùng và tự động đề xuất 1 hành động liên quan đến ứng dụng này ngay lập tức.`;
    
    // Ép vào bộ nhớ ngắn hạn của LLM
    this.memoryManager.addMessage("system", eventContext);

    // Kích hoạt callback để Gateway ép LIVA generate câu trả lời proactively
    if (this.onAppDiscoveredCallback) {
      this.onAppDiscoveredCallback(appName, skillData);
    }
  }
}
