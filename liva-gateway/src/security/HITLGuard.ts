import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import { TelegramManager } from "../services/TelegramManager";
import { TraceContext } from "../utils/TraceContext";

export interface HITLRequest {
    id: string;
    toolName: string;
    args: any;
    reason?: string;
    channel?: string;
    image?: string;
}

export class HITLGuard {
    private static readonly TIMEOUT_MS = 300000; // 300s
    public static readonly events = new EventEmitter();
    
    // Map of pending approvals
    private static pendingRequests = new Map<string, { 
        request: HITLRequest, 
        resolve: (val: boolean) => void, 
        reject: (err: Error) => void, 
        timer: NodeJS.Timeout 
    }>();
    
    private static telegramManager = new TelegramManager();

    /**
     * Request approval from the User.
     * Emits 'hitl_request' which the Gateway (WebSocket) should listen to and forward to UI.
     * Throws an error if rejected or timed out.
     */
    static async requestApproval(request: Omit<HITLRequest, "id">): Promise<boolean> {
        return new Promise((resolve, reject) => {
            const channel = TraceContext.getStore()?.channel || "ui";
            const id = `hitl-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`; // NOSONAR
            const fullReq: HITLRequest = { ...request, id, channel };

            logger.warn({ fullReq }, `[HITLGuard] ⚠️ CẢNH BÁO AN TOÀN / SAFETY ALERT: LLM yêu cầu gọi công cụ rủi ro / LLM requested sensitive tool: ${request.toolName}. Đang chờ phê duyệt / Awaiting approval (Timeout: 300s)...`);

            const timeoutId = setTimeout(() => {
                logger.warn(`[HITLGuard] ⏳ Timeout 300s - Tự động từ chối yêu cầu gọi ${request.toolName} chống Deadlock.`);
                HITLGuard.pendingRequests.delete(id);
                reject(new Error("REJECTED_BY_TIMEOUT"));
            }, this.TIMEOUT_MS);

            HITLGuard.pendingRequests.set(id, { request: fullReq, resolve, reject, timer: timeoutId });
            
            // Phát sự kiện ra ngoài
            HITLGuard.events.emit("hitl_request", fullReq);

            // Luôn gửi thông báo Telegram để làm nhật ký bảo mật trung tâm và giữ tương thích với test suites
            const text = `🔔 *Yêu cầu phê duyệt hành động rủi ro / Risk Action Approval Required*\n\n` +
                         `*Công cụ / Tool:* \`${request.toolName}\`\n` +
                         `*Lý do / Reason:* ${request.reason || "Không có / None"}\n\n` +
                         `Vui lòng duyệt hoặc từ chối trực tiếp trên Telegram.\n` +
                         `Please approve or reject directly on Telegram.`;
            
            const keyboard = [
                [
                    { text: "✅ Approve", callback_data: `approve:${id}` },
                    { text: "❌ Reject", callback_data: `reject:${id}` }
                ]
            ];
            HITLGuard.telegramManager.sendMessage(text, keyboard).catch(e => {
                logger.warn(`[HITLGuard] Không thể gửi thông báo Telegram: ${e.message}`);
            });

            // Gửi tin nhắn xác nhận phụ trợ dựa trên kênh của turn hiện tại (Người dùng nhắn ở đâu xác nhận thêm ở đó)
            if (channel === "zalo") {
                const zaloText = `🔔 *YÊU CẦU XÁC NHẬN BẢO MẬT LIVA / LIVA SECURITY CONFIRMATION*\n\n` +
                             `*Hành động / Action:* Gửi tin Zalo đến / Send Zalo to "${request.args.targetName}"\n` +
                             `*Nội dung / Content:* "${request.args.message}"\n\n` +
                             `👉 Vui lòng trả lời *YES*, *OK*, hoặc *DUYỆT* để đồng ý gửi, hoặc *NO*, *HUY* để hủy bỏ.\n` +
                             `👉 Please reply *YES*, *OK*, or *APPROVE* to allow, or *NO*, *CANCEL* to reject.`;
                import("../utils/ZaloNotifier").then(m => m.notifyZalo(zaloText)).catch(e => {
                    logger.warn(`[HITLGuard] Không thể gửi thông báo Zalo: ${e.message}`);
                });
            } else if (channel === "ui") {
                // UI hoặc kênh mặc định: Gửi bubble tin nhắn trực quan vào thanh chat LIVA UI
                let promptHtml = `<b>LIVA (Bảo mật / Security):</b> Tôi chuẩn bị gửi tin nhắn / Preparing to send message:<br/>` +
                                 `<b>Người nhận / Recipient:</b> <code>${request.args.targetName}</code><br/>` +
                                 `<b>Nội dung / Content:</b> <i>${request.args.message}</i><br/>`;

                if (request.image) {
                    promptHtml += `<img src="${request.image}" style="max-width: 100%; border-radius: 8px; margin-top: 8px; border: 1px solid #444;" /><br/>`;
                }

                promptHtml += `<br/>👉 Bạn có đồng ý gửi không? / Do you authorize this action?<br/>` +
                              `<div class="hitl-container">` +
                              `  <button onclick="window.sendLIVAMessage('yes')" class="hitl-btn hitl-btn-approve">✅ Đồng ý / Approve</button>` +
                              `  <button onclick="window.sendLIVAMessage('no')" class="hitl-btn hitl-btn-reject">❌ Hủy bỏ / Reject</button>` +
                              `</div>`;

                // Phát trực tiếp vào giao diện UI qua global kernel instance
                if (globalThis.kernelInstance?.ui) {
                    globalThis.kernelInstance.ui.broadcastUIEvent("ai_spoken_response", { text: promptHtml });
                }
            }
        });
    }

    /**
     * Get pending request for a specific channel
     */
    static getPendingByChannel(channel: string): HITLRequest | null {
        for (const entry of this.pendingRequests.values()) {
            if (entry.request.channel === channel) {
                return entry.request;
            }
        }
        return null;
    }

    /**
     * Receive response from UI or platforms
     */
    static respond(id: string, approved: boolean) {
        const pending = HITLGuard.pendingRequests.get(id);
        if (pending) {
            clearTimeout(pending.timer);
            HITLGuard.pendingRequests.delete(id);
            if (approved) {
                logger.info(`[HITLGuard] ✅ User ĐÃ PHÊ DUYỆT yêu cầu / User APPROVED request ${id}`);
                pending.resolve(true);
            } else {
                logger.info(`[HITLGuard] ❌ User ĐÃ TỪ CHỐI yêu cầu / User DECLINED request ${id}`);
                pending.reject(new Error("REJECTED_BY_USER"));
            }
        } else {
            logger.warn(`[HITLGuard] Nhận được phản hồi cho ID ${id} nhưng yêu cầu không tồn tại (đã timeout?).`);
        }
    }
}
