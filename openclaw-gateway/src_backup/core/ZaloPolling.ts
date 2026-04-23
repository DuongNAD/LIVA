import axios from "axios";
import { EventEmitter } from "events";
import { logger } from "../utils/logger";

export class ZaloPolling extends EventEmitter {
  private accessToken: string;
  private isPolling: boolean = false;
  private currentOffset: number = 0;

  constructor() {
    super();
    this.accessToken = process.env.ZALO_OA_ACCESS_TOKEN || "";
    // Chỉ kích hoạt nếu là Token kiểu mới có chứa dấu ":"
    if (this.accessToken && this.accessToken.includes(":")) {
      logger.info("📡 [Zalo] Tìm thấy Cấu hình chuẩn. Kích hoạt Cảm biến Listener Zalo...");
      // Defer async polling to microtask queue (outside constructor body)
      // Satisfies SonarQube S4738: async operations must not be called in constructors
      Promise.resolve().then(() => this.startPolling());
    } else {
      logger.warn("⚠️ [Zalo] Không tìm thấy ZALO_OA_ACCESS_TOKEN hợp lệ. Cảm biến Zalo sẽ tạm tắt.");
    }
  }

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

        const res = await axios.post(
          `https://bot-api.zaloplatforms.com/bot${this.accessToken}/getUpdates`,
          payload,
          { timeout: 7000 } // Quá 7s mà Zalo không trả lời thì tự ngắt Connection
        );

        if (res.data && res.data.ok && res.data.result) {
          const updates = Array.isArray(res.data.result) ? res.data.result : [res.data.result];
          
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
         if (e.code !== 'ECONNABORTED') {
           logger.error(`[Zalo Listener Error] ${e.message} \n ${e.response?.data ? JSON.stringify(e.response.data) : ''}`);
         }
      }

      // Nghỉ 1 nhịp trước khi quét phễu tiếp để vCPU rảnh hoàn toàn.
      setTimeout(poll, 1500);
    };

    poll(); // Phát súng đầu tiên
  }

  public stop() {
    this.isPolling = false;
    logger.info("⚠️ [Zalo Listener] Trạm cảm biến Zalo đã đóng.");
  }
}
