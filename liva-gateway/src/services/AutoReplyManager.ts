import { logger } from "../utils/logger";
import { livaEngine } from "../utils/LivaEngine";
import { ConfigManager, AutoReplyRule } from "../core/config/ConfigManager";
import { ChannelRouter, NormalizedMessage, ChannelType } from "../channels/ChannelNormalizer";
import { HITLGuard } from "../security/HITLGuard";
import { SkillRegistry } from "../SkillRegistry";
import { SessionOrchestrator } from "../core/SessionOrchestrator";

export class AutoReplyManager {
    #channelRouter: ChannelRouter;
    #sessions?: SessionOrchestrator;
    
    // Timer maps for Debouncer
    private timers: Map<string, NodeJS.Timeout> = new Map();
    private messageBuffers: Map<string, NormalizedMessage[]> = new Map();

    constructor(channelRouter: ChannelRouter, sessions?: SessionOrchestrator) {
        this.#channelRouter = channelRouter;
        this.#sessions = sessions;
        logger.info("[AutoReplyManager] 🤖 AutoReplyManager Initialized with message debouncer & session memory.");
    }

    /**
     * Handle incoming messages from channels (Zalo, Telegram, Messenger).
     * If a rule matches, intercept the message (returns true) and queues it in the accumulator.
     */
    public async handleIncomingMessage(msg: NormalizedMessage): Promise<boolean> {
        const zaloUserId = process.env.ZALO_USER_ID || "";
        const telegramChatId = process.env.TELEGRAM_CHAT_ID || "";
        
        const senderIdLower = msg.senderId.toLowerCase();
        if (
            senderIdLower === zaloUserId.toLowerCase() ||
            senderIdLower === telegramChatId.toLowerCase() ||
            msg.text.includes("#Liva") ||
            msg.text.includes("• #Liva")
        ) {
            logger.debug(`[AutoReplyManager] Ignore message from owner/LIVA to prevent loop: ${msg.senderId}`);
            return false;
        }

        // Check if Auto Responder is globally enabled in environment
        const config = ConfigManager.getInstance().get();
        if (!config.LIVA_AUTO_RESPONDER_ENABLED) {
            return false;
        }

        // Get runtime configurations from liva-config.json
        const runtimeConfig = await ConfigManager.getInstance().getLivaConfig();
        const autoReplySettings = runtimeConfig.autoReply;
        if (autoReplySettings?.enabled === false) {
            return false;
        }

        const rules: AutoReplyRule[] = autoReplySettings?.rules || [];
        const matchingRule = this.#findMatchingRule(msg, rules);

        if (!matchingRule) {
            logger.debug(`[AutoReplyManager] No auto-reply rule matched for channel: ${msg.channel}, sender: ${msg.senderId}`);
            return false;
        }

        // Rule matched! Intercept and queue the message in the debouncer.
        logger.debug(`[AutoReplyManager] Intercepted message for ${msg.channel}:${msg.senderId}. Adding to debounce queue...`);
        this.#queueMessage(msg, matchingRule);
        
        return true;
    }

    #findMatchingRule(msg: NormalizedMessage, rules: AutoReplyRule[]): AutoReplyRule | null {
        for (const rule of rules) {
            const channelMatches = rule.channel === "all" || rule.channel === msg.channel;
            if (!channelMatches) continue;

            const filter = rule.senderFilter || "*";
            if (filter === "*") {
                return rule;
            }

            const senderLower = (msg.senderName || msg.senderId || "").toLowerCase();
            if (senderLower.includes(filter.toLowerCase())) {
                return rule;
            }
        }
        return null;
    }

    #queueMessage(msg: NormalizedMessage, rule: AutoReplyRule) {
        const key = `${msg.channel}:${msg.senderId}`;
        
        if (!this.messageBuffers.has(key)) {
            this.messageBuffers.set(key, []);
        }
        this.messageBuffers.get(key)!.push(msg);

        // Reset debounce timer
        if (this.timers.has(key)) {
            clearTimeout(this.timers.get(key)!);
        }

        // Wait 8 seconds before processing consolidated context
        const timeout = setTimeout(async () => {
            await this.processAccumulatedMessages(msg.channel, msg.senderId, rule);
        }, 8000);

        this.timers.set(key, timeout);
    }

    /**
     * Consolidate buffered messages and run AI completion & reply logic.
     */
    public async processAccumulatedMessages(channel: ChannelType, senderId: string, rule: AutoReplyRule): Promise<void> {
        const key = `${channel}:${senderId}`;
        const msgs = this.messageBuffers.get(key) || [];
        
        this.timers.delete(key);
        this.messageBuffers.delete(key);

        if (msgs.length === 0) return;

        const latestMsg = msgs[msgs.length - 1];
        const consolidatedText = msgs.map(m => m.text).join("\n");
        logger.info(`[AutoReplyManager] 🎯 Processing accumulated ${channel} context for ${latestMsg.senderName || senderId} (${msgs.length} messages): "${consolidatedText.substring(0, 100)}..."`);

        // Retrieve short-term history for conversational memory
        let history: NormalizedMessage[] = [];
        let sessionId = "";
        if (this.#sessions) {
            const session = this.#sessions.getOrCreateSession(senderId, channel);
            sessionId = session.id;
            history = this.#sessions.getSessionHistory(sessionId);
        }

        // Generate response draft using AI
        const replyDraft = await this.#generateReplyDraft(latestMsg, consolidatedText, rule.instructions, history);
        if (!replyDraft) {
            logger.warn(`[AutoReplyManager] Failed to generate AI reply draft for message: "${consolidatedText}"`);
            return;
        }

        // Save turns into session history
        if (this.#sessions && sessionId) {
            const consolidatedMsg: NormalizedMessage = {
                channel,
                senderId,
                senderName: latestMsg.senderName,
                text: consolidatedText,
                timestamp: Date.now(),
                rawPayload: latestMsg.rawPayload
            };
            this.#sessions.appendMessage(sessionId, consolidatedMsg);

            const aiMsg: NormalizedMessage = {
                channel,
                senderId: "ai",
                senderName: "LIVA",
                text: replyDraft,
                timestamp: Date.now(),
                rawPayload: {}
            };
            this.#sessions.appendMessage(sessionId, aiMsg);
        }

        if (rule.mode === "autonomous") {
            logger.info(`[AutoReplyManager] Sending autonomous response to ${channel}:${senderId}`);
            await this.#sendReply(latestMsg, replyDraft);
        } else {
            // HITL Mode: Request user approval
            logger.info(`[AutoReplyManager] HITL approval required for auto-reply to ${latestMsg.senderName || senderId}`);
            this.#requestHITLApproval(latestMsg, replyDraft).catch(err => {
                logger.error(`[AutoReplyManager] HITL flow error: ${err.message}`);
            });
        }
    }

    async #generateReplyDraft(
        msg: NormalizedMessage, 
        consolidatedText: string, 
        instructions: string, 
        history: NormalizedMessage[]
    ): Promise<string | null> {
        try {
            let historyBlock = "";
            if (history.length > 0) {
                // Take the last 4 turns for conversational memory context
                historyBlock = "LỊCH SỬ TRÒ CHUYỆN GẦN ĐÂY:\n" + 
                    history.slice(-4).map(h => `${h.senderId === "ai" ? "Bạn (AI LIVA đại diện cho người dùng)" : "Người dùng"}: "${h.text}"`).join("\n") + 
                    "\n\n";
            }

            const systemPrompt = `Bạn là trợ lý AI LIVA đại diện cho người dùng để tự động trả lời tin nhắn. 
Hãy đóng vai người dùng và soạn tin nhắn trả lời một cách tự nhiên, ngắn gọn, phù hợp với ngữ cảnh giao tiếp thông thường.
TRÁNH dùng các từ ngữ quá trang trọng hay tỏ ý phục tùng của AI (KHÔNG nói 'Dạ', 'ạ', 'em' trừ khi ngữ cảnh là nói chuyện với người lớn tuổi/sếp).
Chỉ trả về DUY NHẤT nội dung tin nhắn trả lời (không thêm bất kỳ lời dẫn giải nào).

${historyBlock}HƯỚNG DẪN TRẢ LỜI: "${instructions}"`;

            const userPrompt = `Tin nhắn mới từ ${msg.senderName || "đối tác"}: "${consolidatedText}"`;

            const completion = await livaEngine.chat.completions.create({
                model: "router",
                messages: [
                    { role: "system", content: systemPrompt },
                    { role: "user", content: userPrompt }
                ],
                temperature: 0.7,
                max_tokens: 300
            });

            const reply = completion.choices[0]?.message?.content?.trim();
            return reply || null;
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[AutoReplyManager] Lỗi sinh nội dung AI: ${errMsg}`);
            return null;
        }
    }

    async #sendReply(msg: NormalizedMessage, replyText: string): Promise<void> {
        try {
            const registry = SkillRegistry.getInstance();

            if (msg.channel === "email") {
                if (registry) {
                    await registry.executeSkill("reply_email", {
                        originalUid: (msg.rawPayload as Record<string, unknown>)?.uid,
                        body_text: replyText,
                        bypassHITL: true
                    });
                    logger.info(`[AutoReplyManager] Auto-reply email successfully sent via skill to ${msg.senderId}`);
                    return;
                }
            }

            if (msg.channel === "zalo") {
                if (registry) {
                    const result = await registry.executeSkill("reply_zalo_rpa", {
                        targetName: msg.senderName || "",
                        message: replyText,
                        bypassHITL: true
                    });
                    if (result && result.includes("[AuthRequired]")) {
                        logger.warn(`[AutoReplyManager] Zalo RPA requires authentication.`);
                    } else {
                        logger.info(`[AutoReplyManager] Auto-reply Zalo successfully sent via skill to ${msg.senderName}`);
                    }
                    return;
                }
            }

            if (msg.channel === "messenger") {
                if (registry) {
                    const result = await registry.executeSkill("reply_messenger_rpa", {
                        targetName: msg.senderName || "",
                        message: replyText,
                        bypassHITL: true
                    });
                    if (result && result.includes("[AuthRequired]")) {
                        logger.warn(`[AutoReplyManager] Messenger RPA requires authentication.`);
                    } else {
                        logger.info(`[AutoReplyManager] Auto-reply Messenger successfully sent via skill to ${msg.senderName}`);
                    }
                    return;
                }
            }

            const adapter = this.#channelRouter.getAdapter(msg.channel);
            if (!adapter) {
                logger.warn(`[AutoReplyManager] No channel adapter found for ${msg.channel}`);
                return;
            }

            // Append Liva tag if not present
            const taggedMsg = replyText.includes("#Liva") ? replyText : `${replyText} • #Liva`;
            await adapter.sendText(msg.senderId, taggedMsg);
            logger.info(`[AutoReplyManager] Auto-reply successfully sent to ${msg.channel}:${msg.senderId}`);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[AutoReplyManager] Failed to send auto-reply: ${errMsg}`);
        }
    }

    async #requestHITLApproval(msg: NormalizedMessage, replyDraft: string): Promise<void> {
        try {
            const reason = `[Tự động trả lời] Gửi tin nhắn đến ${msg.senderName || msg.senderId} (${msg.channel.toUpperCase()})`;

            const approved = await HITLGuard.requestApproval({
                toolName: "auto_reply",
                args: {
                    channel: msg.channel,
                    recipient: msg.senderName || msg.senderId,
                    incomingMessage: msg.text,
                    draftReply: replyDraft
                },
                reason: `${reason} với nội dung: "${replyDraft}"`
            });

            if (approved) {
                logger.info(`[AutoReplyManager] HITL Approved. Sending reply to ${msg.channel}:${msg.senderId}`);
                await this.#sendReply(msg, replyDraft);
            } else {
                logger.info(`[AutoReplyManager] HITL Denied. Skipped reply to ${msg.channel}:${msg.senderId}`);
            }
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[AutoReplyManager] HITL Approval error: ${errMsg}`);
        }
    }
}
