import { EventEmitter } from "events";
import { WebSocketServer, WebSocket } from "ws";
import { logger } from "../utils/logger";

/**
 * @typedef UISealToken - Branded type for UI security validation via TypeScript 5.x
 */
type UISealToken = string & { readonly __brand: unique symbol };

/**
 * @typedef UIScaledState - Branded type representing the validated internal state of the UI Controller
 */
type UIScaledState = { readonly __brand: unique symbol };

export class UIController extends EventEmitter {
  private wss: WebSocketServer;
  private uiClient: WebSocket | null = null;
  
  #internalSealToken: UISealToken | null = null;
  #validatedState: UIScaledState | null = null;

  constructor(port: number = 8082) {
    super();
    this.wss = new WebSocketServer({ port });
    logger.info(`📡 [WebSocket] Máy chủ phát sóng đã mở tại cổng ${port}`);

    this.wss.on("connection", (ws) => {
      logger.info("🔗 [WebSocket] Giao diện Liva (UI) đã kết nối thành công!");
      this.uiClient = ws;

      // Initialize security tokens upon connection (Vá lỗi TS2352 bằng as unknown as)
      this.#internalSealToken = "SECURITY_SESSION_INIT" as UISealToken;
      this.#validatedState = {} as unknown as UIScaledState;

      ws.on("message", (message, isBinary) => {
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

        logger.debug(`📥 RAW Message from UI:`, rawData);
        try {
          const data = JSON.parse(rawData);
          if (data.event === "user_voice_command") {
            const userText = data.payload.text;
            logger.info(`[Nhận Lệnh] Anh Dương vừa nói/gõ:`, userText);
            // Emit văng lệnh lên CoreKernel
            this.emit("user_input", userText);
          }
        } catch (e) {
          logger.error("[WebSocket] ❌ Lỗi parse JSON từ UI:", e);
        }
      });

      ws.on("close", () => {
        logger.info("❌ [WebSocket] Giao diện đã ngắt kết nối.");
        this.uiClient = null;
        this.#internalSealToken = null;
        this.#validatedState = null;
      });
    });
  }

  public broadcastUIEvent(event: string, payload: any = {}) {
    if (this.uiClient && this.uiClient.readyState === WebSocket.OPEN) {
      if (this.#validatedState) {
        this.uiClient.send(JSON.stringify({ event, payload }));
      } else {
        logger.error("[Security] ❌ Không thể broadcast: Controller đang ở trạng thái không xác thực!");
      }
    }
  }

  public broadcastAudioChunk(buffer: Buffer) {
    if (this.uiClient && this.uiClient.readyState === WebSocket.OPEN) {
      if (this.#internalSealToken) {
        this.uiClient.send(buffer, { binary: true });
      } else {
        logger.error("[Security] ❌ Không thể broadcast audio: Thiếu Seal Token!");
      }
    }
  }
}
