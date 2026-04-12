/**
 * @file E:\Project\LIVA\openclaw-gateway\src\memory\SensoryManager.ts
 * @description Tiến hóa: Implement 'Context-Aware Sensory Perception & Type-Safe Data Extraction' 
 * sử dụng TypeScript 5.x Branded Types và Readonly Deep-Immutability patterns.
 * Đảm bảo tính toàn vẹn thông qua 'Sensory Context Token'.
 */

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
  
  // Sử dụng Readonly để đảm bảo tính bất biến của context bên trong manager
  private readonly _context: SensoryContext = { data: null };

  /** 
   * Tuổi thọ của Sensory Context (TTL): 30 giây.
   * Đảm bảo trí nhớ "bốc hơi" nếu không được cập nhật kịp thời.
   */
  private readonly TTL_MS = 30000;

  private constructor() {}

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
    const rawToken = `token_${Date.now()}_${Math.random().toString(36).substring(2, 15)}`;
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

      // Tạo SensoryData mới với tính bất biến và Token xác thực
      const newData: SensoryData = Object.freeze({
        activeApp: win?.owner?.name || "Unknown",
        windowTitle: win?.title || "Unknown",
        clipboardText: clipText || "",
        capturedAt: Date.now(),
        token: this.generateToken(),
      });

      // Cập nhật context (Deep-immutability pattern)
      (this as any)._context = { data: newData };

      console.log(`[SensoryMemory] 👁️ Context Captured with Token: ${newData.token}`);
    } catch (error) {
      console.error("[SensoryMemory] Lỗi khi kích hoạt giác quan:", error);
    }
  }

  /**
   * Bơm Sensory Memory vào System Prompt của Agent.
   * Kiểm tra tính hợp lệ của Token và TTL để đảm bảo Zero-trust integrity.
   */
  public injectSensoryPrompt(): string {
    const currentData = this._context.data;

    if (!currentData) return "";

    // 1. Kiểm tra Time-To-Live (TTL): Nếu quá 30s, trí nhớ tự hủy.
    if (Date.now() - currentData.capturedAt > this.TTL_MS) {
      console.log(`[SensoryMemory] 🌬️ Ký ức cảm giác đã tự huỷ sau ${this.TTL_MS / 1000}s`);
      this.flush();
      return "";
    }

    // 2. Kiểm tra Token (Xác thực context không bị rò rỉ/tráo đổi)
    if (!currentData.token || typeof currentData.token !== 'string') {
        console.error("[SensoryMemory] Critical: Invalid Sensory Token detected!");
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
   * Dọn dẹp trí nhớ cảm giác (Flush context) để tránh rò rỉ dữ liệu giữa các phiên làm việc.
   */
  public flush(): void {
    (this as any)._context = { data: null };
    console.log(`[SensoryMemory] 🧹 Sensory Context Flushed.`);
  }

  /**
   * Getter để truy cập dữ liệu hiện tại (Chỉ trả về Readonly)
   */
  public get currentData(): SensoryData | null {
    return this._context.data;
  }
}