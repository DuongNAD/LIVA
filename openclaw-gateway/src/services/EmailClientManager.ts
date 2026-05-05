import { ImapFlow } from "imapflow";
import { logger } from "../utils/logger";
import { SensoryManager } from "../memory/SensoryManager";
import * as fsp from "fs/promises";
import * as path from "path";

export class EmailClientManager {
    private client: ImapFlow | null = null;
    #reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    private lastProcessedUID: number = 0;
    private retryCount: number = 0;
    private isRunning: boolean = false;
    private uidFilePath = path.join(process.cwd(), ".email_last_uid");

    private async loadLastUID() {
        try {
            const data = await fsp.readFile(this.uidFilePath, "utf-8");
            this.lastProcessedUID = parseInt(data, 10) || 0;
        } catch {
            this.lastProcessedUID = 0;
        }
    }

    private async saveLastUID() {
        try {
            const tmpPath = `${this.uidFilePath}.tmp`;
            await fsp.writeFile(tmpPath, this.lastProcessedUID.toString(), "utf-8");
            await fsp.rename(tmpPath, this.uidFilePath);
        } catch (e: any) {
            logger.error(`[EmailClientManager] Lỗi lưu UID: ${e.message}`);
        }
    }

    public async startIdling() {
        this.isRunning = true;
        await this.loadLastUID();
        await this.connectWithBackoff();
    }

    private async connectWithBackoff() {
        if (!this.isRunning) return;

        const host = process.env.EMAIL_HOST || "";
        const user = process.env.EMAIL_USER || "";
        const pass = process.env.EMAIL_PASS || "";

        if (!host || !user || !pass) {
            logger.warn("[EmailClientManager] Thiếu config Email (EMAIL_HOST, EMAIL_USER, EMAIL_PASS). Bỏ qua Ingress.");
            return;
        }

        try {
            this.client = new ImapFlow({
                host,
                port: 993,
                secure: true,
                auth: { user, pass },
                logger: false // Mute default logs
            });

            await this.client.connect();
            logger.info("[EmailClientManager] IMAP Connected. Bắt đầu theo dõi inbox.");
            this.retryCount = 0; // Reset backoff

            // Process existing new emails first (simplified for boilerplate)
            await this.processNewEmails();

            const lock = await this.client.getMailboxLock('INBOX');
            try {
                this.client.on('exists', () => {
                    this.processNewEmails().catch(e => logger.error(`[EmailClientManager] Lỗi xử lý mail: ${e.message}`));
                });

                this.client.on('close', () => {
                    logger.warn("[EmailClientManager] IMAP connection closed by server. Reconnecting...");
                    this.triggerReconnect();
                });
            } finally {
                // We hold the lock while idling
                lock.release();
            }
        } catch (e: any) {
            logger.error(`[EmailClientManager] Lỗi kết nối IMAP: ${e.message}`);
            this.triggerReconnect();
        }
    }

    private triggerReconnect() {
        if (!this.isRunning) return;
        if (this.client) {
            this.client.close().catch(() => {});
            this.client = null;
        }

        const delay = Math.min(Math.pow(2, this.retryCount) * 1000, 60000); // Max 60s
        this.retryCount++;
        logger.info(`[EmailClientManager] Reconnecting in ${delay}ms (Attempt ${this.retryCount})`);

        // Guard: clear any existing timer before scheduling a new one
        if (this.#reconnectTimer) clearTimeout(this.#reconnectTimer);
        this.#reconnectTimer = setTimeout(() => {
            this.connectWithBackoff();
        }, delay);
    }

    public sanitizeHTML(html: string): string {
        return html.replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '') // Remove scripts
                   .replace(/<[^>]*>?/gm, ' ') // Remove all HTML tags
                   .replace(/\s+/g, ' ') // Collapse spaces
                   .trim();
    }

    private async processNewEmails() {
        if (!this.client) return;
        
        try {
            // Logic lấy mail mới nhất (Giả lập logic iterator của imapflow)
            for await (let msg of this.client.fetch({ uid: `${this.lastProcessedUID + 1}:*` }, { uid: true, source: true })) {
                if (msg.uid <= this.lastProcessedUID) continue;

                const rawBody = msg.source.toString('utf-8');
                const sanitizedText = this.sanitizeHTML(rawBody);
                
                // Đóng gói Event Brick
                SensoryManager.getInstance().ingest("email", {
                    type: "email",
                    content: sanitizedText,
                    uid: msg.uid
                }, 86400000); // 24h TTL
                
                this.lastProcessedUID = Math.max(this.lastProcessedUID, msg.uid);
            }
            await this.saveLastUID();
        } catch (e: any) {
            logger.error(`[EmailClientManager] Fetch failed: ${e.message}`);
        }
    }

    public dispose() {
        this.isRunning = false;
        if (this.#reconnectTimer) {
            clearTimeout(this.#reconnectTimer);
            this.#reconnectTimer = null;
        }
        if (this.client) {
            this.client.logout().catch(() => {});
            this.client = null;
        }
        logger.info("[EmailClientManager] Disposed an toàn.");
    }
}
