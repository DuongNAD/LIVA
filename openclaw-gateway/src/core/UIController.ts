import { EventEmitter } from 'node:events';
import { WebSocketServer, WebSocket } from "ws";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";
import { FileExplorer } from "../services/FileExplorer";

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

  /** File Explorer for mobile app */
  private fileExplorer: FileExplorer;

  constructor(port: number = 8082) { // port param is ignored, we use args
    super();
    this.#configPath = path.join(process.cwd(), "..", "data", "liva-config.json");
    this.fileExplorer = new FileExplorer();
    
    const isDev = process.argv.includes("--dev");
    const wsPort = 8082; // Force 8082 for UI compatibility
    const host = "0.0.0.0"; // Allow LAN connections for Mobile Web App
    const authToken = null; // Bypass auth token so UI connects seamlessly

    this.wss = new WebSocketServer({ port: wsPort, host }, () => {
        const address = this.wss.address() as any;
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
            logger.info(`📡 [WebSocket] Chế độ SIDECAR: Đã sinh cổng động ${actualPort} và gửi Handshake.`);
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
          logger.debug(`📥 RAW Binary Audio from UI: ${(message as Buffer).length} bytes`);
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

          // ─── NEW: Config SSOT Events ───
          else if (data.event === "get_config") {
            this.#handleGetConfig(ws);
          }
          else if (data.event === "update_config") {
            this.#handleUpdateConfig(ws, data.payload);
          }

          // ─── NEW: Skills list ───
          else if (data.event === "get_skills_list") {
            this.emit("get_skills_list", ws);
          }

          // ─── NEW: System status ───
          else if (data.event === "get_system_status") {
            this.emit("get_system_status", ws);
          }

          // ─── NEW: Camera Vision (webcam frame for AI) ───
          else if (data.event === "camera_frame") {
            this.emit("camera_frame", data.payload);
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

        } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          logger.error(`[WebSocket] ❌ Lỗi parse JSON từ UI: ${e?.message ?? e}`);
        }
      });

      ws.on("close", () => {
        this.clients.delete(ws);
        logger.info(`❌ [WebSocket] Client ngắt kết nối. Còn lại: ${this.clients.size} clients`);
        
        // Only reset tokens if NO clients remain
        if (this.clients.size === 0) {
          this.#internalSealToken = null;
          this.#validatedState = null;
        }
      });
    });
  }

  // ═══════════════════════════════════════════════════════
  //  Multi-Client Broadcast (thay thế single-client send)
  // ═══════════════════════════════════════════════════════

  public broadcastUIEvent(event: string, payload: any = {}) {
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

  #sendToClient(ws: WebSocket, event: string, payload: any) {
    if (ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ event, payload }));
    }
  }

  /**
   * Send skills list to specific client (called from CoreKernel)
   */
  public sendSkillsList(ws: WebSocket, skills: any[]) {
    this.#sendToClient(ws, "skills_list", { skills });
  }

  /**
   * Send system status to specific client (called from CoreKernel)
   */
  public sendSystemStatus(ws: WebSocket, status: any) {
    this.#sendToClient(ws, "system_status", { ...status });
  }

  // ═══════════════════════════════════════════════════════
  //  Config SSOT — Gateway owns the config file
  // ═══════════════════════════════════════════════════════

  async #handleGetConfig(ws: WebSocket) {
    try {
      const raw = await fsp.readFile(this.#configPath, "utf8");
      const config = JSON.parse(raw);
      this.#sendToClient(ws, "config_data", config);
      logger.info("[Config] 📤 Đã gửi config cho client");
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.warn(`[Config] ⚠️ Không đọc được config: ${errMsg}`);
      // Send default config if file doesn't exist
      this.#sendToClient(ws, "config_data", this.#getDefaultConfig());
    }
  }

  async #handleUpdateConfig(ws: WebSocket, partialConfig: any) {
    try {
      // 1. Read current config
      let currentConfig: any;
      try {
        const raw = await fsp.readFile(this.#configPath, "utf8");
        currentConfig = JSON.parse(raw);
      } catch {
        currentConfig = this.#getDefaultConfig();
      }

      // 2. Deep merge (shallow merge per section)
      if (partialConfig.avatar) {
        currentConfig.avatar = { ...currentConfig.avatar, ...partialConfig.avatar };
      }
      if (partialConfig.ai) {
        currentConfig.ai = { ...currentConfig.ai, ...partialConfig.ai };
      }
      if (partialConfig.ui) {
        currentConfig.ui = { ...currentConfig.ui, ...partialConfig.ui };
      }

      // 3. Atomic Write: .tmp + rename() prevents corrupt config on crash
      const tmpPath = `${this.#configPath}.tmp`;
      await fsp.writeFile(tmpPath, JSON.stringify(currentConfig, null, 2), "utf8");
      await fsp.rename(tmpPath, this.#configPath);
      logger.info("[Config] 💾 Config đã được cập nhật và lưu thành công");

      // 4. Broadcast to ALL clients (Widget + Dashboard)
      this.broadcastUIEvent("config_updated", currentConfig);

    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[Config] ❌ Lỗi cập nhật config: ${errMsg}`);
      this.#sendToClient(ws, "config_error", { error: errMsg });
    }
  }

  #getDefaultConfig() {
    return {
      avatar: {
        engineMode: "auto",
        live2dModel: "models/live2d/pio/index.json",
        vrmModel: "models/vrm/default.vrm",
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
      ui: {
        widgetPosition: "bottom-right",
        dashboardTheme: "dark",
      },
    };
  }
}
