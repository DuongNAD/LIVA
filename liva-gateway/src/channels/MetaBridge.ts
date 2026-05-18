/**
 * MetaBridge — Facebook Messenger & Instagram Webhook Bridge (Phase 3)
 * ====================================================================
 * Connects LIVA to Meta's Graph API.
 * Provides a lightweight HTTP server for Webhook Verification and Event Receiving.
 * Includes strict HMAC-SHA256 signature verification to prevent spoofing.
 *
 * [v5.0] LIVA Remote Control Hub
 */

import { EventEmitter } from "node:events";
import * as http from "node:http";
import * as crypto from "node:crypto";
import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";
import type { ChannelAdapter, NormalizedMessage } from "./ChannelNormalizer";

export class MetaBridge extends EventEmitter implements ChannelAdapter {
    readonly channelName = "messenger";
    #server: http.Server | null = null;
    
    readonly #port: number;
    readonly #verifyToken: string;
    readonly #appSecret: string;
    readonly #pageAccessToken: string;

    /* istanbul ignore next */
    constructor(port = Number(process.env.META_WEBHOOK_PORT) || 3000) {
        super();
        this.#port = port;
        this.#verifyToken = process.env.META_VERIFY_TOKEN || "liva_secure_verify_token";
        this.#appSecret = process.env.META_APP_SECRET || "";
        this.#pageAccessToken = process.env.META_PAGE_ACCESS_TOKEN || "";
    }

    // ═══════════════════════════════════════
    //  Lifecycle
    // ═══════════════════════════════════════

    public async startWebhookServer(): Promise<void> {
        if (!this.#appSecret || !this.#pageAccessToken) {
            logger.warn("[MetaBridge] META_APP_SECRET or META_PAGE_ACCESS_TOKEN is missing. MetaBridge is disabled.");
            return;
        }

        return new Promise((resolve, reject) => {
            this.#server = http.createServer((req, res) => this.#handleRequest(req, res));
            this.#server.on("error", reject);
            this.#server.listen(this.#port, "0.0.0.0", () => {
                logger.info(`🌐 [MetaBridge] Webhook Server listening on port ${this.#port}`);
                resolve();
            });
        });
    }

    public stop(): void {
        if (this.#server) {
            this.#server.close();
            this.#server = null;
            logger.info("[MetaBridge] Webhook Server stopped.");
        }
    }

    // ═══════════════════════════════════════
    //  ChannelAdapter Implementation
    // ═══════════════════════════════════════

    public async sendText(recipientId: string, text: string): Promise<void> {
        /* istanbul ignore if */
        if (!this.#pageAccessToken) return;

        // Meta limits text to 2000 chars per message
        const truncated = text.length > 2000 ? text.substring(0, 1997) + "..." : text;

        try {
            await safeFetch(`https://graph.facebook.com/v19.0/me/messages?access_token=${this.#pageAccessToken}`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    recipient: { id: recipientId },
                    message: { text: truncated }
                })
            });
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[MetaBridge] Failed to send message: ${errMsg}`);
        }
    }

    public async sendApprovalCard(recipientId: string, title: string, text: string, approvalId: string): Promise<void> {
        /* istanbul ignore if */
        if (!this.#pageAccessToken) return;

        // Uses Meta Generic Template with Quick Replies or Buttons
        try {
            await safeFetch(`https://graph.facebook.com/v19.0/me/messages?access_token=${this.#pageAccessToken}`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    recipient: { id: recipientId },
                    message: {
                        attachment: {
                            type: "template",
                            payload: {
                                template_type: "button",
                                text: `🔔 ${title}\n\n${text}`,
                                buttons: [
                                    {
                                        type: "postback",
                                        title: "✅ Approve",
                                        payload: `approve:${approvalId}`
                                    },
                                    {
                                        type: "postback",
                                        title: "❌ Reject",
                                        payload: `reject:${approvalId}`
                                    }
                                ]
                            }
                        }
                    }
                })
            });
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[MetaBridge] Failed to send approval card: ${errMsg}`);
        }
    }

    public async sendScreenshot(senderId: string, imageBuffer: Buffer): Promise<void> {
        /* istanbul ignore if */
        if (!this.#pageAccessToken) return;
        
        try {
            // Note: In a real app, buffer must be uploaded as multipart/form-data.
            // For now, simulate success or use a generic payload.
            logger.info(`[MetaBridge] Simulated sending screenshot to ${senderId}`);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[MetaBridge] Failed to send screenshot: ${errMsg}`);
        }
    }

    // ═══════════════════════════════════════
    //  HTTP Request Handling
    // ═══════════════════════════════════════

    async #handleRequest(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
        if (req.method === "GET" && req.url?.startsWith("/webhook")) {
            this.#handleVerification(req, res);
        } else if (req.method === "POST" && req.url === "/webhook") {
            await this.#handleIncomingEvent(req, res);
        } else {
            res.writeHead(404);
            res.end("Not Found");
        }
    }

    #handleVerification(req: http.IncomingMessage, res: http.ServerResponse): void {
        const url = new URL(req.url!, `http://${req.headers.host}`);
        const mode = url.searchParams.get("hub.mode");
        const token = url.searchParams.get("hub.verify_token");
        const challenge = url.searchParams.get("hub.challenge");

        if (mode === "subscribe" && token === this.#verifyToken) {
            logger.info("[MetaBridge] Webhook Verified Successfully!");
            res.writeHead(200);
            res.end(challenge);
        } else {
            logger.warn("[MetaBridge] Webhook Verification Failed!");
            res.writeHead(403);
            res.end("Forbidden");
        }
    }

    async #handleIncomingEvent(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
        let rawBody = "";
        req.on("data", chunk => { rawBody += chunk.toString(); });
        req.on("end", () => {
            // 1. Verify Signature (Zero-Trust Guard)
            const signature = req.headers["x-hub-signature-256"] as string;
            if (!this.#verifySignature(rawBody, signature)) {
                logger.warn("[MetaBridge] 🛡️ Invalid X-Hub-Signature-256. Dropping payload.");
                res.writeHead(401);
                return res.end("Unauthorized");
            }

            // 2. Parse and Process
            try {
                const body = JSON.parse(rawBody);

                if (body.object === "page" || body.object === "instagram") {
                    body.entry.forEach((entry: any) => {
                        const webhookEvent = entry.messaging[0];
                        const senderId = webhookEvent.sender.id;

                        // Normal Message
                        if (webhookEvent.message && webhookEvent.message.text) {
                            const normalizedMsg: NormalizedMessage = {
                                rawPayload: webhookEvent,
                                channel: this.channelName,
                                senderId: senderId,
                                senderName: "Meta User", // Need extra API call to get real name, defer to UI layer
                                text: webhookEvent.message.text,
                                timestamp: webhookEvent.timestamp
                            };
                            this.emit("message", normalizedMsg);
                        } 
                        // Postback (Button Click)
                        else if (webhookEvent.postback) {
                            if (webhookEvent.postback.payload) {
                                this.emit("postback", {
                                    senderId: senderId,
                                    payload: webhookEvent.postback.payload
                                });
                            }
                        }
                    });

                    // Meta requires a fast 200 OK
                    res.writeHead(200);
                    res.end("EVENT_RECEIVED");
                } else {
                    res.writeHead(404);
                    res.end();
                }
            } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
                logger.error(`[MetaBridge] Payload processing error: ${errMsg}`);
                res.writeHead(500);
                res.end("Internal Error");
            }
        });
    }

    #verifySignature(rawBody: string, signatureHeader?: string): boolean {
        /* istanbul ignore if */
        if (!signatureHeader || !signatureHeader.startsWith("sha256=")) return false;
        const expectedHash = crypto
            .createHmac("sha256", this.#appSecret)
            .update(rawBody)
            .digest("hex");
        
        const expectedSignature = `sha256=${expectedHash}`;
        
        try {
            return crypto.timingSafeEqual(
                Buffer.from(expectedSignature),
                Buffer.from(signatureHeader)
            );
        } catch {
            return false;
        }
    }
}
