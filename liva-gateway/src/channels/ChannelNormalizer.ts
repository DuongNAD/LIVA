/**
 * ChannelNormalizer — Multi-Channel Ingress Interface (Phase 1)
 * ==============================================================
 * Defines the contract for all messaging channel adapters.
 * Every channel (Telegram, Zalo, Meta) normalizes messages
 * into a single `NormalizedMessage` format before reaching
 * the AgentLoop core.
 *
 * [v5.0] LIVA Remote Control Hub
 */

// ===========================
// Normalized Message Format
// ===========================

export interface NormalizedMessage {
    /** Source channel identifier */
    channel: ChannelType;
    /** Unique sender ID on the source platform */
    senderId: string;
    /** Human-readable sender name (if available) */
    senderName?: string;
    /** Text content of the message */
    text: string;
    /** URL or base64 of attached media (image/file) */
    mediaUrl?: string;
    /** Media type hint */
    mediaType?: "image" | "video" | "file" | "audio";
    /** Reference to a previous message (for threading) */
    replyToMessageId?: string;
    /** Original platform-specific payload */
    rawPayload: unknown;
    /** Unix timestamp (ms) */
    timestamp: number;
}

export type ChannelType = "telegram" | "zalo" | "messenger" | "instagram" | "websocket";

// ===========================
// Channel Adapter Interface
// ===========================

export interface ChannelAdapter {
    /** Channel identifier */
    readonly channelName: ChannelType;

    /** Send a plain text message to a user */
    sendText(senderId: string, text: string): Promise<void>;

    /**
     * Send an approval card with action buttons.
     * Used by ApprovalEngine for Human-in-the-Loop flow.
     */
    sendApprovalCard(
        senderId: string,
        title: string,
        body: string,
        approvalId: string
    ): Promise<void>;

    /** Send a screenshot/image to a user */
    sendScreenshot(senderId: string, imageBuffer: Buffer): Promise<void>;
}

// ===========================
// Channel Router (Dispatch)
// ===========================

/**
 * ChannelRouter — Routes outbound messages to the correct channel adapter.
 * CoreKernel registers adapters at startup; AgentLoop uses this to reply
 * to the originating channel.
 */
export class ChannelRouter {
    readonly #adapters = new Map<ChannelType, ChannelAdapter>();

    /** Register a channel adapter */
    public register(adapter: ChannelAdapter): void {
        this.#adapters.set(adapter.channelName, adapter);
    }

    /** Get adapter by channel name */
    public getAdapter(channel: ChannelType): ChannelAdapter | undefined {
        return this.#adapters.get(channel);
    }

    /** Get all registered channel names */
    public getRegisteredChannels(): ChannelType[] {
        return [...this.#adapters.keys()];
    }

    /** Send text to the originating channel */
    public async replyText(msg: NormalizedMessage, text: string): Promise<void> {
        const adapter = this.#adapters.get(msg.channel);
        if (adapter) {
            await adapter.sendText(msg.senderId, text);
        }
    }

    /** Forward approval card to the originating channel */
    public async sendApproval(
        msg: NormalizedMessage,
        title: string,
        body: string,
        approvalId: string
    ): Promise<void> {
        const adapter = this.#adapters.get(msg.channel);
        if (adapter) {
            await adapter.sendApprovalCard(msg.senderId, title, body, approvalId);
        }
    }
}
