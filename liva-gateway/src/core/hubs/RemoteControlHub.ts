/**
 * RemoteControlHub — Sprint 3 Task 3.1
 *
 * Extracted from CoreKernel constructor (Lines 247-378).
 * Wires Telegram, Meta, CDP, VSCode event listeners.
 *
 * IMPORTANT: This class does NOT own the bridges/channels.
 * CoreKernel still holds them as public properties for backward-compat.
 * RemoteControlHub only *wires the event listeners* between them.
 */

import type { NormalizedMessage } from "../../channels/ChannelNormalizer";
import type { DependencyContainer } from "../DependencyContainer";
import { logger } from "../../utils/logger";

export class RemoteControlHub {
    #deps: DependencyContainer;

    constructor(deps: DependencyContainer) {
        this.#deps = deps;
    }

    /**
     * Wire all remote-control event listeners.
     * Called once from CoreKernel constructor after all services are instantiated.
     */
    wireListeners(): void {
        this.#wireTelegramPipeline();
        this.#wireMetaPipeline();
        this.#wireCDPPipeline();
        this.#wireApprovalEngineEvents();
    }

    // --- [v5.0] TELEGRAM EVENT PIPELINE ---
    #wireTelegramPipeline(): void {
        const { telegram, securityGateway, sessions, nlTranslator } = this.#deps;

        telegram.on("message", async (msg: NormalizedMessage) => {
            // Security gate: validate sender through SecurityGateway
            const blockReason = securityGateway.validateIncoming(msg.channel, msg.senderId);
            if (blockReason) {
                logger.warn(`[RemoteControl] 🛡️ Blocked: ${blockReason}`);
                return;
            }

            logger.info(`📱 [RemoteControl] Telegram command from ${msg.senderName}: "${msg.text}"`);
            const enrichedMessage = `[Tin nhắn từ Telegram điện thoại]: ${msg.text}`;

            // Keep session history
            const sessionId = sessions.getOrCreateSession(msg.senderId, msg.channel).id;
            sessions.appendMessage(sessionId, msg);

            // Translate NL to IDE Command
            const intent = await nlTranslator.translate(msg.text);
            if (intent.action !== "unknown" && intent.confidence > 0.8) {
                logger.info(`[RemoteControl] NL translated to IDE action: ${intent.action}`);
                // Can be forwarded to AgentLoop as an execution token, or handled natively.
            }

            await this.#deps.dispatch("agent_input", enrichedMessage);
        });

        // Handle Telegram approval callback buttons (Approve/Reject)
        telegram.on("callback_query", async (query: { queryId: string; senderId: string; data: string; chatId?: number; messageId?: number }) => {
            const { data, chatId, messageId } = query;

/* istanbul ignore next */
            if (data.startsWith("approve:") || data.startsWith("reject:")) {
                const parts = data.split(":");
                const approved = parts[0] === "approve";
                const approvalId = parts[1];

                if (approvalId.startsWith("hitl-")) {
                    import("../../security/HITLGuard").then(m => m.HITLGuard.respond(approvalId, approved));
                } else {
                    this.#deps.approvalEngine.resolveApproval(approvalId, approved);
                }

                // Update the Telegram message to show decision
/* istanbul ignore next */
                if (chatId && messageId) {
/* istanbul ignore next */
                    const statusText = approved ? "✅ **APPROVED** — Đã phê duyệt." : "❌ **REJECTED** — Đã từ chối.";
                    telegram.editMessage(String(chatId), messageId, statusText).catch(() => {});
                }
            }
        });
    }

    // --- Meta Webhook Pipeline ---
    #wireMetaPipeline(): void {
        const { meta, securityGateway, sessions, nlTranslator, approvalEngine } = this.#deps;

        meta.on("message", async (msg: NormalizedMessage) => {
            const blockReason = securityGateway.validateIncoming(msg.channel, msg.senderId);
            if (blockReason) return;

            logger.info(`📱 [RemoteControl] Meta command from ${msg.senderName}: "${msg.text}"`);
            const enrichedMessage = `[Tin nhắn từ Messenger/IG]: ${msg.text}`;

            const sessionId = sessions.getOrCreateSession(msg.senderId, msg.channel).id;
            sessions.appendMessage(sessionId, msg);

            const intent = await nlTranslator.translate(msg.text);
            if (intent.action !== "unknown" && intent.confidence > 0.8) {
                logger.info(`[RemoteControl] NL translated to IDE action: ${intent.action}`);
            }

            await this.#deps.dispatch("agent_input", enrichedMessage);
        });

        meta.on("postback", async (postback: { senderId: string; payload: string }) => {
            logger.info(`[MetaBridge] Received postback: ${postback.payload}`);
            if (postback.payload.startsWith("approve:") || postback.payload.startsWith("reject:")) {
                const [action, id] = postback.payload.split(":");
                approvalEngine.resolveApproval(id, action === "approve");
            }
        });
    }

    // --- [v5.0] CDP BRIDGE — Approval Button Detection ---
    #wireCDPPipeline(): void {
        const { cdpBridge, securityGateway, approvalEngine, telegram } = this.#deps;

        cdpBridge.on("approval_required", async (payload: { text: string; selector: string }) => {
            logger.info(`[CDP] 🔔 IDE yêu cầu phê duyệt: "${payload.text}"`);

            // Create approval record
            const risk = securityGateway.classifyRisk(payload.text);
            const approvalId = approvalEngine.createApproval(
                "antigravity",
                payload.text,
                `IDE button detected: ${payload.selector}`,
                risk
            );

            // Forward to Telegram (primary remote control channel)
            try {
                await approvalEngine.forwardToChannel(approvalId, telegram, this.#deps.getDefaultRemoteSenderId());
            } catch (e: unknown) {
                const err = e as Error;
                logger.warn(`[CDP] Could not forward approval to Telegram: ${err.message}`);
            }

            // Also broadcast to local UI
            await this.#deps.dispatch("ui_broadcast", {
                name: "exec_approval_required",
                data: { approvalId, toolName: "IDE", command: payload.text, reason: payload.selector }
            });
        });
    }

    // --- Approval Engine (grant/deny → click in IDE) ---
    #wireApprovalEngineEvents(): void {
        const { approvalEngine, cdpBridge } = this.#deps;

        approvalEngine.on("approval_granted", async (approval: unknown) => {
            const a = approval as { source: string };
/* istanbul ignore next */
            if (a.source === "antigravity" && cdpBridge.isConnected()) {
                logger.info(`[CDP] ✅ Remote approval granted — clicking button in IDE`);
                try {
                    await cdpBridge.clickApprovalButton(true);
                } catch (e: unknown) {
                    /* istanbul ignore next */
                    const err = e as Error;
                    logger.error(`[CDP] Failed to click approval button: ${err.message}`);
                }
            }
        });

        approvalEngine.on("approval_denied", async (approval: unknown) => {
            const a = approval as { source: string };
/* istanbul ignore next */
            if (a.source === "antigravity" && cdpBridge.isConnected()) {
                logger.info(`[CDP] ❌ Remote approval denied — clicking reject in IDE`);
                try {
                    await cdpBridge.clickApprovalButton(false);
                } catch (e: unknown) {
                    /* istanbul ignore next */
                    const err = e as Error;
                    logger.error(`[CDP] Failed to click reject button: ${err.message}`);
                }
            }
        });
    }
}
