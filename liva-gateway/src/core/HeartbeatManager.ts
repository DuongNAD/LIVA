import * as fs from 'node:fs/promises';
import * as path from "node:path";
import { AgentLoop } from "./AgentLoop";
import { logger } from "../utils/logger";
import { getEmailCredentials } from "../utils/EmailHelper";

export class HeartbeatManager {
    #timer: NodeJS.Timeout | null = null;
    private readonly INTERVAL_MS = 30 * 60 * 1000; // 30 mins
    
    constructor(private agentLoop: AgentLoop) {}

    public start() {
        if (this.#timer) return;
        
        // Cứ 30 phút đập 1 nhịp
        this.#timer = setInterval(async () => {
            await this.triggerHeartbeat();
        }, this.INTERVAL_MS);
        this.#timer.unref(); // Don't prevent process exit
        
        logger.info("💓 [HeartbeatManager] Đã khởi động động cơ chủ động (30m/nhịp).");
    }

    public stop() {
        if (this.#timer) {
            clearInterval(this.#timer);
            this.#timer = null;
            logger.info("💓 [HeartbeatManager] Đã dừng nhịp đập.");
        }
    }

    private async triggerHeartbeat() {
        try {
            const heartbeatPath = path.join(process.cwd(), "src", "HEARTBEAT.md");
            const content = await fs.readFile(heartbeatPath, "utf-8");
            
            let finalContent = content;
            if (!getEmailCredentials()) {
                // Strip the email checklist item if email is not configured
                finalContent = content.replace(/1\.\s+Có\s+email\s+nào\s+quan\s+trọng\s+chưa\s+đọc\s+không[^\n]*\n?/g, "");
            }
            
            // Gọi AgentLoop với cờ isHeartbeat = true
            logger.info("💓 [HeartbeatManager] Phát kích thích nhịp đập (Proactive Turn)...");
            this.agentLoop.handleUserInput(finalContent, true);
        } catch (e) {
            logger.error(`[HeartbeatManager] Lỗi đọc tệp HEARTBEAT.md: ${e}`);
        }
    }
}
