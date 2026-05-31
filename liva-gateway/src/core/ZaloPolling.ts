import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";

import type { ChannelAdapter } from "../channels/ChannelNormalizer";

export class ZaloPolling extends EventEmitter implements ChannelAdapter {
  public readonly channelName = "zalo" as const;
  private accessToken: string;
  private isPolling: boolean = false;
  private currentOffset: number = 0;
  // 🔒 [Audit Fix L-5] Store pending timer ref to clear on stop()
  #pollTimerRef: NodeJS.Timeout | null = null;
  #abortController: AbortController = new AbortController();

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
        const payload: Record<string, string> = { timeout: "5" }; // 5 giây giữ kết nối
        
        // Offset để đảm bảo LIVA không bị điếc nhai lại những tin nhắn đã đọc
        if (this.currentOffset > 0) {
           payload.offset = this.currentOffset.toString();
        }

        const res = await safeFetch(
          `https://bot-api.zaloplatforms.com/bot${this.accessToken}/getUpdates`,
          {
             method: "POST",
             headers: { "Content-Type": "application/json" },
             body: JSON.stringify(payload),
             signal: this.#abortController.signal
          },
          7000 // Quá 7s mà Zalo không trả lời thì tự ngắt Connection
        );
        
        const data = await res.json() as Record<string, unknown>;

        if (data?.ok && data.result) { // NOSONAR
          const updates = Array.isArray(data.result) ? data.result : [data.result];
          
          for (const update of updates) {
            if (!update) continue;

            // Xoắn Offset lên 1 nấc để dọn dẹp các update id cũ
            if (update.update_id) {
               this.currentOffset = Math.max(this.currentOffset, update.update_id + 1);
            }

            if (update.message && update.message.text) {
               const incomingText = update.message.text;
               const chat = (update.message as any).chat;
               const senderId = chat?.id ? String(chat.id) : undefined;
               
               logger.info(`💌 [Zalo Inbound] Tín hiệu từ Zalo điện thoại: "${incomingText}" (Sender ID: ${senderId})`);
               
               // Đẩy gán thêm cờ để LIVA biết người dùng đang ở ngoài dùng điện thoại
               const enrichedMessage = `[Tin nhắn từ Zalo điện thoại]: ${incomingText}`;
               
               // Gửi luồng thông báo đi xuyên vào AgentLoop Mẹ
               this.emit("zalo_incoming", enrichedMessage, senderId);
            }
          }
        }
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
         if (!(e instanceof Error) || e.name !== 'AbortError') {
           logger.error(`[Zalo Listener Error] ${errMsg}`);
         }
      }

      // Nghỉ 1 nhịp trước khi quét phễu tiếp để vCPU rảnh hoàn toàn.
      // 🔒 [Audit Fix L-5] Store timer ref for cleanup in stop()
      this.#pollTimerRef = setTimeout(poll, 1500);
    };

    poll(); // Phát súng đầu tiên
  }

  public stop() {
    this.isPolling = false;
    // 🔒 [Audit Fix L-5] Clear pending timer to prevent final fire
    if (this.#pollTimerRef) {
      clearTimeout(this.#pollTimerRef);
      this.#pollTimerRef = null;
    }
    this.#abortController.abort();
    this.#abortController = new AbortController(); // Reset for future start()
    logger.info("⚠️ [Zalo Listener] Trạm cảm biến Zalo đã đóng.");
  }

  public async sendText(senderId: string, text: string): Promise<void> {
    const token = this.accessToken;
    if (!token || !senderId) return;

    const taggedMsg = text.includes("#Liva") ? text : `${text}\n\n#Liva`;

    try {
      const isBotToken = token.includes(":");
      const endpoint = isBotToken
        ? `https://bot-api.zaloplatforms.com/bot${token}/sendMessage`
        : "https://openapi.zalo.me/v3.0/oa/message/cs";

      if (isBotToken) {
        await safeFetch(endpoint, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ chat_id: senderId, text: taggedMsg })
        });
      } else {
        await safeFetch(endpoint, {
          method: "POST",
          headers: {
            access_token: token,
            "Content-Type": "application/json"
          },
          body: JSON.stringify({
            recipient: { user_id: senderId },
            message: { text: taggedMsg }
          })
        });
      }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[ZaloPolling] sendText failed: ${errMsg}`);
    }
  }

  public async sendApprovalCard(
    senderId: string,
    title: string,
    body: string,
    approvalId: string
  ): Promise<void> {
    const token = this.accessToken;
    if (!token || !senderId) return;

    try {
      const isBotToken = token.includes(":");
      if (isBotToken) {
        // Bot Creator doesn't support buttons, fallback to text instructions
        const textMsg = `🔐 *${title}*\n\n${body.substring(0, 1500)}\n\n👉 Vui lòng trả lời *YES*, *OK*, hoặc *DUYỆT* để đồng ý, hoặc *NO*, *HUY* để hủy bỏ.`;
        await this.sendText(senderId, textMsg);
      } else {
        // OA supports templates with buttons
        const endpoint = "https://openapi.zalo.me/v3.0/oa/message/cs";
        const payload = {
          recipient: { user_id: senderId },
          message: {
            text: `🔔 ${title}`,
            attachment: {
              type: "template",
              payload: {
                template_type: "generic",
                elements: [
                  {
                    title: title,
                    subtitle: body.substring(0, 100),
                    buttons: [
                      {
                        type: "oa.query.show",
                        name: "✅ Phê duyệt",
                        payload: `approve:${approvalId}`
                      },
                      {
                        type: "oa.query.show",
                        name: "❌ Từ chối",
                        payload: `reject:${approvalId}`
                      }
                    ]
                  }
                ]
              }
            }
          }
        };
        await safeFetch(endpoint, {
          method: "POST",
          headers: {
            access_token: token,
            "Content-Type": "application/json"
          },
          body: JSON.stringify(payload)
        });
      }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[ZaloPolling] sendApprovalCard failed: ${errMsg}`);
    }
  }

  public async sendScreenshot(senderId: string, imageBuffer: Buffer): Promise<void> {
    logger.info(`[ZaloPolling] Simulated sending screenshot to ${senderId}`);
  }
}
