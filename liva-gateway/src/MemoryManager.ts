import { safeRename } from './utils/FileUtils';
import * as fs from 'node:fs/promises';
import * as path from "node:path";
import { QuantizedMemoryStore, CoreKernel } from "./memory/TurboQuantStore";
import { StructuredMemory } from "./memory/StructuredMemory";
import { WorkingBuffer } from "./memory/WorkingBuffer";
import { EmbeddingService } from "./services/EmbeddingService";
import { EncryptionEngine } from "./memory/EncryptionEngine";
import { logger } from "./utils/logger";
import { ConsolidationCron } from "./memory/ConsolidationCron";
import { BookIndex } from "./memory/BookIndex";
import { DualChannelSegmenter } from "./memory/DualChannelSegmenter";
import { ReconsolidationEngine } from "./memory/ReconsolidationEngine";
import { ReflectionDaemon } from "./memory/ReflectionDaemon";
import { longContextReorder } from "./utils/LongContextReorder";
import LRUCache from "lru-cache";
import type OpenAI from "openai";
import { TaskQueue, TaskPriority } from "./core/TaskQueue";

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
  private quantStore: QuantizedMemoryStore;
  private structuredMemory!: StructuredMemory;
  public readonly workingBuffer: WorkingBuffer;
  private readonly authority: CoreKernel;
  private readonly embeddingService: EmbeddingService;
  private readonly agentId: string;
  private memCache: ChatMessage[] = []; // In-memory Cache

  public bookIndex?: BookIndex;
  public consolidationCron?: ConsolidationCron;
  public archivingCron?: any;
  // [H-MEM v18] New modules
  public segmenter?: DualChannelSegmenter;
  public reconsolidationEngine?: ReconsolidationEngine;
  public reflectionDaemon?: ReflectionDaemon;  // [UHM] Background Φ/Ψ extraction

  // [Optimization 1.1] LRU Cache for Vector Search (L0.5 Caching)
  private hybridCache = new LRUCache<string, { role: "user" | "assistant" | "system", content: string }[]>({
      max: 50,
      ttl: 5 * 60 * 1000 // 5 minutes TTL
  });

  constructor(agentId: string, embeddingService?: EmbeddingService) {
    this.agentId = agentId;
    this.memoryDirectory = path.join(process.cwd(), "data", "agents", agentId);
    
    // File-First Memory Paths
    this.sessionStatePath = path.join(this.memoryDirectory, "SESSION-STATE.md");
    this.longTermMarkdownPath = path.join(this.memoryDirectory, "MEMORY.md");

    // Legacy Paths
    this.shortTermFilePath = path.join(this.memoryDirectory, "short_term_memory.jsonl");
    this.longTermFilePath = path.join(this.memoryDirectory, "long_term_memory.enc");
    this.userProfilePath = path.join(process.cwd(), "..", "data", "user_profile.json");

    // Khởi tạo bộ nhớ nén siêu nhẹ
    this.authority = new CoreKernel(["system", "user", "assistant"]);
    this.quantStore = new QuantizedMemoryStore(
      this.authority,
      path.join(this.memoryDirectory, "turbo_quant_memory.jsonl"),
    );
    // Working Buffer (Quản lý Token & Context Compaction)
    this.workingBuffer = new WorkingBuffer(agentId);
    // Shared Embedding Service (Singleton — replaces @xenova/transformers)
    this.embeddingService = embeddingService ?? EmbeddingService.getInstance();
  }

  public async initialize(): Promise<void> {
    try {
      // Structured Memory (KV store bổ trợ RAG) — async factory, zero blocking I/O
      this.structuredMemory = await StructuredMemory.create(this.agentId);

      // Initialize Shared Embedding Service (Singleton — Promise Lock prevents double-load)
      logger.info("[Memory] Đang nạp EmbeddingService singleton (HuggingFace)...");
      await this.embeddingService.ensureReady();
      logger.info("[Memory] EmbeddingService ready.");

      await fs.mkdir(this.memoryDirectory, { recursive: true });

      // Khởi tạo tệp Phôi Ký ức (Mã hóa Encrypted)
      try {
        await fs.access(this.longTermFilePath);
      } catch {
        const initialContent = `# LONG-TERM MEMORY\n*Instruction: Extract facts and store them here in ENGLISH for token efficiency.*\n\n---\n\n## Habits & Preferences\n\n## Acquired Knowledge\n`;
        await EncryptionEngine.writeFileEncrypted(this.longTermFilePath, initialContent);
      }

      // Khởi tạo File-First Memory
      try {
        await fs.access(this.sessionStatePath);
      } catch {
        const sessionTemplate = `# SESSION STATE\n\n## Core Intent\n\n## Current Context\n\n## Pending Tasks\n- [ ] Task 1\n`;
        await fs.writeFile(this.sessionStatePath, sessionTemplate, "utf-8");
      }

      try {
        await fs.access(this.longTermMarkdownPath);
      } catch {
        await fs.writeFile(this.longTermMarkdownPath, "# LONG-TERM MEMORY\n\n", "utf-8");
      }
      
      // Load cache 1 lần duy nhất từ ổ cứng vào bộ nhớ RAM
      const rawHistory = await fs.readFile(
        path.join(this.memoryDirectory, "turbo_quant_memory.jsonl"),
        "utf-8",
      ).catch(() => "");
      
      const lines = rawHistory.split("\n").filter((line) => line.trim() !== "");
      const SESSION_EXPIRY_MS = 2 * 60 * 60 * 1000; // 2 hours expiry
      const now = Date.now();

      try {
        const loadedMessages: ChatMessage[] = [];
        const validLines: string[] = [];

        for (const line of lines) {
          try {
            const parsed = JSON.parse(line);
            const timestamp = parsed.temporal?.timestamp || parsed.timestamp || Date.now();
            if (now - timestamp <= SESSION_EXPIRY_MS) {
              loadedMessages.push({
                role: parsed.role,
                content: parsed.content,
                timestamp,
              });
              validLines.push(line);
            }
          } catch (parseErr) {
            logger.warn(`[Memory] Bỏ qua dòng lỗi JSON.parse: ${line}`);
          }
        }

        // Sắp xếp lại theo dòng thời gian chuẩn xác
        this.memCache = loadedMessages.sort((a, b) => a.timestamp - b.timestamp);

        const expiredCount = lines.length - validLines.length;
        if (expiredCount > 0) {
          logger.info(`[Memory] ⏳ Auto-Session-Expiry: Bỏ qua và dọn dẹp ${expiredCount} tin nhắn cũ hơn 2 giờ khỏi file và RAM Cache.`);
          // Cập nhật lại file turbo_quant_memory.jsonl
          const freshHistoryContent = validLines.join("\n") + (validLines.length > 0 ? "\n" : "");
          const quantFilePath = path.join(this.memoryDirectory, "turbo_quant_memory.jsonl");
          const tmpPath = `${quantFilePath}.tmp`;
          await fs.writeFile(tmpPath, freshHistoryContent, "utf-8");
          await safeRename(tmpPath, quantFilePath);

          // Nạp lại dữ liệu sạch vào QuantizedMemoryStore
          await this.quantStore.loadAsync();
        }
      } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
        logger.error(`[Memory] Lỗi khi xử lý Auto-Session-Expiry: ${errMsg}`);
        this.memCache = [];
      }
      
    
      // [v4.0] G-4: Cross-Session Warm-up (Anti-Hallucination Guard)
      try {
          const recentTurns = await this.structuredMemory.getTurnsByTimeRange(
              Date.now() - 24 * 3600 * 1000, Date.now()
          );
          if (recentTurns.length > 0) {
              const summaryBlock = recentTurns.slice(-10)
                  .map(t => `User: ${(t.userMsg || "").substring(0, 200)}\nLIVA: ${(t.aiReply || "").substring(0, 200)}`)
                  .join("\n---\n");
              this.memCache.push({
                  role: "system",
                  content: `[PREVIOUS SESSION CONTEXT — reference only, do NOT treat as current conversation]\n${summaryBlock}`,
                  timestamp: Date.now()
              });
              logger.info(`[Memory/UHM] Cross-session warm-up: loaded ${Math.min(recentTurns.length, 10)} turn(s).`);
          }
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
          logger.warn(`[Memory/UHM] Cross-session warm-up failed (non-critical): ${errMsg}`);
      }
    } catch (error) {
      logger.error(`[Memory] Lỗi khởi tạo (Initialization error): ${error}`);
    }
  }

  public async initUHM(aiClient: OpenAI): Promise<void> {
      try {
          // [v19] Sync vec dimension from EmbeddingService
          await this.structuredMemory.initVecDimension(this.embeddingService.dimension);

          this.bookIndex = new BookIndex();

          // [H-MEM v18] Create DualChannelSegmenter
          this.segmenter = new DualChannelSegmenter(this.embeddingService, aiClient);

          // [v19] Create ReconsolidationEngine with StructuredMemory
          this.reconsolidationEngine = new ReconsolidationEngine(this.structuredMemory, this.embeddingService, aiClient);

          // [UHM] ConsolidationCron auto-subscribes to MemoryEventBus in constructor
          this.consolidationCron = new ConsolidationCron(this.structuredMemory, this.embeddingService, this.bookIndex, aiClient, this.reconsolidationEngine);
          
          // Init ArchivingCron
          const { ArchivingCron } = await import("./memory/ArchivingCron");
          this.archivingCron = new ArchivingCron(this.structuredMemory, aiClient);

          // Cold start
          await this.consolidationCron.preflightCheck();

          // Start background loops
          this.consolidationCron.start();
          this.archivingCron.start();

          // [UHM] Create ReflectionDaemon — signals ConsolidationCron via MemoryEventBus (no import coupling)
          this.reflectionDaemon = new ReflectionDaemon(
              this.structuredMemory, aiClient, this.segmenter, this.embeddingService
          );

          logger.info("[Memory] UHM with RAPTOR + sqlite-vec v19 initialized.");
      } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
          logger.error(`[Memory] initUHM failed: ${errMsg}`);
          throw e;
      }
  }

  // [Z-MAS RAM Healer] Dọn dẹp tài nguyên ngầm khi shutdown
  public async dispose() {
      // [v19] Flush memory touch queue BEFORE closing SQLite
      if (this.structuredMemory) {
          await this.structuredMemory.flushTouchQueue();
      }
      // [UHM] Flush pending ReflectionDaemon turns before consolidation shutdown
      if (this.reflectionDaemon) {
          await this.reflectionDaemon.flushPending();
          this.reflectionDaemon.dispose();
      }
      if (this.consolidationCron) this.consolidationCron.dispose();
      await this.quantStore.dispose();
      // 🔒 [Audit Fix C-3] Close SQLite connection (AFTER flush is complete)
      if (this.structuredMemory) {
          await this.structuredMemory.close();
      }
      logger.info("[Memory] Đã giải phóng hoàn toàn các luồng Garbage Collection nền.");
  }

  /** Expose StructuredMemory instance for DI (prevents duplicate instantiation) */
  
  public async purgeUserContext(): Promise<void> {
      try {
          if (this.structuredMemory) {
              await this.structuredMemory.deleteAllVectors();
              this.structuredMemory.deleteAllFacts();
              await this.structuredMemory.deleteAllEvents();
              this.structuredMemory.db.exec("DELETE FROM l3_edges");
              this.structuredMemory.db.exec("DELETE FROM l3_nodes");
          }
          await this.quantStore.dispose(); // Release all tensor caches and entries
          // Reset memcache
          this.memCache = [];
          
          await fs.writeFile(this.sessionStatePath, "# SESSION STATE\n", "utf-8");
          await fs.writeFile(this.longTermMarkdownPath, "# LONG-TERM MEMORY\n", "utf-8");

          logger.info("[Memory] Phục hồi (Purge) Dữ liệu người dùng (GDPR) hoàn tất.");
      } catch (error) {
          logger.error(`[Memory] Lỗi trong quá trình Purge (GDPR): ${error}`);
      }
  }

  /**
   * [P5] Reset ALL memory to blank slate — preserves user_profile.json only.
   * Wipes: SESSION-STATE, MEMORY.md, long_term_memory.enc, short_term_memory.jsonl,
   *        turbo_quant_memory.jsonl, structured_memory.sqlite (+ WAL/SHM).
   * In-memory: Clears memCache, quantStore, workingBuffer.
   */
  public async resetAllMemory(): Promise<{ success: boolean; error?: string }> {
      try {
          logger.warn("[Memory] 🧹 RESET ALL MEMORY — Bắt đầu xóa trắng toàn bộ trí nhớ...");

          // 1. Flush any pending writes before deleting files
          if (this.structuredMemory) {
              try {
                  await this.structuredMemory.flushTouchQueue();
              } catch { /* ignore cleanup errors */ }
          }
          if (this.consolidationCron) {
              this.consolidationCron.dispose();
              this.consolidationCron = undefined;
          }

          // 2. Close SQLite connection before deleting .sqlite files
          try {
              if (this.structuredMemory) {
                  await this.structuredMemory.close();
              }
          } catch { /* ignore */ }

          // 3. Clear in-memory caches
          this.memCache = [];
          await this.workingBuffer.clear();
          try { await this.quantStore.dispose(); } catch { /* ignore */ }

          // 4. Delete all memory files (atomic: delete one by one, skip errors)
          const filesToDelete = [
              this.sessionStatePath,
              this.longTermMarkdownPath,
              this.longTermFilePath,
              this.shortTermFilePath,
              path.join(this.memoryDirectory, "turbo_quant_memory.jsonl"),
              path.join(this.memoryDirectory, "structured_memory.sqlite"),
              path.join(this.memoryDirectory, "structured_memory.sqlite-shm"),
              path.join(this.memoryDirectory, "structured_memory.sqlite-wal"),
          ];

          for (const filePath of filesToDelete) {
              try {
                  await fs.unlink(filePath);
                  logger.debug(`[Memory/Reset] 🗑️ Deleted: ${path.basename(filePath)}`);
              } catch {
                  // File may not exist — skip silently
              }
          }

          // 5. Delete legacy memory subdirectory (from previous LanceDB installations) if exists
          const memorySubDir = path.join(this.memoryDirectory, "memory");
          try {
              await fs.rm(memorySubDir, { recursive: true, force: true });
          } catch { /* ignore */ }

          // 6. Re-create template files (blank slate)
          await fs.mkdir(this.memoryDirectory, { recursive: true });

          const sessionTemplate = `# SESSION STATE\n\n## Core Intent\n\n## Current Context\n\n## Pending Tasks\n- [ ] Task 1\n`;
          await fs.writeFile(this.sessionStatePath, sessionTemplate, "utf-8");

          await fs.writeFile(this.longTermMarkdownPath, "# LONG-TERM MEMORY\n\n", "utf-8");

          const initialLT = `# LONG-TERM MEMORY\n*Instruction: Extract facts and store them here in ENGLISH for token efficiency.*\n\n---\n\n## Habits & Preferences\n\n## Acquired Knowledge\n`;
          await EncryptionEngine.writeFileEncrypted(this.longTermFilePath, initialLT);

          // Empty JSONL files
          await fs.writeFile(this.shortTermFilePath, "", "utf-8");
          await fs.writeFile(path.join(this.memoryDirectory, "turbo_quant_memory.jsonl"), "", "utf-8");

          // 7. Re-initialize StructuredMemory (new SQLite DB)
          this.structuredMemory = await StructuredMemory.create(this.agentId);

          // 8. Re-initialize QuantStore (new empty JSONL)
          this.quantStore = new QuantizedMemoryStore(
              this.authority,
              path.join(this.memoryDirectory, "turbo_quant_memory.jsonl"),
          );

          logger.info("[Memory] ✅ RESET ALL MEMORY hoàn tất — Trí nhớ trắng như tờ giấy mới.");
          return { success: true };
      } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          logger.error(`[Memory] ❌ Reset All Memory thất bại: ${errMsg}`);
          return { success: false, error: errMsg };
      }
  }

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
    await safeRename(tmpPath, this.sessionStatePath);
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
 // NOSONAR

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

    // Background embedding via shared EmbeddingService (non-blocking, queued to prevent VRAM spikes)
    if (this.embeddingService.ready) {
      const bgRole = role;
      const bgToken = token;
      
      TaskQueue.wrapMemoryTask(
        async () => {
          const realVector = await this.embeddingService.embed(content);
          // [Audit Fix H-2] Ghi đè dummy vector bằng real embedding vào QuantStore
          this.quantStore.updateLastVector(bgRole, realVector, bgToken);
          logger.debug(`[Memory BG] Đã cập nhật embedding thật cho [${bgRole}] (${content.substring(0, 30)}...)`);
        },
        `MemoryManager-BackgroundEmbed-${Date.now()}`,
        TaskPriority.NORMAL
      ).catch((e: unknown) => {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.warn(`[Memory BG] Embedding lỗi (bỏ qua): ${errMsg}`);
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
    // 1. Tải toàn bộ cửa sổ lịch sử hiện tại
    const fullHistory = await this.getShortTermHistory();

    // Nếu lịch sử còn ngắn, tải thẳng luôn không cần RAG
    if (fullHistory.length <= windowSize) {
      return fullHistory;
    }

    // 2. Sử dụng Sliding Window tách 5-6 tin nhắn gần nhất ráp nguyên bản (Chronological)
    const recentWindow = fullHistory.slice(-windowSize);
    const recentContents = new Set(recentWindow.map((m) => m.content.trim()));

    // [Optimization 1.1] Fast-Path L0.5 Cache Lookup
    const cacheKey = Buffer.from(currentQuery.trim()).toString("base64");
    let cachedRecalled = this.hybridCache.get(cacheKey);

    if (!cachedRecalled) {
      // 3. Tạo vector đại diện cho câu hỏi hiện tại
      // 🔒 [Audit Fix C-2/M-5] Dùng embedWithTimeout() — timer tự clear trong finally, zero leak
      let queryEmbedding: number[] = Array.from(
        { length: 256 },
        () => Math.random() * 2 - 1,
   // NOSONAR
      );
      try {
        queryEmbedding = await this.embeddingService.embedWithTimeout(currentQuery, 2000);
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.warn("[Memory] Embedding timeout/lỗi, dùng dummy vector cho semantic search:" + " " + errMsg);
      }

      // 4. Khứ hồi lượng tử các tin nhắn trùng lập ngữ nghĩa ẩn sâu dưới đáy file
      // Fix: Truy vấn cả role "user" và "assistant" thay vì chỉ "system" (tránh miss memories)
      const userToken = this.authority.mintAuthToken("user") as string;
      const assistantToken = this.authority.mintAuthToken("assistant") as string;
      const userResults = this.quantStore.searchSimilar(queryEmbedding, "user", userToken, 2);
      const assistantResults = this.quantStore.searchSimilar(queryEmbedding, "assistant", assistantToken, 2);
      const semanticResults = [...userResults, ...assistantResults];

      cachedRecalled = semanticResults.map(entry => ({
          role: entry.role as "user" | "assistant" | "system",
          content: entry.content
      }));
      this.hybridCache.set(cacheKey, cachedRecalled);
      logger.debug(`[Memory] L0.5 Cache Miss: Vector search xong và lưu ${cachedRecalled.length} kết quả vào Cache.`);
    } else {
      logger.debug(`[Memory] L0.5 Cache Hit (Fast-Path): Bỏ qua Vector Search, nạp trực tiếp ${cachedRecalled.length} kết quả.`);
    }

    const recalledChat: ChatMessage[] = [];
    for (const entry of cachedRecalled) {
      // Loại trừ tin nhắn vừa nói nãy lặp lại
      if (!recentContents.has(entry.content.trim())) {
        recalledChat.push({
          role: "system",
          content: `[Recalled Context]: Regarding the past, I recall ${entry.role} saying: "${entry.content}"`,
          timestamp: Date.now(),
        });
      }
    }

    const finalRecalled = longContextReorder(recalledChat);
    logger.debug(
      `[Memory] Khứ hồi ${finalRecalled.length} ký ức cũ (đã xếp lại LongContextReorder), ghép với ${recentWindow.length} tin tức thời.`,
    );
    return [...finalRecalled, ...recentWindow];
  }

  // Phương thức mới: Cập nhật thông tin vào bộ nhớ dài hạn định dạng Markdown
  public async updateLongTermMemory(
    category: string,
    facts: string[],
  ): Promise<void> {
    try {
      let currentContent = await EncryptionEngine.readFileDecrypted(this.longTermFilePath);

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

      await EncryptionEngine.writeFileEncrypted(this.longTermFilePath, currentContent);
    } catch (error) {
      // Nuốt log nếu file lock
    }
  }

  // Đọc toàn bộ tệp Mã hóa giải ngược về Context Sạch
  public async getLongTermContext(): Promise<string> {
    return EncryptionEngine.readFileDecrypted(this.longTermFilePath);
  }

  // --- Các phương thức làm việc với user profile ---

  public async getUserProfile(): Promise<any> {
    try {
      const data = await fs.readFile(this.userProfilePath, "utf-8");
      return JSON.parse(data);
    } catch (error: unknown) {
      const isENOENT = error instanceof Error && 'code' in error && (error as NodeJS.ErrnoException).code === 'ENOENT';
      if (isENOENT) {
        logger.info("[Memory] user_profile.json chưa được tạo (Đang chờ Onboarding).");
      } else {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[Memory] Không thể đọc user_profile.json, trả về null. ${errMsg}`);
      }
      return null;
    }
  }

  public async updateUserProfile(updates: any): Promise<void> {
    try {
      const currentProfile = (await this.getUserProfile()) || {};
      const newProfile = { ...currentProfile, ...updates };

      const tmpPath = `${this.userProfilePath}.tmp`;
      await fs.writeFile(tmpPath, JSON.stringify(newProfile, null, 2), "utf-8");
      await safeRename(tmpPath, this.userProfilePath);
      logger.info("[Memory] Đã cập nhật user_profile.json thành công.");
    } catch (error: unknown) {
      const errMsg = error instanceof Error ? error.message : String(error);
      logger.error(`[Memory] Lỗi khi cập nhật user_profile.json: ${errMsg}`);
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

  public get db() {
    return this.structuredMemory?.db;
  }

  public getAllFacts() {
    return this.structuredMemory?.getAllFacts() || [];
  }

  public async consolidateNow(force: boolean = false): Promise<number> {
    if (!this.consolidationCron) return 0;
    return this.consolidationCron.consolidateNow(force);
  }
}
