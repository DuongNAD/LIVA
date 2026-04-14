import * as fs from "fs/promises";
import * as path from "path";
import * as crypto from "crypto";
import { QuantizedMemoryStore, CoreKernel } from "./memory/TurboQuantStore";
import { pipeline, FeatureExtractionPipeline } from "@xenova/transformers";

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
  private readonly memoryDirectory: string;
  private readonly shortTermFilePath: string;
  private readonly longTermFilePath: string;
  private readonly userProfilePath: string;
  private readonly quantStore: QuantizedMemoryStore;
  private readonly authority: CoreKernel;
  private embedder: FeatureExtractionPipeline | null = null;
  private memCache: ChatMessage[] = []; // In-memory Cache

  constructor(agentId: string) {
    this.memoryDirectory = path.join(process.cwd(), "data", "agents", agentId);
    this.shortTermFilePath = path.join(
      this.memoryDirectory,
      "short_term_memory.jsonl",
    );
    // Nâng cấp Z-MAS: Chuyển đổi định dạng thu thập thành .enc siêu bảo mật
    this.longTermFilePath = path.join(
      this.memoryDirectory,
      "long_term_memory.enc",
    );
    // File user_profile.json (lưu trữ hồ sơ cá nhân của người dùng)
    this.userProfilePath = path.join(process.cwd(), "src", "user_profile.json");

    // Khởi tạo bộ nhớ nén siêu nhẹ
    this.authority = new CoreKernel(["system", "user", "assistant"]);
    this.quantStore = new QuantizedMemoryStore(
      this.authority,
      path.join(this.memoryDirectory, "turbo_quant_memory.jsonl"),
    );
  }

  public async initialize(): Promise<void> {
    try {
      // Load Local Embedding Model (Không dùng external API)
      console.log(
        "[Memory] Đang nạp mô hình Nhúng (Embedding Model) cục bộ...",
      );
      try {
        this.embedder = await pipeline(
          "feature-extraction",
          "Xenova/all-MiniLM-L6-v2",
        );
        console.log("[Memory] Đã load xong Local Embedding Model (Xenova).");
      } catch (err: any) {
        console.error(
          "[Memory] Không thể chạy pipeline Xenova embeddings:",
          err.message,
        );
      }

      await fs.mkdir(this.memoryDirectory, { recursive: true });

      // Khởi tạo tệp Phôi Ký ức (Mã hóa Encrypted)
      try {
        await fs.access(this.longTermFilePath);
      } catch {
        const initialContent = `# Hồ Sơ Ký Ức Dài Hạn (Long-term Context)\n\n*Hệ thống sẽ định kỳ trích xuất (extract) và ghi chú các sự thật (facts) quan trọng vào đây.*\n\n---\n\n## Thói quen & Sở thích (Habits & Preferences)\n\n## Kiến thức đã học (Learned Knowledge)\n`;
        const securedPayload = encryptData(initialContent);
        await fs.writeFile(this.longTermFilePath, securedPayload, "utf-8");
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
      console.error("[Memory] Lỗi khởi tạo (Initialization error):", error);
    }
  }

  public async addMessage(
    role: "user" | "assistant" | "system",
    content: string,
  ): Promise<void> {
    let embeddingVector: number[] = Array.from(
      { length: 256 },
      () => Math.random() * 2 - 1,
    );

    if (this.embedder) {
      // Tạo vector thật từ LLM thay vì dummy random
      const output = await this.embedder(content, {
        pooling: "mean",
        normalize: true,
      });
      embeddingVector = Array.from(output.data);
    }

    // Đẩy song song vào QuantStore (nếu vẫn dùng)
    const token = this.authority.mintAuthToken(role) as string;
    await this.quantStore.addMemory(role, content, embeddingVector, token);
    
    // Ghi đệm vào RAM Cache
    this.memCache.push({ role, content, timestamp: Date.now() });
    
    console.log(
      `[Memory] Đã nén và lưu tin nhắn của [${role}] vào RAM & Quant Store`,
    );
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
    let queryEmbedding: number[] = Array.from(
      { length: 256 },
      () => Math.random() * 2 - 1,
    );
    if (this.embedder) {
      try {
        const output = await this.embedder(currentQuery, {
          pooling: "mean",
          normalize: true,
        });
        queryEmbedding = Array.from(output.data);
      } catch (e) {
        console.error("[Memory] Lỗi nhúng văn bản (Embedding error):", e);
      }
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
    const token = this.authority.mintAuthToken("system") as string;
    const semanticResults = this.quantStore.searchSimilar(queryEmbedding, "system", token, 3);

    const recalledChat: ChatMessage[] = [];
    for (const entry of semanticResults) {
      // Loại trừ tin nhắn vừa nói nãy lặp lại, và bỏ qua system prompt
      if (
        entry.role !== "system" &&
        !recentContents.has(entry.content.trim())
      ) {
        recalledChat.push({
          role: "system",
          content: `[Ký ức cũ liên quan]: Lục lại lịch sử, tôi nhớ ${entry.role === "user" ? "người dùng" : "bản thân (AI)"} từng nói: "${entry.content}"`,
          timestamp: Date.now(),
        });
      }
    }

    console.log(
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
      await fs.writeFile(this.longTermFilePath, securedUpdate, "utf-8");
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
      console.error(
        "[Memory] Không thể đọc user_profile.json, trả về null.",
        error,
      );
      return null;
    }
  }

  public async updateUserProfile(updates: any): Promise<void> {
    try {
      const currentProfile = (await this.getUserProfile()) || {};
      const newProfile = { ...currentProfile, ...updates };

      await fs.writeFile(
        this.userProfilePath,
        JSON.stringify(newProfile, null, 2),
        "utf-8",
      );
      console.log("[Memory] Đã cập nhật user_profile.json thành công.");
    } catch (error) {
      console.error("[Memory] Lỗi khi cập nhật user_profile.json:", error);
    }
  }
}
