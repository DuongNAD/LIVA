import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";

export class ZaloPolling extends EventEmitter {
  private accessToken: string;
  private isPolling: boolean = false;
  private currentOffset: number = 0;
  // 🔒 [Audit Fix L-5] Store pending timer ref to clear on stop()
  private pollTimerRef: NodeJS.Timeout | null = null;

  constructor() {
    super();
    this.accessToken = process.env.ZALO_OA_ACCESS_TOKEN || "";
    // Chỉ kích hoạt nếu là Token kiểu mới có chứa dấu ":"
    if (this.accessToken && this.accessToken.includes(":")) {
      logger.info("📡 [Zalo] Tìm thấy Cấu hình chuẩn. Kích hoạt Cảm biến Listener Zalo...");
      // Defer async init to microtask queue — satisfies SonarQube S4738: no async in constructors
      this._pollingPromise = Promise.resolve().then(() => this.startPolling()); // NOSONAR — intentional async init
    } else {
      logger.warn("⚠️ [Zalo] Không tìm thấy ZALO_OA_ACCESS_TOKEN hợp lệ. Cảm biến Zalo sẽ tạm tắt.");
      this._pollingPromise = Promise.resolve(); // NOSONAR — intentional async init
    }
  }

  /** Resolves when polling loop has started (or was skipped) */
  public readonly _pollingPromise: Promise<void>;

  private async startPolling() {
    this.isPolling = true;
    logger.info("📡 [Zalo Listener] Bắt đầu rà quét tin nhắn liên tục (Long-Polling)...");
    
    const poll = async () => {
      if (!this.isPolling) return;
      try {
        const payload: any = { timeout: "5" }; // 5 giây giữ kết nối
        
        // Offset để đảm bảo LIVA không bị điếc nhai lại những tin nhắn đã đọc
        if (this.currentOffset > 0) {
           payload.offset = this.currentOffset;
        }

        const res = await safeFetch(
          `https://bot-api.zaloplatforms.com/bot${this.accessToken}/getUpdates`,
          {
             method: "POST",
             headers: { "Content-Type": "application/json" },
             body: JSON.stringify(payload),
          },
          7000 // Quá 7s mà Zalo không trả lời thì tự ngắt Connection
        );
        
        const data = await res.json() as any;

        if (data?.ok && data.result) { // NOSONAR
          const updates = Array.isArray(data.result) ? data.result : [data.result];
          
          for (const update of updates) {
            if (!update) continue;

            // Xoắn Offset lên 1 nấc để dọn dẹp các update id cũ
            if (update.update_id) {
               this.currentOffset = Math.max(this.currentOffset, update.update_id + 1);
            }

            if (update.message && update.message.text) {
               const incomingText = update.message.text;
               
               logger.info(`💌 [Zalo Inbound] Tín hiệu từ Zalo điện thoại: "${incomingText}"`);
               
               // Đẩy gán thêm cờ để LIVA biết người dùng đang ở ngoài dùng điện thoại
               const enrichedMessage = `[Tin nhắn từ Zalo điện thoại]: ${incomingText}`;
               
               // Gửi luồng thông báo đi xuyên vào AgentLoop Mẹ
               this.emit("zalo_incoming", enrichedMessage);
            }
          }
        }
      } catch (e: any) {
         if (e.name !== 'AbortError') {
           logger.error(`[Zalo Listener Error] ${e.message}`);
         }
      }

      // Nghỉ 1 nhịp trước khi quét phễu tiếp để vCPU rảnh hoàn toàn.
      // 🔒 [Audit Fix L-5] Store timer ref for cleanup in stop()
      this.pollTimerRef = setTimeout(poll, 1500);
    };

    poll(); // Phát súng đầu tiên
  }

  public stop() {
    this.isPolling = false;
    // 🔒 [Audit Fix L-5] Clear pending timer to prevent final fire
    if (this.pollTimerRef) {
      clearTimeout(this.pollTimerRef);
      this.pollTimerRef = null;
    }
    logger.info("⚠️ [Zalo Listener] Trạm cảm biến Zalo đã đóng.");
  }
}
