import { safeRename } from './utils/FileUtils';
import * as fs from 'node:fs/promises';
import * as path from "node:path";
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
import { generateULID } from "./utils/ULID";

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
  // [v27] TurboQuantStore DEPRECATED — all vector ops consolidated to L2 sqlite-vec
  private structuredMemory!: StructuredMemory;
  public readonly workingBuffer: WorkingBuffer;
  private readonly embeddingService: EmbeddingService;
  private readonly agentId: string;
  private memCache: ChatMessage[] = []; // In-memory Cache

  public bookIndex?: BookIndex;
  public consolidationCron?: ConsolidationCron;
  public archivingCron?: { start: () => void; stop: () => void };
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

    // Working Buffer (Quản lý Token & Context Compaction)
    this.workingBuffer = new WorkingBuffer(agentId);
    // Shared Embedding Service (Singleton — replaces @xenova/transformers)
    this.embeddingService = embeddingService ?? EmbeddingService.getInstance();
  }

  public async initialize(): Promise<void> {
    try {
      // Structured Memory (KV store bổ trợ RAG) — async factory, zero blocking I/O
      const isTest = this.agentId.includes("test") || process.env.NODE_ENV === "test" || process.env.VITEST === "true";
      const customStorePath = isTest ? path.join(this.memoryDirectory, "structured_memory.sqlite") : undefined;
      this.structuredMemory = await StructuredMemory.create(this.agentId, customStorePath);

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
      
      // [v27] Load recent turns from L1 SQLite (replaces turbo_quant_memory.jsonl)
      // Only load turns from the last 2 hours as session warm-up
      try {
        const SESSION_EXPIRY_MS = 2 * 60 * 60 * 1000; // 2 hours expiry
        const cutoff = Date.now() - SESSION_EXPIRY_MS;
        const recentTurnNodes = await this.structuredMemory.getTurnsByTimeRange(cutoff, Date.now());
        const loadedMsgs: ChatMessage[] = [];
        for (const t of recentTurnNodes) {
          if (t.userMsg && t.userMsg.trim()) {
            loadedMsgs.push({
              role: 'user',
              content: t.userMsg.trim(),
              timestamp: t.temporal_anchor
            });
          }
          if (t.aiReply && t.aiReply.trim()) {
            loadedMsgs.push({
              role: 'assistant',
              content: t.aiReply.trim(),
              timestamp: t.temporal_anchor + 1
            });
          }
        }
        this.memCache = loadedMsgs;
        if (this.memCache.length > 0) {
          logger.info(`[Memory] Loaded ${this.memCache.length} recent turns from L1 SQLite (last 2h).`);
        }
      } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
        logger.warn(`[Memory] Failed to warm-up from L1 turns (non-critical): ${errMsg}`);
        this.memCache = [];
      }

      // [v27] Auto-cleanup legacy turbo_quant_memory.jsonl if it still exists
      try {
        const legacyQuantFile = path.join(this.memoryDirectory, "turbo_quant_memory.jsonl");
        await fs.unlink(legacyQuantFile);
        logger.info(`[Memory] 🗑️ Cleaned up legacy turbo_quant_memory.jsonl`);
      } catch { /* file doesn't exist — expected */ }
      
    
      // [v4.0] G-4: Cross-Session Warm-up (Anti-Hallucination Guard)
      // [BUG-7 Fix] Moved to system prompt injection (getPreviousSessionContextPrompt) to prevent chat history pollution.
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
      // [BUG-4 Fix] Stop ArchivingCron timer (was missing from shutdown chain)
      if (this.archivingCron) this.archivingCron.stop();
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
              await this.structuredMemory.deleteAllFacts();
              await this.structuredMemory.deleteAllEvents();
              await this.structuredMemory.dbBridge.exec("DELETE FROM l3_edges");
              await this.structuredMemory.dbBridge.exec("DELETE FROM l3_nodes");
          }
          // [v27] No quantStore to dispose — all vectors in L2 sqlite-vec
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
   * [v27] Reset ALL memory to blank slate — preserves user_profile.json only.
   * Wipes: SESSION-STATE, MEMORY.md, long_term_memory.enc, short_term_memory.jsonl,
   *        structured_memory.sqlite (+ WAL/SHM).
   * In-memory: Clears memCache, workingBuffer.
   */
  public async resetAllMemory(): Promise<{ success: boolean; error?: string }> {
      try {
          logger.warn("[Memory] 🧹 RESET ALL MEMORY — Bắt đầu xóa trắng toàn bộ trí nhớ...");

          // 1. Flush any pending writes and get DB path before deleting files
          let dbPathToDelete = path.join(this.memoryDirectory, "structured_memory.sqlite");
          if (this.structuredMemory) {
              try {
                  dbPathToDelete = this.structuredMemory.getDbPath();
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

          // 4. Delete all memory files (atomic: delete one by one, skip errors)
          const filesToDelete = [
              this.sessionStatePath,
              this.longTermMarkdownPath,
              this.longTermFilePath,
              this.shortTermFilePath,
              path.join(this.memoryDirectory, "turbo_quant_memory.jsonl"), // [v27] Legacy cleanup
              dbPathToDelete,
              `${dbPathToDelete}-shm`,
              `${dbPathToDelete}-wal`,
          ];

          for (const filePath of filesToDelete) {
              try {
                  await fs.unlink(filePath);
                  logger.debug(`[Memory/Reset] 🗑️ Deleted: ${path.basename(filePath)}`);
              } catch (err: unknown) {
                  const error = err as NodeJS.ErrnoException;
                  if (error.code !== "ENOENT") {
                      logger.error(`[Memory/Reset] ❌ Failed to delete ${path.basename(filePath)}: ${error.message}`);
                  }
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

          // Empty JSONL files (legacy)
          await fs.writeFile(this.shortTermFilePath, "", "utf-8");

          // 7. Re-initialize StructuredMemory (new SQLite DB)
          this.structuredMemory = await StructuredMemory.create(this.agentId);

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
    // [v27] Direct RAM cache — no quantStore dependency
    this.memCache.push({ role, content, timestamp: Date.now() });

    // RAM Cache cap — prevent unbounded growth and context window overflow (hallucinations)
    if (this.memCache.length > 50) {
        this.memCache = this.memCache.slice(-30);
        logger.info(`[Memory GC] Đã chặt bỏ 20 tin nhắn cũ khỏi RAM Cache để tránh tràn ngữ cảnh.`);
    }

    // [v27] Background: embed → L2 sqlite-vec (replaces quantStore)
    if (this.embeddingService.ready) {
      TaskQueue.wrapMemoryTask(
        async () => {
          const vector = await this.embeddingService.embed(content);
          // [MEM-2 Fix] ULID replaces Date.now()+random — time-sortable, collision-proof
          const vecId = `msg_${role}_${generateULID()}`;
          this.structuredMemory.upsertVector({
            vecId,
            type: 'CONVERSATION',
            content,
            vector,
            domain: 'Conversation',
            category: role,
          });
          logger.debug(`[Memory BG] Embedded [${role}] → L2 sqlite-vec (${content.substring(0, 30)}...)`);
        },
        `MemoryManager-Embed-${Date.now()}`,
        TaskPriority.NORMAL
      ).catch((e: unknown) => {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.warn(`[Memory BG] Embedding lỗi (bỏ qua): ${errMsg}`);
      });
    }

    logger.debug(`[Memory] Đã lưu tin nhắn của [${role}] vào RAM Cache + L2 background embed.`);
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
      // 3. [v27] Tạo embedding vector cho câu hỏi hiện tại
      let queryEmbedding: number[] = [];
      try {
        queryEmbedding = await this.embeddingService.embedWithTimeout(currentQuery, 2000);
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.warn("[Memory] Embedding timeout/lỗi, bỏ qua semantic search:" + " " + errMsg);
      }

      // 4. [v27] Hybrid search via L2 sqlite-vec (RRF: KNN + FTS5) — replaces TurboQuantStore
      let semanticResults: Array<{ content: string; category: string }> = [];
      if (queryEmbedding.length > 0) {
        try {
          const hybridResults = await this.structuredMemory.searchHybridVectors(
            currentQuery, queryEmbedding, 4
          );
          semanticResults = hybridResults.map(r => ({
            content: r.content,
            category: r.category
          }));
        } catch (searchErr: unknown) {
          const errMsg = searchErr instanceof Error ? searchErr.message : String(searchErr);
          logger.warn(`[Memory] L2 hybrid search failed (non-critical): ${errMsg}`);
        }
      }

      cachedRecalled = semanticResults.map(entry => ({
          role: (entry.category === 'assistant' ? 'assistant' : 'user') as "user" | "assistant" | "system",
          content: entry.content
      }));
      this.hybridCache.set(cacheKey, cachedRecalled);
      logger.debug(`[Memory] L0.5 Cache Miss: L2 hybrid search → ${cachedRecalled.length} kết quả.`);
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

    // [v28] Token-aware trimming — prevent history from overflowing context window
    const combined = [...finalRecalled, ...recentWindow];
    return MemoryManager.trimHistoryToTokenBudget(combined);
  }

  /**
   * [v28] Estimate token count from text using char/4 heuristic.
   * Accurate within ~15% for English/Vietnamese mixed text.
   */
  public static estimateTokens(text: string): number {
    if (!text) return 0;
    return Math.ceil(text.length / 4);
  }

  /**
   * [v28] Trim history messages to fit within 20% of context window.
   * Strategy: truncate individual long messages → drop oldest recalled context.
   */
  private static trimHistoryToTokenBudget(messages: ChatMessage[]): ChatMessage[] {
    let contextTokens = 8192;
    try {
      // Dynamic import avoidance: use lazy check
      const { ConfigManager } = require("./core/config/ConfigManager");
      contextTokens = ConfigManager.getInstance().contextWindowTokens;
    } catch { /* ConfigManager not initialized yet — use default */ }

    const HISTORY_TOKEN_BUDGET = Math.floor(contextTokens * 0.20);
    const MAX_MSG_TOKENS = 300; // ~1200 chars per message max

    // Step 1: Truncate individual messages that are too long
    const trimmed = messages.map(msg => {
      const tokens = MemoryManager.estimateTokens(msg.content);
      if (tokens > MAX_MSG_TOKENS) {
        return { ...msg, content: msg.content.substring(0, MAX_MSG_TOKENS * 4) + "\n[...truncated]" };
      }
      return msg;
    });

    // Step 2: Drop oldest messages (from front = recalled context) if total exceeds budget
    let totalTokens = trimmed.reduce((sum, m) => sum + MemoryManager.estimateTokens(m.content), 0);
    if (totalTokens <= HISTORY_TOKEN_BUDGET) return trimmed;

    // Drop recalled context (system role) first, then oldest user/assistant
    const result: ChatMessage[] = [];
    // Iterate from newest to oldest (end to start) — keep newest messages
    for (let i = trimmed.length - 1; i >= 0; i--) {
      const msgTokens = MemoryManager.estimateTokens(trimmed[i].content);
      if (totalTokens > HISTORY_TOKEN_BUDGET && trimmed[i].role === "system") {
        totalTokens -= msgTokens;
        continue; // Drop recalled context first
      }
      result.unshift(trimmed[i]);
    }

    // If still over budget, hard trim from front
    while (result.length > 2 && MemoryManager.estimateTokens(result.map(m => m.content).join("")) > HISTORY_TOKEN_BUDGET) {
      result.shift();
    }

    if (result.length < messages.length) {
      logger.debug(`[Memory/v28] Token-aware trim: ${messages.length} → ${result.length} messages (budget: ${HISTORY_TOKEN_BUDGET} tokens)`);
    }
    return result;
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
    } catch {
      // Nuốt log nếu file lock
    }
  }

  // Đọc toàn bộ tệp Mã hóa giải ngược về Context Sạch
  public async getLongTermContext(): Promise<string> {
    return EncryptionEngine.readFileDecrypted(this.longTermFilePath);
  }

  // --- Các phương thức làm việc với user profile ---

  public async getUserProfile(): Promise<Record<string, unknown> | null> {
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

  public async updateUserProfile(updates: Record<string, unknown>): Promise<void> {
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
  public async setStructuredFact(
    key: string,
    value: string,
    options?: { ttlDays?: number; source?: string; category?: string }
  ): Promise<void> {
    await this.structuredMemory.setFact(key, value, options);
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
   * [v26] Clear session context for stateless testing
   */
  public async clearSession(): Promise<void> {
    this.memCache = [];
    this.hybridCache.clear();
    await this.workingBuffer.clear();
  }

  /**
   * Delete a structured fact
   */
  public async deleteStructuredFact(key: string): Promise<boolean> {
    return await this.structuredMemory.deleteFact(key);
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

  /**
   * [v4.0] G-4: Cross-Session Warm-up (Anti-Hallucination Guard)
   * Formats the summary of conversation turns from the previous session (older than 2 hours, within 24 hours)
   * to be injected directly into the system prompt context.
   */
  public async getPreviousSessionContextPrompt(): Promise<string> {
    try {
      const SESSION_EXPIRY_MS_CROSS = 2 * 60 * 60 * 1000; // 2 hours
      const recentTurns = await this.structuredMemory.getTurnsByTimeRange(
        Date.now() - 24 * 3600 * 1000, Date.now() - SESSION_EXPIRY_MS_CROSS
      );
      if (recentTurns.length > 0) {
        const summaryBlock = recentTurns.slice(-5)
          .map(t => `User: ${(t.userMsg || "").substring(0, 120)}\nAssistant: ${(t.aiReply || "").substring(0, 120)}`)
          .join("\n---\n");
        return `\n\n<PREVIOUS_SESSION_CONTEXT>\n[SYSTEM NOTE: The following is a summary of conversation turns from the previous session within the last 24 hours. Use it only for context. Do NOT repeat or mimic its formatting in your response.]\n${summaryBlock}\n</PREVIOUS_SESSION_CONTEXT>\n`;
      }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.warn(`[Memory/UHM] getPreviousSessionContextPrompt failed: ${errMsg}`);
    }
    return "";
  }
}
