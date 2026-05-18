import { safeRename } from '../utils/FileUtils';
import { EventEmitter } from 'node:events';
import { WebSocketServer, WebSocket, AddressInfo } from "ws";
import { promises as fsp } from "node:fs";
import * as syncFs from "node:fs";
import * as path from "node:path";
import { randomUUID } from "node:crypto";
import { z } from "zod";
import { logger } from "../utils/logger";
import { FileExplorer } from "../services/FileExplorer";
import { AppConfig } from "../config/AppConfig";

const SystemConfigSchema = z.object({
  geolocationEnabled: z.boolean().optional(),
  proactiveEnabled: z.boolean().optional(),
  proactiveHour: z.number().min(0).max(23).optional(),
  proactiveMinute: z.number().min(0).max(59).optional(),
  proactiveDeliverUI: z.boolean().optional(),
  proactiveDeliverTelegram: z.boolean().optional(),
  proactiveDeliverZalo: z.boolean().optional(),
  proactiveDeliverEmail: z.boolean().optional(),
  proactiveFocus: z.string().optional(),

  digestInterestsEnabled: z.boolean().optional(),
  digestInterestsHour: z.number().min(0).max(23).optional(),
  digestInterestsMinute: z.number().min(0).max(59).optional(),
  digestInterestsDeliverUI: z.boolean().optional(),
  digestInterestsDeliverTelegram: z.boolean().optional(),
  digestInterestsDeliverZalo: z.boolean().optional(),
  digestInterestsDeliverEmail: z.boolean().optional(),

  digestFocusEnabled: z.boolean().optional(),
  digestFocusHour: z.number().min(0).max(23).optional(),
  digestFocusMinute: z.number().min(0).max(59).optional(),
  digestFocusDeliverUI: z.boolean().optional(),
  digestFocusDeliverTelegram: z.boolean().optional(),
  digestFocusDeliverZalo: z.boolean().optional(),
  digestFocusDeliverEmail: z.boolean().optional(),
  digestFocusTopics: z.string().optional()
});

/**
 * @typedef UISealToken - Branded type for UI security validation via TypeScript 5.x
 */
type UISealToken = string & { __brand: "UISealToken" };

/**
 * @typedef UIScaledState - Branded type representing the validated internal state of the UI Controller
 */
type UIScaledState = { __brand: "UIScaledState" };

/**
 * UIController — Multi-Client WebSocket Hub
 * ==========================================
 * Refactored from single-client (this.uiClient) to Set<WebSocket> to support
 * both Widget + Dashboard windows connecting simultaneously.
 * 
 * Also serves as SSOT (Single Source of Truth) for liva-config.json:
 * - Dashboard sends `update_config` → Gateway writes file → broadcasts to all clients.
 * - Widget/Dashboard send `get_config` → Gateway reads file → responds.
 */
export class UIController extends EventEmitter {
  private readonly wss: WebSocketServer;

  /** Multi-client connection pool (Widget + Dashboard + future clients) */
  private clients: Set<WebSocket> = new Set();

  #internalSealToken: UISealToken | null = null;
  #validatedState: UIScaledState | null = null;

  /** Path to shared config file (SSOT) */
  #configPath: string;

  /** Path to persisted user profile */
  #profilePath: string;

  /** File Explorer for mobile app */
  private fileExplorer: FileExplorer;

  constructor(port: number = 8082) { // port param is ignored, we use args
    super();
    this.#configPath = path.join(process.cwd(), "..", "data", "liva-config.json");
    this.#profilePath = path.join(process.cwd(), "..", "data", "user_profile.json");
    this.fileExplorer = new FileExplorer();

    const appConfig = AppConfig.get();
    const isDev = appConfig.IS_DEV;
    const wsPort = appConfig.GATEWAY_WS_PORT; // Dynamic port for Sidecar
    const host = "127.0.0.1"; // [Phase 5.1] Strict Binding: Zero-Trust Firewall (Reject LAN scans)
    const authToken = isDev ? null : randomUUID();

    // ─── [Phase 5.1] Dead-Man Switch (Time-Bomb) ───
    let timeBomb: NodeJS.Timeout | null = null;
    
    const armTimeBomb = () => {
      if (timeBomb) clearTimeout(timeBomb);
      timeBomb = setTimeout(() => {
        logger.error("💀 [Dead-Man Switch] UI không kết nối trong 10s. Tự sát để xả VRAM!");
        process.exit(0); // [v26] process.exit(0) as requested to avoid crash logs on intentional exit
      }, 10000);
    };

    const defuseTimeBomb = () => {
      if (timeBomb) {
        clearTimeout(timeBomb);
        timeBomb = null;
        logger.info("🛡️ [Security] Gỡ bom Time-Bomb thành công. Client UI hợp lệ đã kết nối.");
      }
    };

    // Arm it initially
    if (!isDev) armTimeBomb();

    this.wss = new WebSocketServer({ port: wsPort, host }, () => {
      const address = this.wss.address() as AddressInfo;
      const actualPort = address.port;

      if (isDev) {
        logger.info(`📡 [WebSocket] Chế độ DEV: Máy chủ mở tại cổng ${actualPort}`);
      } else {
        // [DYNAMIC HANDSHAKE] In ra stdout đúng 1 dòng để Tauri bắt
        const handshake = JSON.stringify({
          event: "GATEWAY_READY",
          port: actualPort,
          token: authToken
        });
        process.stdout.write(handshake + "\n");
        // Tuyệt đối không log thêm ở stdout!
      }
    });

    this.wss.on("connection", (ws, req) => {
      // ─── Token Authentication (Sidecar Mode) ───
      if (!isDev) {
        const url = new URL(req.url || "", `http://127.0.0.1`);
        if (url.searchParams.get("token") !== authToken) {
          logger.error("❌ [Security] Từ chối kết nối WebSocket do sai Token!");
          ws.close(1008, "Invalid Token");
          return;
        }
        
        // Defuse the bomb!
        defuseTimeBomb();
      }

      // ─── Add to multi-client pool ───
      this.clients.add(ws);
      logger.info(`🔗 [WebSocket] Client kết nối! Tổng: ${this.clients.size} clients`);

      // Initialize security tokens upon connection
      this.#internalSealToken = "SECURITY_SESSION_INIT" as UISealToken;
      this.#validatedState = {} as unknown as UIScaledState;

      ws.on("message", async (message, isBinary) => {
        // 1. Validate presence of Seal Token before processing any interaction
        if (!this.#internalSealToken || !this.#validatedState) {
          logger.error("[Security] ❌ Không tìm thấy UI Seal Token hoặc Validated State!");
          return;
        }

        if (isBinary) {
          // Emit audio_input for VAD pipeline - NO DEBUG LOG (prevents I/O flooding)
          this.emit("audio_input", message as Buffer);
          return;
        }

        const rawData = message.toString();

        // Zero-Latency Preemption (Barge-in / Ngắt lời)
        if (rawData.includes("[INTERRUPT]")) {
          logger.warn(`[WebSocket] 🛑 Giao diện yêu cầu NGẮT LỜI KHẨN CẤP!`);
          this.emit("interrupt");
          return;
        }

        logger.debug(`📥 RAW Message from UI: ${rawData}`);
        try {
          const data = JSON.parse(rawData);

          // ─── Existing: User voice/text command ───
          if (data.event === "user_voice_command") {
            const userText = data.payload.text;
            logger.info(`[Nhận Lệnh] Anh Dương vừa nói/gõ: ${userText}`);
            this.emit("user_input", userText);
          }
          else if (data.event === "get_ai_config") {
            this.#handleGetAIConfig(ws);
          }
          else if (data.event === "update_ai_config") {
            this.#handleUpdateAIConfig(ws, data.payload);
          }
          else if (data.event === "test_ai_connection") {
            this.emit("test_ai_connection", ws, data.payload);
          }
          else if (data.event === "get_voice_status") {
            this.emit("get_voice_status", ws);
          }
          else if (data.event === "get_voice_profiles") {
            this.emit("get_voice_profiles", ws);
          }
          else if (data.event === "select_voice_profile") {
            this.emit("select_voice_profile", ws, data.payload);
          }
          else if (data.event === "start_voice_training") {
            this.emit("start_voice_training", ws, data.payload);
          }
          else if (data.event === "stop_voice_training") {
            this.emit("stop_voice_training", ws);
          }

          // ─── NEW: Config SSOT Events ───
          else if (data.event === "get_config") {
            this.#handleGetConfig(ws);
          }
          else if (data.event === "update_config") {
            this.#handleUpdateConfig(ws, data.payload);
          }
          else if (data.event === "get_avatar_models") {
            await this.#handleGetAvatarModels(ws);
          }

          // ─── NEW: Skills list ───
          else if (data.event === "get_skills_list") {
            this.emit("get_skills_list", ws);
          }
          else if (data.event === "toggle_skill") {
            this.emit("toggle_skill", ws, data.payload);
          }
          else if (data.event === "toggle_all_skills") {
            this.emit("toggle_all_skills", ws, data.payload);
          }

          // ─── NEW: System status ───
          else if (data.event === "get_system_status") {
            this.emit("get_system_status", ws);
          }
          else if (data.event === "force_gc") {
            this.emit("force_gc", ws);
          }
          else if (data.event === "trigger_gitnexus_index") {
            this.emit("trigger_gitnexus_index", ws);
          }
          else if (data.event === "reload_skills") {
            this.emit("reload_skills", ws);
          }
          else if (data.event === "check_skill") {
            this.emit("test_skill", ws, data.payload);
          }
          else if (data.event === "test_skill") {
            this.emit("test_skill", ws, data.payload);
          }
          else if (data.event === "consolidate_memory") {
            this.emit("consolidate_memory", ws, data.payload);
          }

          // ─── NEW: User Profile (Onboarding) ───
          else if (data.event === "get_user_profile") {
            await this.#handleGetUserProfile(ws);
            this.emit("get_user_profile", ws);
          }
          else if (data.event === "update_user_profile") {
            await this.#handleUpdateUserProfile(ws, data.payload);
            this.emit("update_user_profile", ws, data.payload);
          }

          // ─── NEW: Camera Vision (webcam frame for AI) ───
          else if (data.event === "camera_frame") {
            this.emit("camera_frame", data.payload);
          }

          // ─── NEW: Task Manager Events ───
          else if (data.event === "get_tasks") {
            this.emit("get_tasks", ws);
          }
          else if (data.event === "add_task") {
            this.emit("add_task", ws, data.payload);
          }
          else if (data.event === "update_task") {
            this.emit("update_task", ws, data.payload);
          }
          else if (data.event === "delete_task") {
            this.emit("delete_task", ws, data.payload);
          }
          else if (data.event === "execute_task") {
            this.emit("execute_task", ws, data.payload);
          }
          else if (data.event === "task_plan_chat") {
            this.emit("task_plan_chat", ws, data.payload);
          }

          // ─── NEW: Memory Viewer Events ───
          else if (data.event === "get_memory_data") {
            this.emit("get_memory_data", ws);
          }
          else if (data.event === "delete_memory_fact") {
            this.emit("delete_memory_fact", ws, data.payload);
          }


          // ─── NEW: File Explorer ───
          else if (data.event === "explorer_ls") {
            try {
              const files = await this.fileExplorer.listDirectory(data.payload.path);
              this.#sendToClient(ws, "explorer_ls_result", { path: data.payload.path, files });
            } catch (e: unknown) {
              const errMsg = e instanceof Error ? e.message : String(e);
              this.#sendToClient(ws, "explorer_error", { error: errMsg });
            }
          }
          else if (data.event === "explorer_cat") {
            try {
              const content = await this.fileExplorer.readFile(data.payload.path);
              this.#sendToClient(ws, "explorer_cat_result", { path: data.payload.path, content });
            } catch (e: unknown) {
              const errMsg = e instanceof Error ? e.message : String(e);
              this.#sendToClient(ws, "explorer_error", { error: errMsg });
            }
          }

          // ─── Ping/Pong ───
          else if (data.event === "ping") {
            this.#sendToClient(ws, "pong", {});
          }

          // ─── [v25 Pillar 4] Wake Word Triggered (from Frontend ONNX WASM) ───
          // Frontend detects wake word locally via ONNX model, then sends this event
          else if (data.event === "wake_word_triggered") {
            logger.info(`[WebSocket] Wake word triggered from frontend (ONNX WASM)`);
            this.emit("wake_word_triggered");
          }



          // ─── [P5] Memory Reset ───
          else if (data.event === "reset_memory") {
            this.emit("reset_memory", ws);
          }

          // ─── NEW: Environment & Vault Management ───
          else if (data.event === "get_env_config") {
            await this.#handleGetEnvConfig(ws);
          }
          else if (data.event === "save_env_config") {
            await this.#handleSaveEnvConfig(ws, data.payload);
          }
          else if (data.event === "restart_gateway") {
            await this.restart();
          }

        } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          logger.error(`[WebSocket] ❌ Lỗi parse JSON từ UI: ${errMsg}`);
        }
      });

      ws.on("close", () => {
        this.clients.delete(ws);
        logger.info(`❌ [WebSocket] Client ngắt kết nối. Còn lại: ${this.clients.size} clients`);

        // Only reset tokens if NO clients remain
        if (this.clients.size === 0) {
          this.#internalSealToken = null;
          this.#validatedState = null;
          if (!isDev) armTimeBomb(); // Re-arm the bomb when the last client disconnects
        }
      });
    });
  }

  // ═══════════════════════════════════════════════════════
  //  Multi-Client Broadcast (thay thế single-client send)
  // ═══════════════════════════════════════════════════════

  public get connectedClientCount(): number {
    return this.clients.size;
  }

  public broadcastUIEvent(event: string, payload: Record<string, unknown> = {}) {
    if (!this.#validatedState) {
      logger.error("[Security] ❌ Không thể broadcast: Controller ở trạng thái không xác thực!");
      return;
    }

    const message = JSON.stringify({ event, payload });
    this.clients.forEach(client => {
      if (client.readyState === WebSocket.OPEN) {
        client.send(message);
      }
    });
  }

  public broadcastAudioChunk(buffer: Buffer) {
    if (!this.#internalSealToken) {
      logger.error("[Security] ❌ Không thể broadcast audio: Thiếu Seal Token!");
      return;
    }

    this.clients.forEach(client => {
      if (client.readyState === WebSocket.OPEN) {
        client.send(buffer, { binary: true });
      }
    });
  }

  // ═══════════════════════════════════════════════════════
  //  Send to specific client (for request-response patterns)
  // ═══════════════════════════════════════════════════════

  #sendToClient(ws: WebSocket, event: string, payload: Record<string, unknown>) {
    if (ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ event, payload }));
    }
  }

  /**
   * Send skills list to specific client (called from CoreKernel)
   */
  public sendSkillsList(ws: WebSocket, skills: Record<string, unknown>[]) {
    this.#sendToClient(ws, "skills_list", { skills });
  }

  /**
   * Send system status to specific client (called from CoreKernel)
   */
  public sendSystemStatus(ws: WebSocket, status: Record<string, unknown>) {
    this.#sendToClient(ws, "system_status", { ...status });
  }

  /**
   * Send user profile to specific client (called from CoreKernel)
   */
  public sendUserProfile(ws: WebSocket, profile: Record<string, unknown>) {
    this.#sendToClient(ws, "user_profile", profile);
  }

  /**
   * Send tasks list to specific client (called from CoreKernel)
   */
  public sendTasksList(ws: WebSocket, tasks: Record<string, unknown>[]) {
    this.#sendToClient(ws, "tasks_list", { tasks });
  }

  /**
   * Send memory data to specific client (called from CoreKernel)
   */
  public sendMemoryData(ws: WebSocket, data: Record<string, unknown>) {
    this.#sendToClient(ws, "memory_data", data);
  }

  public sendAIConfig(ws: WebSocket, ai: Record<string, unknown>) {
    this.#sendToClient(ws, "ai_config", { ai });
  }

  public sendVoiceStatus(ws: WebSocket, voice: Record<string, unknown>) {
    this.#sendToClient(ws, "voice_status", { voice });
  }

  public sendVoiceProfiles(ws: WebSocket, profiles: Record<string, unknown>[]) {
    this.#sendToClient(ws, "voice_profiles", { profiles });
  }

  // ═══════════════════════════════════════════════════════
  //  User Profile SSOT — Gateway owns the profile file
  // ═══════════════════════════════════════════════════════

  async #handleGetUserProfile(ws: WebSocket) {
    try {
      const raw = await fsp.readFile(this.#profilePath, "utf8");
      const profile = JSON.parse(raw);
      if (profile && typeof profile === "object" && !Array.isArray(profile)) {
        this.#sendToClient(ws, "user_profile", profile as Record<string, unknown>);
        logger.info("[Profile] 📤 Đã gửi user profile đã lưu cho client");
        return;
      }
      throw new Error("Invalid user profile payload");
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.warn(`[Profile] ⚠️ Không đọc được user profile: ${errMsg}`);
      this.#sendToClient(ws, "user_profile", {});
    }
  }

  async #handleUpdateUserProfile(ws: WebSocket, profile: Record<string, unknown>) {
    try {
      const sanitized = profile && typeof profile === "object" && !Array.isArray(profile) ? profile : {};
      const tmpPath = `${this.#profilePath}.tmp`;
      await fsp.writeFile(tmpPath, JSON.stringify(sanitized, null, 2), "utf8");
      await safeRename(tmpPath, this.#profilePath);
      logger.info("[Profile] 💾 User profile đã được cập nhật và lưu thành công");
      this.#sendToClient(ws, "profile_updated_success", sanitized);
      this.broadcastUIEvent("profile_updated_success", sanitized);
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[Profile] ❌ Lỗi cập nhật user profile: ${errMsg}`);
      this.#sendToClient(ws, "profile_update_error", { error: errMsg });
    }
  }

  // ═══════════════════════════════════════════════════════
  //  Config SSOT — Gateway owns the config file
  // ═══════════════════════════════════════════════════════

  async #handleGetAvatarModels(ws: WebSocket) {
    try {
      const modelsRoot = path.join(process.cwd(), "..", "liva-ui", "public", "models");
      const models3d = await this.#scanAvatarModels(
        path.join(modelsRoot, "vrm"),
        [".vrm", ".fbx"],
        "3d",
      );
      const models2d = await this.#scanAvatarModels(
        path.join(modelsRoot, "live2d"),
        [".json"],
        "2d",
      );
      this.#sendToClient(ws, "avatar_models_list", { models3d, models2d });
      logger.info(`[Config] 📤 Avatar models: ${models3d.length} 3D, ${models2d.length} 2D`);
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.warn(`[Config] ⚠️ Không quét được avatar models: ${errMsg}`);
      this.#sendToClient(ws, "avatar_models_list", { models3d: [], models2d: [] });
    }
  }

  async #scanAvatarModels(
    rootDir: string,
    extensions: string[],
    type: "2d" | "3d",
  ): Promise<Array<Record<string, unknown>>> {
    const results: Array<Record<string, unknown>> = [];
    const seen = new Set<string>();

    const walk = async (currentDir: string, relPrefix: string): Promise<void> => {
      let entries: import("node:fs").Dirent[];
      try {
        entries = await fsp.readdir(currentDir, { withFileTypes: true });
      } catch {
        return;
      }

      for (const ent of entries) {
        if (ent.name.startsWith(".") || ent.name.endsWith(".fbm")) continue;

        const fullPath = path.join(currentDir, ent.name);
        if (ent.isDirectory()) {
          const nextPrefix = relPrefix ? `${relPrefix}/${ent.name}` : ent.name;
          await walk(fullPath, nextPrefix);
          continue;
        }

        const ext = path.extname(ent.name).toLowerCase();
        if (!extensions.includes(ext)) continue;

        const relPath = (relPrefix ? `${relPrefix}/${ent.name}` : ent.name).replace(/\\/g, "/");
        if (seen.has(relPath)) continue;
        seen.add(relPath);

        const stat = await fsp.stat(fullPath);
        const stem = relPrefix || ent.name.replace(/\.[^.]+$/, "");
        const format = ext === ".vrm" ? "vrm" : ext === ".fbx" ? "fbx" : "live2d";
        const displayName = stem
          .split(/[/\\]/)
          .pop()!
          .replace(/[-_]/g, " ")
          .replace(/\b\w/g, (c) => c.toUpperCase());
        const hasTextureDir = ext === ".fbx" ? await this.#hasSiblingFolder(fullPath, ".fbm") : false;

        results.push({
          name: displayName,
          filename: relPath,
          size: this.#formatBytes(stat.size),
          type,
          format,
          isActive: false,
          hasTextureDir,
        });
      }
    };

    await walk(rootDir, "");
    results.sort((a, b) => String(a.name).localeCompare(String(b.name)));
    return results;
  }

  async #hasSiblingFolder(filePath: string, suffix: string): Promise<boolean> {
    try {
      const dir = path.dirname(filePath);
      const base = path.basename(filePath, path.extname(filePath));
      const sibling = path.join(dir, `${base}${suffix}`);
      const stat = await fsp.stat(sibling);
      return stat.isDirectory();
    } catch {
      return false;
    }
  }

  #formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `~${(bytes / 1024).toFixed(1)} KB`;
    return `~${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  async #handleGetConfig(ws: WebSocket) {
    try {
      const raw = await fsp.readFile(this.#configPath, "utf8");
      const config = JSON.parse(raw);
      this.#sendToClient(ws, "config_data", config);
      this.#sendToClient(ws, "ai_config", { ai: config.ai ?? {} });
      logger.info("[Config] 📤 Đã gửi config cho client");
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.warn(`[Config] ⚠️ Không đọc được config: ${errMsg}`);
      const fallback = this.#getDefaultConfig();
      this.#sendToClient(ws, "config_data", fallback);
      this.#sendToClient(ws, "ai_config", { ai: fallback.ai });
    }
  }

  async #handleGetAIConfig(ws: WebSocket) {
    try {
      const raw = await fsp.readFile(this.#configPath, "utf8");
      const config = JSON.parse(raw);
      this.#sendToClient(ws, "ai_config", { ai: config.ai ?? this.#getDefaultConfig().ai });
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.warn(`[Config] ⚠️ Không đọc được AI config: ${errMsg}`);
      this.#sendToClient(ws, "ai_config", { ai: this.#getDefaultConfig().ai });
    }
  }

  async #handleUpdateAIConfig(ws: WebSocket, partialAI: Record<string, unknown>) {
    try {
      let currentConfig: Record<string, unknown>;
      try {
        const raw = await fsp.readFile(this.#configPath, "utf8");
        currentConfig = JSON.parse(raw);
      } catch {
        currentConfig = this.#getDefaultConfig();
      }

      currentConfig.ai = { ...(currentConfig.ai as Record<string, unknown>), ...partialAI };

      const tmpPath = `${this.#configPath}.tmp`;
      await fsp.writeFile(tmpPath, JSON.stringify(currentConfig, null, 2), "utf8");
      await safeRename(tmpPath, this.#configPath);
      logger.info("[Config] 💾 AI config đã được cập nhật và lưu thành công");
      this.broadcastUIEvent("config_updated", currentConfig);
      this.emit("config_updated", currentConfig);
      this.#sendToClient(ws, "ai_config_updated", { ai: currentConfig.ai });
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[Config] ❌ Lỗi cập nhật AI config: ${errMsg}`);
      this.#sendToClient(ws, "config_error", { error: errMsg });
    }
  }

  async #handleUpdateConfig(ws: WebSocket, partialConfig: Record<string, unknown>) {
    try {
      // 1. Read current config
      let currentConfig: Record<string, unknown>;
      try {
        const raw = await fsp.readFile(this.#configPath, "utf8");
        currentConfig = JSON.parse(raw);
      } catch {
        currentConfig = this.#getDefaultConfig();
      }

      // 2. Deep merge (shallow merge per section)
      if (partialConfig.avatar) {
        currentConfig.avatar = { ...(currentConfig.avatar as Record<string, unknown>), ...(partialConfig.avatar as Record<string, unknown>) };
      }
      if (partialConfig.ai) {
        currentConfig.ai = { ...(currentConfig.ai as Record<string, unknown>), ...(partialConfig.ai as Record<string, unknown>) };
      }
      if (partialConfig.ui) {
        currentConfig.ui = { ...(currentConfig.ui as Record<string, unknown>), ...(partialConfig.ui as Record<string, unknown>) };
      }

      // Xử lý System Config an toàn bằng Zod
      const sysData = partialConfig.system;
      if (sysData) {
        const parsed = SystemConfigSchema.safeParse(sysData);
        if (parsed.success) {
          currentConfig.system = { ...(currentConfig.system as Record<string, unknown>), ...parsed.data };
        } else {
          logger.warn(`[Config] Payload cấu hình hệ thống không hợp lệ bị từ chối: ${JSON.stringify(parsed.error.issues)}`);
        }
      }

      // 3. Atomic Write: .tmp + rename() prevents corrupt config on crash
      const tmpPath = `${this.#configPath}.tmp`;
      await fsp.writeFile(tmpPath, JSON.stringify(currentConfig, null, 2), "utf8");
      await safeRename(tmpPath, this.#configPath);
      logger.info("[Config] 💾 Config đã được cập nhật và lưu thành công");

      // 4. Broadcast to ALL clients (Widget + Dashboard) and emit internally
      this.broadcastUIEvent("config_updated", currentConfig);
      this.emit("config_updated", currentConfig);

    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[Config] ❌ Lỗi cập nhật config: ${errMsg}`);
      this.#sendToClient(ws, "config_error", { error: errMsg });
    }
  }

  #getDefaultConfig() {
    return {
      avatar: {
        engineMode: "3D",
        live2dModel: "models/live2d/pio/index.json",
        vrmModel: "models/vrm/default_avatar/tripo_convert_648e4371-4299-44d8-94d8-e6a63e0e07a3.fbx",
        autoBlinkEnabled: true,
        lookAtMouseEnabled: true,
        lipSyncEnabled: true,
      },
      ai: {
        provider: "local",
        cloudBaseUrl: "",
        cloudApiKey: "",
        cloudModel: "",
        localModelsDir: "E:\\AI_Models",
        routerModel: "gemma-4-E4B-it-Q4_K_M.gguf",
        expertModel: "",
        temperature: 0.7,
        maxTokens: 4096,
        topP: 0.9,
      },
      voice: {
        enabled: true,
        provider: "hybrid",
        activeProfile: "default",
        trainingEnabled: false,
        sampleRate: 16000,
        language: "vi-VN"
      },
      ui: {
        widgetPosition: "bottom-right",
        dashboardTheme: "dark",
      },
      system: {
        geolocationEnabled: false,
        proactiveEnabled: true,
        proactiveHour: 7,
        proactiveMinute: 0,
        proactiveDeliverUI: true,
        proactiveDeliverTelegram: true,
        proactiveDeliverZalo: false,
        proactiveDeliverEmail: false,
        proactiveFocus: "",
        digestInterestsEnabled: false,
        digestInterestsHour: 7,
        digestInterestsMinute: 0,
        digestInterestsDeliverUI: true,
        digestInterestsDeliverTelegram: true,
        digestInterestsDeliverZalo: false,
        digestInterestsDeliverEmail: false,

        digestFocusEnabled: false,
        digestFocusHour: 8,
        digestFocusMinute: 0,
        digestFocusDeliverUI: true,
        digestFocusDeliverTelegram: true,
        digestFocusDeliverZalo: false,
        digestFocusDeliverEmail: false,
        digestFocusTopics: ""
      }
    };
  }

  #getEnvPath(): string {
    const cwd = process.cwd();
    if (syncFs.existsSync(path.join(cwd, "liva-gateway"))) {
      return path.join(cwd, "liva-gateway", ".env");
    }
    return path.join(cwd, ".env");
  }

  async #handleGetEnvConfig(ws: WebSocket) {
    let envContent = "";
    try {
      const envPath = this.#getEnvPath();
      if (syncFs.existsSync(envPath)) {
        envContent = await fsp.readFile(envPath, "utf8");
      }
    } catch (e) {
      logger.error(`[EnvConfig] Lỗi đọc .env: ${e}`);
    }

    let vaultData: Record<string, string> = {};
    try {
      let vaultPath = process.env.LIVA_VAULT_PATH;
      if (!vaultPath) {
        const cwd = process.cwd();
        const path1 = path.join(cwd, "data", "liva_vault.json");
        const path2 = path.join(cwd, "..", "data", "liva_vault.json");
        if (syncFs.existsSync(path1)) {
          vaultPath = path1;
        } else if (syncFs.existsSync(path2)) {
          vaultPath = path2;
        } else {
          vaultPath = path1;
        }
      }
      if (syncFs.existsSync(vaultPath)) {
        const rawVault = await fsp.readFile(vaultPath, "utf8");
        const encryptedVault = JSON.parse(rawVault);
        const { EncryptionEngine } = await import("../memory/EncryptionEngine");
        for (const [key, encVal] of Object.entries(encryptedVault)) {
          if (typeof encVal === "string" && encVal) {
            vaultData[key] = EncryptionEngine.decrypt(encVal);
          }
        }
      }
    } catch (e) {
      logger.error(`[EnvConfig] Lỗi đọc vault: ${e}`);
    }

    this.#sendToClient(ws, "env_config_data", {
      content: envContent,
      vault: vaultData
    });
  }

  async #handleSaveEnvConfig(ws: WebSocket, payload: any) {
    let envContent = payload?.content || "";
    const sensitiveKeys = [
      "EMAIL_HOST",
      "EMAIL_USER",
      "EMAIL_PASS",
      "TAVILY_API_KEY",
      "TELEGRAM_BOT_TOKEN",
      "TELEGRAM_ALLOWED_IDS",
      "ZALO_APP_ID",
      "ZALO_APP_SECRET",
      "GOOGLE_CLIENT_SECRET"
    ];

    const extractEnvField = (content: string, key: string): string => {
      const match = content.match(new RegExp(`^${key}=(.*)$`, 'm'));
      return match ? match[1].trim() : '';
    };

    const removeEnvField = (content: string, key: string): string => {
      const regex = new RegExp(`^${key}=.*$`, 'm');
      return content.replace(regex, `${key}=`);
    };

    const { EncryptionEngine } = await import("../memory/EncryptionEngine");

    let vaultPath = process.env.LIVA_VAULT_PATH;
    if (!vaultPath) {
      const cwd = process.cwd();
      const path1 = path.join(cwd, "data", "liva_vault.json");
      const path2 = path.join(cwd, "..", "data", "liva_vault.json");
      if (syncFs.existsSync(path1)) {
        vaultPath = path1;
      } else if (syncFs.existsSync(path2)) {
        vaultPath = path2;
      } else {
        vaultPath = path1;
      }
    }

    let existingVault: Record<string, string> = {};
    if (syncFs.existsSync(vaultPath)) {
      try {
        const rawVault = await fsp.readFile(vaultPath, "utf8");
        existingVault = JSON.parse(rawVault);
      } catch (e) {
        logger.error(`[Vault] Lỗi đọc vault hiện tại: ${e}`);
      }
    }

    for (const key of sensitiveKeys) {
      const val = extractEnvField(envContent, key);
      if (val) {
        existingVault[key] = EncryptionEngine.encrypt(val);
        envContent = removeEnvField(envContent, key);
      } else {
        delete existingVault[key];
        envContent = removeEnvField(envContent, key);
      }
    }

    try {
      const vaultDir = path.dirname(vaultPath);
      if (!syncFs.existsSync(vaultDir)) {
        await fsp.mkdir(vaultDir, { recursive: true });
      }
      const tmpVaultPath = `${vaultPath}.tmp`;
      await fsp.writeFile(tmpVaultPath, JSON.stringify(existingVault, null, 2), "utf8");
      await safeRename(tmpVaultPath, vaultPath);
      logger.info(`[Vault] 🛡️ Đã mã hóa bảo mật các key nhạy cảm vào ${vaultPath}`);
    } catch (e) {
      logger.error(`[Vault] Lỗi ghi vault: ${e}`);
    }

    try {
      const envPath = this.#getEnvPath();
      const envDir = path.dirname(envPath);
      if (!syncFs.existsSync(envDir)) {
        await fsp.mkdir(envDir, { recursive: true });
      }
      const tmpEnvPath = `${envPath}.tmp`;
      await fsp.writeFile(tmpEnvPath, envContent, "utf8");
      await safeRename(tmpEnvPath, envPath);
      logger.info(`[EnvConfig] 💾 Đã lưu cấu hình .env thành công`);
    } catch (e) {
      logger.error(`[EnvConfig] Lỗi ghi .env: ${e}`);
    }

    // Broadcast update warning to all clients
    this.broadcastUIEvent("system_notification", { 
      message: "🔄 Đang tự động khởi động lại LIVA Gateway để áp dụng tích hợp mới...",
      freezeUI: true 
    });

    // Wait 1 second before doing the actual restart so client can receive and show the message
    setTimeout(() => {
      this.restart();
    }, 1000);
  }

  public async restart() {
    logger.warn("🔄 [Gateway] Bắt đầu tiến trình TÁI KHỞI ĐỘNG (Restart Gateway)...");
    
    // Close WebSocket server
    this.wss.close();
    
    // Shutdown kernel
    if ((globalThis as any).kernelInstance) {
      await (globalThis as any).kernelInstance.shutdown();
    }
    
    // Wait 800ms for ports to be completely free
    setTimeout(async () => {
      try {
        const { spawn } = await import("child_process");
        const cwd = process.cwd();
        let targetCwd = cwd;
        let cmd = "npm.cmd";
        let args = ["run", "dev", "-w", "liva-gateway"];
        
        if (syncFs.existsSync(path.join(cwd, "..", "package.json"))) {
          targetCwd = path.join(cwd, "..");
        }
        
        logger.info(`[Gateway] Spawning new process: ${cmd} ${args.join(" ")} in ${targetCwd}`);
        
        const child = spawn(cmd, args, {
          cwd: targetCwd,
          detached: true,
          stdio: "ignore",
          shell: true
        });
        child.unref();
      } catch (err) {
        logger.error(`[Gateway] Lỗi spawn tiến trình mới: ${err}`);
      } finally {
        process.exit(0);
      }
    }, 800);
  }
}
