import * as fs from "fs/promises";
import * as path from "node:path";
import * as crypto from "node:crypto";
import { QuantizedMemoryStore, CoreKernel } from "./memory/TurboQuantStore";
import { StructuredMemory } from "./memory/StructuredMemory";
import { WorkingBuffer } from "./memory/WorkingBuffer";
import { EmbeddingService } from "./services/EmbeddingService";
import { logger } from "./utils/logger";

const ENCRYPTION_KEY = process.env.ENCRYPTION_KEY || crypto.createHash("sha256").update("LIVA_FALLBACK_SECRET_KEY").digest("base64").substring(0, 32);
const IV_LENGTH = 16;

function encryptData(text: string): string {
  const iv = crypto.randomBytes(IV_LENGTH);
  const cipher = crypto.createCipheriv("aes-256-gcm", Buffer.from(ENCRYPTION_KEY), iv);
  let encrypted = cipher.update(text, "utf8", "hex");
  encrypted += cipher.final("hex");
  const authTag = cipher.getAuthTag().toString("hex");
  return `${iv.toString("hex")}:${authTag}:${encrypted}`;
}

function decryptData(text: string): string {
  try {
    const parts = text.split(":");
    if (parts.length !== 3) return text; // Có thể Sếp đang còn giữ định dạng MD thô
    const iv = Buffer.from(parts[0], "hex");
    const authTag = Buffer.from(parts[1], "hex");
    const encryptedText = parts[2];
    const decipher = crypto.createDecipheriv("aes-256-gcm", Buffer.from(ENCRYPTION_KEY), iv);
    decipher.setAuthTag(authTag);
    let decrypted = decipher.update(encryptedText, "hex", "utf8");
    decrypted += decipher.final("utf8");
    return decrypted;
  } catch (e) {
    return text; // Trả về text nguyên bản nếu lỗi giải mã để tương thích ngược Markdown
  }
}

export interface ChatMessage {
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
}

export class MemoryManager {
  private readonly sessionStatePath: string;
  private readonly longTermMarkdownPath: string; // MEMORY.md
  private readonly memoryDirectory: string;
  private readonly shortTermFilePath: string;
  private readonly longTermFilePath: string; // Vẫn giữ legacy enc file
  private readonly userProfilePath: string;
  private readonly quantStore: QuantizedMemoryStore;
  private readonly structuredMemory: StructuredMemory;
  public readonly workingBuffer: WorkingBuffer;
  private readonly authority: CoreKernel;
  private readonly embeddingService: EmbeddingService;
  private memCache: ChatMessage[] = []; // In-memory Cache

  constructor(agentId: string, embeddingService?: EmbeddingService) {
    this.memoryDirectory = path.join(process.cwd(), "data", "agents", agentId);
    
    // File-First Memory Paths
    this.sessionStatePath = path.join(this.memoryDirectory, "SESSION-STATE.md");
    this.longTermMarkdownPath = path.join(this.memoryDirectory, "MEMORY.md");

    // Legacy Paths
    this.shortTermFilePath = path.join(this.memoryDirectory, "short_term_memory.jsonl");
    this.longTermFilePath = path.join(this.memoryDirectory, "long_term_memory.enc");
    this.userProfilePath = path.join(process.cwd(), "src", "user_profile.json");

    // Khởi tạo bộ nhớ nén siêu nhẹ
    this.authority = new CoreKernel(["system", "user", "assistant"]);
    this.quantStore = new QuantizedMemoryStore(
      this.authority,
      path.join(this.memoryDirectory, "turbo_quant_memory.jsonl"),
    );
    // Structured Memory (KV store bổ trợ RAG)
    this.structuredMemory = new StructuredMemory(agentId);
    // Working Buffer (Quản lý Token & Context Compaction)
    this.workingBuffer = new WorkingBuffer(agentId);
    // Shared Embedding Service (Singleton — replaces @xenova/transformers)
    this.embeddingService = embeddingService ?? EmbeddingService.getInstance();
  }

  public async initialize(): Promise<void> {
    try {
      // Initialize Shared Embedding Service (Singleton — Promise Lock prevents double-load)
      logger.info("[Memory] Đang nạp EmbeddingService singleton (HuggingFace)...");
      await this.embeddingService.ensureReady();
      logger.info("[Memory] EmbeddingService ready.");

      await fs.mkdir(this.memoryDirectory, { recursive: true });

      // Khởi tạo tệp Phôi Ký ức (Mã hóa Encrypted)
      try {
        await fs.access(this.longTermFilePath);
      } catch {
        const initialContent = `# Hồ Sơ Ký Ức Dài Hạn (Long-term Context)\n\n*Hệ thống sẽ định kỳ trích xuất (extract) và ghi chú các sự thật (facts) quan trọng vào đây.*\n\n---\n\n## Thói quen & Sở thích (Habits & Preferences)\n\n## Kiến thức đã học (Learned Knowledge)\n`;
        const securedPayload = encryptData(initialContent);
        const tmpPath = `${this.longTermFilePath}.tmp`;
        await fs.writeFile(tmpPath, securedPayload, "utf-8");
        await fs.rename(tmpPath, this.longTermFilePath);
      }

      // Khởi tạo File-First Memory
      try {
        await fs.access(this.sessionStatePath);
      } catch {
        const sessionTemplate = `# WORKING SESSION STATE\n\n## Intent\n(Mục tiêu cốt lõi của phiên làm việc hiện tại)\n\n## Current Context\n(Ngữ cảnh và tình trạng của các dữ liệu đang xử lý)\n\n## Pending Tasks\n- [ ] Nhiệm vụ 1\n- [ ] Nhiệm vụ 2\n`;
        await fs.writeFile(this.sessionStatePath, sessionTemplate, "utf-8");
      }

      try {
        await fs.access(this.longTermMarkdownPath);
      } catch {
        await fs.writeFile(this.longTermMarkdownPath, "# LIVA LONG-TERM MEMORY\n\n", "utf-8");
      }
      
      // Load cache 1 lần duy nhất từ ổ cứng vào bộ nhớ RAM
      const rawHistory = await fs.readFile(
        path.join(this.memoryDirectory, "turbo_quant_memory.jsonl"),
        "utf-8",
      ).catch(() => "");
      
      const lines = rawHistory.split("\n").filter((line) => line.trim() !== "");
      try {
        this.memCache = lines.map((line) => {
          const parsed = JSON.parse(line);
          return {
            role: parsed.role,
            content: parsed.content,
            timestamp: parsed.timestamp || Date.now(),
          };
        });
      } catch {
        this.memCache = [];
      }
      
    } catch (error) {
      logger.error(`[Memory] Lỗi khởi tạo (Initialization error): ${error}`);
    }
  }

  // [Z-MAS RAM Healer] Dọn dẹp tài nguyên ngầm khi shutdown
  public dispose() {
      this.quantStore.dispose();
      // 🔒 [Audit Fix C-3] Close SQLite connection
      this.structuredMemory.close();
      logger.info("[Memory] Đã giải phóng hoàn toàn các luồng Garbage Collection nền.");
  }

  /** Expose StructuredMemory instance for DI (prevents duplicate instantiation) */
  public getStructuredMemoryInstance(): StructuredMemory {
      return this.structuredMemory;
  }

  // --- FILE-FIRST MEMORY METHODS (WAL Protocol) ---

  public async getSessionState(): Promise<string> {
    try {
      return await fs.readFile(this.sessionStatePath, "utf-8");
    } catch {
      return "";
    }
  }

  public async updateSessionState(content: string): Promise<void> {
    const tmpPath = `${this.sessionStatePath}.tmp`;
    await fs.writeFile(tmpPath, content, "utf-8");
    await fs.rename(tmpPath, this.sessionStatePath);
    logger.info("[Memory] Cập nhật SESSION-STATE.md thành công (WAL protocol).");
  }

  public async getLongTermMarkdown(): Promise<string> {
    try {
      return await fs.readFile(this.longTermMarkdownPath, "utf-8");
    } catch {
      return "";
    }
  }

  public async appendLongTermMarkdown(content: string): Promise<void> {
    await fs.appendFile(this.longTermMarkdownPath, `\n${content}\n`, "utf-8");
    logger.info("[Memory] Đã nối thêm dữ liệu vào MEMORY.md.");
  }

  public async appendDailyLog(content: string): Promise<void> {
    const date = new Date();
    const dateStr = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
    const dailyPath = path.join(this.memoryDirectory, "memory", `${dateStr}.md`);
    
    try {
      await fs.mkdir(path.join(this.memoryDirectory, "memory"), { recursive: true });
      await fs.appendFile(dailyPath, `\n[${date.toISOString()}] ${content}\n`, "utf-8");
    } catch (e) {
      logger.error(`[Memory] Lỗi ghi nhật ký hàng ngày: ${e}`);
    }
  }

  // --- END FILE-FIRST MEMORY METHODS ---

  public async addMessage(
    role: "user" | "assistant" | "system",
    content: string,
  ): Promise<void> {
    // 🔒 [Memory Fix #8] Ghi cache và QuantStore ngay lập tức với dummy vector (không block event loop)
    // Embedding sẽ chạy phiên bản real ở nền trong tick tiếp theo mà không cản tầng WS
    const dummyVector: number[] = Array.from({ length: 256 }, () => Math.random() * 2 - 1);

    // Ghi đệm vào RAM Cache ngay lập tức (không đợi embedding)
    this.memCache.push({ role, content, timestamp: Date.now() });

    // V14: Lưỡi Hái Tử Thần - Chống phình to lõi RAM Zalo
    if (this.memCache.length > 200) {
        this.memCache = this.memCache.slice(-100);
        logger.info(`[Memory GC] Đã chặt bỏ 100 tin nhắn cũ khỏi RAM Cache ngầm bảo vệ Zalo!`);
    }

    // 🔒 [Memory Fix #8] Đẩy embedding ra khỏi hot path bằng setImmediate → không block event loop
    const token = this.authority.mintAuthToken(role) as string;
    await this.quantStore.addMemory(role, content, dummyVector, token);

    // Background embedding via shared EmbeddingService (non-blocking)
    if (this.embeddingService.ready) {
      const bgRole = role;
      const bgToken = token;
      setImmediate(async () => {
        try {
          const realVector = await this.embeddingService.embed(content);
          // [Audit Fix H-2] Ghi đè dummy vector bằng real embedding vào QuantStore
          this.quantStore.updateLastVector(bgRole, realVector, bgToken);
          logger.debug(`[Memory BG] Đã cập nhật embedding thật cho [${bgRole}] (${content.substring(0, 30)}...)`);
        } catch (e: any) {
          logger.warn(`[Memory BG] Embedding lỗi (bỏ qua): ${e.message}`);
        }
      });
    }

    logger.debug(`[Memory] Đã nén và lưu tin nhắn của [${role}] vào RAM & Quant Store`);
  }

  public async getShortTermHistory(): Promise<ChatMessage[]> {
    // Triệt tiêu Disk I/O: Trả về trực tiếp từ RAM Cache
    return this.memCache;
  }

  public async getHybridContext(
    currentQuery: string,
    windowSize: number = 6,
  ): Promise<ChatMessage[]> {
    // 1. Tạo vector đại diện cho câu hỏi hiện tại
    // 🔒 [Audit Fix C-2/M-5] Dùng embedWithTimeout() — timer tự clear trong finally, zero leak
    let queryEmbedding: number[] = Array.from(
      { length: 256 },
      () => Math.random() * 2 - 1,
    );
    try {
      queryEmbedding = await this.embeddingService.embedWithTimeout(currentQuery, 2000);
    } catch (e: any) {
      logger.warn("[Memory] Embedding timeout/lỗi, dùng dummy vector cho semantic search:", e.message);
    }

    // 2. Tải toàn bộ cửa sổ lịch sử hiện tại
    const fullHistory = await this.getShortTermHistory();

    // Nếu lịch sử còn ngắn, tải thẳng luôn không cần RAG
    if (fullHistory.length <= windowSize) {
      return fullHistory;
    }

    // 3. Sử dụng Sliding Window tách 5-6 tin nhắn gần nhất ráp nguyên bản (Chronological)
    const recentWindow = fullHistory.slice(-windowSize);
    const recentContents = new Set(recentWindow.map((m) => m.content.trim()));

    // 4. Khứ hồi lượng tử các tin nhắn trùng lập ngữ nghĩa ẩn sâu dưới đáy file
    // Fix: Truy vấn cả role "user" và "assistant" thay vì chỉ "system" (tránh miss memories)
    const userToken = this.authority.mintAuthToken("user") as string;
    const assistantToken = this.authority.mintAuthToken("assistant") as string;
    const userResults = this.quantStore.searchSimilar(queryEmbedding, "user", userToken, 2);
    const assistantResults = this.quantStore.searchSimilar(queryEmbedding, "assistant", assistantToken, 2);
    const semanticResults = [...userResults, ...assistantResults];

    const recalledChat: ChatMessage[] = [];
    for (const entry of semanticResults) {
      // Loại trừ tin nhắn vừa nói nãy lặp lại
      if (!recentContents.has(entry.content.trim())) {
        recalledChat.push({
          role: "system",
          content: `[Ký ức cũ liên quan]: Lục lại lịch sử, tôi nhớ ${entry.role === "user" ? "người dùng" : "bản thân (AI)"} từng nói: "${entry.content}"`,
          timestamp: Date.now(),
        });
      }
    }

    logger.debug(
      `[Memory] Khứ hồi ${recalledChat.length} ký ức cũ, ghép với ${recentWindow.length} tin tức thời.`,
    );
    // 5. Kết hợp: Những tin nhắn được khứ hồi nằm trên cùng + Chuỗi hội thoại tức thời nằm ở dưới (kề prompt AI)
    return [...recalledChat, ...recentWindow];
  }

  // Phương thức mới: Cập nhật thông tin vào bộ nhớ dài hạn định dạng Markdown
  public async updateLongTermMemory(
    category: string,
    facts: string[],
  ): Promise<void> {
    try {
      let rawContent = await fs.readFile(this.longTermFilePath, "utf-8");
      let currentContent = decryptData(rawContent);

      // Xây dựng chuỗi văn bản danh sách (Bullet points formatting)
      const newFacts = facts.map((fact) => `- ${fact}`).join("\n");
      const sectionHeader = `## ${category}`;

      if (currentContent.includes(sectionHeader)) {
        // Nếu mục đã tồn tại, chèn thêm (append) vào ngay dưới tiêu đề đó
        currentContent = currentContent.replace(
          sectionHeader,
          `${sectionHeader}\n${newFacts}`,
        );
      } else {
        // Nếu danh mục chưa tồn tại, tạo phần mới ở cuối tệp
        currentContent += `\n${sectionHeader}\n${newFacts}\n`;
      }

      const securedUpdate = encryptData(currentContent);
      const tmpPath = `${this.longTermFilePath}.tmp`;
      await fs.writeFile(tmpPath, securedUpdate, "utf-8");
      await fs.rename(tmpPath, this.longTermFilePath);
    } catch (error) {
      // Nuốt log nếu file lock
    }
  }

  // Đọc toàn bộ tệp Mã hóa giải ngược về Context Sạch
  public async getLongTermContext(): Promise<string> {
    try {
      const rawPayload = await fs.readFile(this.longTermFilePath, "utf-8");
      return decryptData(rawPayload);
    } catch (error) {
      return "";
    }
  }

  // --- Các phương thức làm việc với user profile ---

  public async getUserProfile(): Promise<any> {
    try {
      const data = await fs.readFile(this.userProfilePath, "utf-8");
      return JSON.parse(data);
    } catch (error) {
      logger.error(
        `[Memory] Không thể đọc user_profile.json, trả về null. ${error}`,
      );
      return null;
    }
  }

  public async updateUserProfile(updates: any): Promise<void> {
    try {
      const currentProfile = (await this.getUserProfile()) || {};
      const newProfile = { ...currentProfile, ...updates };

      const tmpPath = `${this.userProfilePath}.tmp`;
      await fs.writeFile(tmpPath, JSON.stringify(newProfile, null, 2), "utf-8");
      await fs.rename(tmpPath, this.userProfilePath);
      logger.info("[Memory] Đã cập nhật user_profile.json thành công.");
    } catch (error) {
      logger.error(`[Memory] Lỗi khi cập nhật user_profile.json: ${error}`);
    }
  }

  // ===========================
  // Structured Memory (KV Store)
  // ===========================

  /**
   * Set a structured fact (key-value pair)
   * This is deterministic memory — injected directly into system prompt
   */
  public setStructuredFact(
    key: string,
    value: string,
    options?: { ttlDays?: number; source?: string; category?: string }
  ): void {
    this.structuredMemory.setFact(key, value, options);
  }

  /**
   * Get all structured facts
   */
  public getStructuredFacts() {
    return this.structuredMemory.getAllFacts();
  }

  /**
   * Get formatted structured memory for system prompt injection
   */
  public getStructuredMemoryPrompt(): string {
    return this.structuredMemory.formatForSystemPrompt();
  }

  /**
   * Delete a structured fact
   */
  public deleteStructuredFact(key: string): boolean {
    return this.structuredMemory.deleteFact(key);
  }
}
