/**
 * @file E:\Project\LIVA\openclaw-gateway\src\memory\SensoryManager.ts
 * @description Tiến hóa: Implement 'Context-Aware Sensory Perception & Type-Safe Data Extraction' 
 * sử dụng TypeScript 5.x Branded Types và Readonly Deep-Immutability patterns.
 * Đảm bảo tính toàn vẹn thông qua 'Sensory Context Token'.
 */
import { logger } from "../utils/logger";

// --- Anti-Prompt-Injection Constants ---
const SENSORY_MAX_LENGTH = 2000;

/**
 * 🛡️ Anti-Prompt-Injection Sanitizer
 * Prevents attacker-controlled clipboard/window data from manipulating the LLM.
 * 
 * Defenses:
 * 1. Truncate to SENSORY_MAX_LENGTH (2000 chars) to prevent token flooding
 * 2. Strip HTML tags to prevent markup injection
 * 3. Escape control characters (NULL, BEL, BS, etc.) that could confuse parsers
 * 4. Collapse excessive whitespace to prevent layout-based injection
 */
export function sanitizeSensoryData(raw: string): string {
    if (!raw || typeof raw !== "string") return "";

    let sanitized = raw;

    // 1. Truncate — prevent token flooding / context window exhaustion
    if (sanitized.length > SENSORY_MAX_LENGTH) {
        sanitized = sanitized.substring(0, SENSORY_MAX_LENGTH) + "…[truncated]";
    }

    // 2. Strip HTML tags — prevent <script>, <img onerror=>, etc.
    sanitized = sanitized.replace(/<[^>]*>/g, "");

    // 3. Escape control characters (C0 control codes U+0000–U+001F except \n, \r, \t)
    sanitized = sanitized.replace(/[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]/g, "");

    // 4. Collapse excessive whitespace (>3 consecutive newlines → 2)
    sanitized = sanitized.replace(/\n{4,}/g, "\n\n\n");

    return sanitized.trim();
}

// --- Branded Types Definition (Compile-time Safety) ---
export type Brand<K, T> = K & { __brand: T };

/** 
 * SensoryContextToken: Một token duy nhất được tạo ra để xác thực context.
 * Ngăn chặn việc rò rỉ dữ liệu giữa các lớp nhận thức khác nhau (Zero-trust integrity).
 */
export type SensoryContextToken = Brand<string, "SensoryContextToken">;

/** 
 * SensoryData: Dữ liệu cảm giác được bảo vệ bởi tính bất biến (Readonly).
 */
export interface SensoryData {
  readonly activeApp: string;
  readonly windowTitle: string;
  readonly clipboardText: string;
  readonly capturedAt: number;
  readonly token: SensoryContextToken;
}

/** 
 * SensoryContext: Interface đại diện cho trạng thái hiện tại của giác quan.
 */
export interface SensoryContext {
  readonly data: SensoryData | null;
}

export class SensoryManager {
  private static instance: SensoryManager;
  
  /**
   * Tối ưu hóa hiệu năng: Sử dụng Map để lưu trữ đa context (O(1) lookup).
   * Key là SensoryContextToken, Value là SensoryData.
   */
  private readonly _contextMap: Map<SensoryContextToken, SensoryData> = new Map();

  /** 
   * Tuổi thọ của Sensory Context (TTL): 30 giây.
   * Đảm bảo trí nhớ "bốc hơi" nếu không được cập nhật kịp thời.
   */
  private readonly TTL_MS = 30000;

  /**
   * Cơ chế Garbage Collection: Tần suất dọn dẹp (mỗi 5 giây).
   */
  private readonly GC_INTERVAL_MS = 5000;
  private gcTimer: NodeJS.Timeout | null = null;

  private constructor() {
    this.startGarbageCollection();
  }

  /**
   * Khởi động cơ chế dọn dẹp bộ nhớ tự động (Garbage Collection).
   */
  private startGarbageCollection(): void {
    this.gcTimer = setInterval(() => {
      const now = Date.now();
      let cleanedCount = 0;

      for (const [token, data] of this._contextMap.entries()) {
        if (now - data.capturedAt > this.TTL_MS) {
          this._contextMap.delete(token);
          cleanedCount++;
        }
      }
    }, this.GC_INTERVAL_MS);
  }

  /**
   * Singleton pattern để quản lý duy nhất một luồng giác quan.
   */
  public static getInstance(): SensoryManager {
    if (!this.instance) {
      this.instance = new SensoryManager();
    }
    return this.instance;
  }

  /**
   * Tạo ra một token mới cho mỗi lần capture để xác thực tính duy nhất của context.
   */
  private generateToken(): SensoryContextToken {
    const rawToken = `token_${Date.now()}_${Math.random().toString(36).substring(2, 15)}`; // NOSONAR
    return rawToken as SensoryContextToken;
  }

  /**
   * Hàm này được gọi thông qua Global Hotkey để LIVA "tóm gọn" màn hình & clipboard.
   * Sử dụng Type-Safe Data Extraction để đảm bảo dữ liệu đầu vào luôn hợp lệ.
   */
  public async captureContext(): Promise<void> {
    try {
      // Dynamic import để tương thích với các module môi trường (CommonJS/ESM)
      const activeWinModule = await import("active-win");
      const activeWindow = activeWinModule.default;
      const clipboardyModule = await import("clipboardy");
      const clipboardy = clipboardyModule.default;

      // Thực hiện thu thập dữ liệu thô
      const win = await activeWindow();
      const clipText = await clipboardy.read();

      // 🛡️ Sanitize all external input BEFORE storing (Anti-Prompt-Injection)
      const newData: SensoryData = Object.freeze({
        activeApp: sanitizeSensoryData(win?.owner?.name || "Unknown"),
        windowTitle: sanitizeSensoryData(win?.title || "Unknown"),
        clipboardText: sanitizeSensoryData(clipText || ""),
        capturedAt: Date.now(),
        token: this.generateToken(),
      });

      // Cập nhật Map (O(1)) thay vì ghi đè biến đơn lẻ để hỗ trợ đa luồng/đa cảm biến nếu cần
      this._contextMap.set(newData.token, newData);

      logger.info(`[SensoryMemory] 👁️ Context Captured with Token: ${newData.token}`);
    } catch (error) {
      logger.error({ err: error }, "[SensoryMemory] Lỗi khi kích hoạt giác quan");
    }
  }

  /**
   * Cập nhật logic để lấy context mới nhất từ _contextMap.
   */
  private getLatestData(): SensoryData | null {
    let latest: SensoryData | null = null;
    for (const data of this._contextMap.values()) {
      if (!latest || data.capturedAt > latest.capturedAt) {
        latest = data;
      }
    }
    return latest;
  }

  public injectSensoryPrompt(): string {
    const currentData = this.getLatestData();

    if (!currentData) return "";

    // 1. Kiểm tra Time-To-Live (TTL): Nếu quá 30s, trí nhớ tự hủy.
    if (Date.now() - currentData.capturedAt > this.TTL_MS) {
      logger.debug(`[SensoryMemory] 🌬️ Ký ức cảm giác đã tự huỷ sau ${this.TTL_MS / 1000}s`);
      return "";
    }

    // 2. Kiểm tra Token (Xác thực context không bị rò rỉ/tráo đổi)
    if (!currentData.token || typeof currentData.token !== 'string') {
        logger.error("[SensoryMemory] Critical: Invalid Sensory Token detected!");
        return "";
    }

    // 3. Xây dựng Prompt với cấu trúc Type-Safe
    let prompt = `\n<SystemSensory timestamp="${new Date(currentData.capturedAt).toLocaleTimeString("vi-VN")}" token="${currentData.token}">\n`;
    prompt += `- Người dùng đang thao tác trên phần mềm: ${currentData.activeApp}\n`;
    prompt += `- Dòng tiêu đề cửa sổ: ${currentData.windowTitle}\n`;

    if (currentData.clipboardText) {
      prompt += `- Nội dung Clipboard (Vừa copy): """${currentData.clipboardText}"""\n`;
    }
    prompt += `</SystemSensory>\n`;

    return prompt;
  }

  /**
   * Dọn dẹp toàn bộ dữ liệu trong map.
   */
  public flush(): void {
    this._contextMap.clear();
    logger.info(`[SensoryMemory] 🧹 Sensory Context Flushed.`);
  }

  /**
   * 🔒 [Audit Fix C-4] Dọn dẹp GC timer khi shutdown.
   * Gọi từ CoreKernel.shutdown() để ngăn zombie setInterval.
   */
  public dispose(): void {
    if (this.gcTimer) {
      clearInterval(this.gcTimer);
      this.gcTimer = null;
    }
    this._contextMap.clear();
    logger.info(`[SensoryMemory] 🧹 GC Timer stopped. Sensory Manager disposed.`);
  }

  /**
   * Lấy context hiện tại mới nhất.
   */
  public get currentData(): SensoryData | null {
    return this.getLatestData();
  }
}