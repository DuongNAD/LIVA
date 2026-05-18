import { EventEmitter } from "node:events";
import { CDPBridge } from "../bridges/CDPBridge";
import { TelegramBridge } from "../channels/TelegramBridge";
import { logger } from "../utils/logger";

export interface SecurityConfig {
    blockedCommands: string[];
    allowedCommands: string[];
    cooldownMs: number;
}

export class AutoAcceptDaemon extends EventEmitter {
    #cdpBridge: CDPBridge;
    #telegramBridge: TelegramBridge;
    #config: SecurityConfig;
    #isEnabled = true;
    #lastClickTime = 0;
    #pendingApproval: any | null = null;
    #hitlTimer: NodeJS.Timeout | null = null;
    #chatId: string;

    constructor(cdpBridge: CDPBridge, telegramBridge: TelegramBridge, config?: Partial<SecurityConfig>) {
        super();
        this.#cdpBridge = cdpBridge;
        this.#telegramBridge = telegramBridge;
        this.#chatId = process.env.TELEGRAM_CHAT_ID || "";
        this.#config = {
            blockedCommands: config?.blockedCommands || [
                'rm -rf /', 'rm -rf /*', 'rm -rf ~', 'rm -rf .git',
                'git push --force', 'git push -f', 'git clean -fdx',
                'drop database', 'drop table', 'truncate table',
                'format c:', 'dd if=', 'shutdown', 'reboot', 'mkfs.', 'wipefs', 'shred'
            ],
            allowedCommands: config?.allowedCommands || [],
            cooldownMs: config?.cooldownMs || 3000
        };

        this.#setupListeners();
    }

    public enable() {
        this.#isEnabled = true;
        logger.info("[AutoAccept] 🛡️ Daemon Enabled.");
    }

    public disable() {
        this.#isEnabled = false;
        logger.info("[AutoAccept] 🛑 Daemon Disabled.");
    }

    #setupListeners() {
        this.#cdpBridge.on("approval_required", async (payload: any) => {
            if (!this.#isEnabled) return;
            await this.#handleApproval(payload);
        });

        // Listen for callbacks from Telegram
        this.#telegramBridge.on("callback_query", async (event: any) => {
            const data = event.data as string;
            if (data.startsWith("approve:hitl_") || data.startsWith("reject:hitl_")) {
                const approve = data.startsWith("approve:");
                
                // If there's no pending approval, just ignore or edit message
                if (!this.#pendingApproval) {
                    if (event.messageId) {
                        await this.#telegramBridge.editMessage(event.chatId, event.messageId, `_This approval request has expired or was already handled._`);
                    }
                    return;
                }

                logger.info(`[AutoAccept] 👨‍💻 HITL Override: ${approve ? 'Approve' : 'Reject'}`);
                
                if (event.messageId) {
                    await this.#telegramBridge.editMessage(
                        event.chatId, 
                        event.messageId, 
                        `_HITL Override: ${approve ? '✅ Approved' : '❌ Rejected'} by ${event.senderId}_`
                    );
                }

                this.#resolveHITL(approve);
            }
        });
    }

    async #handleApproval(payload: { text: string, command: string, selector: string }) {
        const now = Date.now();
        if (now - this.#lastClickTime < this.#config.cooldownMs) {
            return; // Cooldown active
        }

        const cmd = (payload.command || "").toLowerCase();

        // 1. Check Blacklist -> immediate rejection
        if (this.#config.blockedCommands.some(bad => cmd.includes(bad))) {
            logger.warn(`[AutoAccept] 🚫 BLOCKED malicious command: ${cmd}`);
            await this.#cdpBridge.clickApprovalButton(false); // Reject
            if (this.#chatId) {
                await this.#telegramBridge.sendText(this.#chatId, `🚨 **LIVA Security Alert**\n\nBlocked malicious command:\n\`\`\`bash\n${payload.command}\n\`\`\``);
            }
            return;
        }

        // 2. Check Whitelist
        this.#lastClickTime = now;
        const isSafe = this.#config.allowedCommands.length === 0 || this.#config.allowedCommands.some(good => cmd.startsWith(good));

        if (isSafe) {
            logger.info(`[AutoAccept] ⚡ Auto-approving safe command: ${cmd}`);
            await this.#cdpBridge.clickApprovalButton(true);
        } else {
            // 3. HITL Fallback
            if (this.#pendingApproval) return; // Wait for current HITL
            
            logger.info(`[AutoAccept] ⚠️ Command requires HITL: ${cmd}`);
            this.#pendingApproval = payload;
            
            if (this.#chatId) {
                const hitlId = `hitl_${Date.now()}`;
                await this.#telegramBridge.sendApprovalCard(
                    this.#chatId,
                    "Approval Required",
                    payload.command,
                    hitlId
                );
            }

            // Timeout after 60s
            this.#hitlTimer = setTimeout(() => {
                logger.warn("[AutoAccept] ⏱️ HITL Timeout. Rejecting.");
                this.#resolveHITL(false);
            }, 60000);
        }
    }

    async #resolveHITL(approve: boolean) {
        this.#pendingApproval = null;
        clearTimeout(this.#hitlTimer as NodeJS.Timeout);
        this.#hitlTimer = null;
        await this.#cdpBridge.clickApprovalButton(approve);
        this.#lastClickTime = Date.now();
    }
}
