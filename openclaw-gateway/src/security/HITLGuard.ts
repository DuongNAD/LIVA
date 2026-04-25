import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";

export interface HITLRequest {
    id: string;
    toolName: string;
    args: any;
    reason?: string;
}

export class HITLGuard {
    private static readonly TIMEOUT_MS = 60000; // 60s
    public static readonly events = new EventEmitter();
    
    // Map of pending approvals
    private static pendingRequests = new Map<string, { resolve: (val: boolean) => void, reject: (err: Error) => void, timer: NodeJS.Timeout }>();

    /**
     * Request approval from the User.
     * Emits 'hitl_request' which the Gateway (WebSocket) should listen to and forward to UI.
     * UI sends back 'hitl_response' with { id, approved: boolean }.
     * Throws an error if rejected or timed out.
     */
    static async requestApproval(request: Omit<HITLRequest, "id">): Promise<boolean> {
        return new Promise((resolve, reject) => {
            const id = `hitl-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`; // NOSONAR
            const fullReq: HITLRequest = { ...request, id };

            logger.warn({ fullReq }, `[HITLGuard] ⚠️ CẢNH BÁO AN TOÀN: LLM yêu cầu gọi công cụ rủi ro cao: ${request.toolName}. Đang chờ phê duyệt (Timeout: 60s)...`);

            const timeoutId = setTimeout(() => {
                logger.warn(`[HITLGuard] ⏳ Timeout 60s - Tự động từ chối yêu cầu gọi ${request.toolName} chống Deadlock.`);
                HITLGuard.pendingRequests.delete(id);
                reject(new Error("REJECTED_BY_TIMEOUT"));
            }, this.TIMEOUT_MS);

            HITLGuard.pendingRequests.set(id, { resolve, reject, timer: timeoutId });
            
            // Phát sự kiện ra ngoài để UI/Gateway hứng và gửi xuống Webview
            HITLGuard.events.emit("hitl_request", fullReq);
        });
    }

    /**
     * Receive response from UI
     */
    static respond(id: string, approved: boolean) {
        const pending = HITLGuard.pendingRequests.get(id);
        if (pending) {
            clearTimeout(pending.timer);
            HITLGuard.pendingRequests.delete(id);
            if (approved) {
                logger.info(`[HITLGuard] ✅ User ĐÃ PHÊ DUYỆT yêu cầu ${id}`);
                pending.resolve(true);
            } else {
                logger.info(`[HITLGuard] ❌ User ĐÃ TỪ CHỐI yêu cầu ${id}`);
                pending.reject(new Error("REJECTED_BY_USER"));
            }
        } else {
            logger.warn(`[HITLGuard] Nhận được phản hồi cho ID ${id} nhưng yêu cầu không tồn tại (đã timeout?).`);
        }
    }
}
