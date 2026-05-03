/**
 * ApprovalEngine — Human-in-the-Loop Approval Flow Manager (Phase 1)
 * =====================================================================
 * Bridges IDE approval buttons with messaging channels.
 * When an IDE (Antigravity/VS Code) requires human approval for
 * a dangerous operation, this engine:
 *   1. Creates a pending approval record
 *   2. Forwards it to the user's phone via Telegram/Zalo
 *   3. Waits for approve/reject callback
 *   4. Relays decision back to the IDE
 *
 * Features:
 *   - TTL-based auto-expiry (default: 5 minutes)
 *   - Audit trail for all decisions
 *   - Multi-channel forwarding
 *
 * [v5.0] LIVA Remote Control Hub
 */

import { EventEmitter } from "node:events";
import { randomUUID } from "node:crypto";
import { logger } from "../utils/logger";
import type { ChannelAdapter } from "../channels/ChannelNormalizer";

// ===========================
// Types
// ===========================

export interface PendingApproval {
    /** Unique approval ID */
    id: string;
    /** Source IDE */
    source: "antigravity" | "vscode";
    /** Command/action requiring approval */
    command: string;
    /** Additional context (code snippet, file path, etc.) */
    context: string;
    /** Risk level */
    risk: "safe" | "moderate" | "dangerous";
    /** Creation timestamp */
    createdAt: number;
    /** Resolution timestamp */
    resolvedAt?: number;
    /** Approval decision */
    approved?: boolean;
    /** Channel the approval was sent to */
    forwardedTo?: string;
    /** Sender ID the approval was forwarded to */
    forwardedSenderId?: string;
}

export interface ApprovalAuditEntry {
    approvalId: string;
    action: "created" | "forwarded" | "approved" | "rejected" | "expired";
    timestamp: number;
    details?: string;
}

// ===========================
// ApprovalEngine
// ===========================

export class ApprovalEngine extends EventEmitter {
    readonly #pendingApprovals = new Map<string, PendingApproval>();
    readonly #auditTrail: ApprovalAuditEntry[] = [];
    readonly #defaultTTL: number;
    #expiryTimer: ReturnType<typeof setInterval> | null = null;

    constructor(ttlMs: number = 300_000) { // Default: 5 minutes
        super();
        this.#defaultTTL = ttlMs;

        // Start periodic expiry check
        this.#expiryTimer = setInterval(() => this.#expireStale(), 30_000);
        this.#expiryTimer.unref(); // Don't prevent process exit
    }

    // ═══════════════════════════════════════
    //  Create & Forward
    // ═══════════════════════════════════════

    /**
     * Create a new pending approval.
     * Returns the approval ID for tracking.
     */
    public createApproval(
        source: "antigravity" | "vscode",
        command: string,
        context: string,
        risk: "safe" | "moderate" | "dangerous" = "moderate"
    ): string {
        const id = randomUUID();
        const approval: PendingApproval = {
            id,
            source,
            command,
            context,
            risk,
            createdAt: Date.now(),
        };

        this.#pendingApprovals.set(id, approval);
        this.#audit(id, "created", `${source}: ${command.substring(0, 100)}`);

        logger.info(`[Approval] 🆕 Created: ${id} (${risk}) — ${command.substring(0, 50)}`);
        return id;
    }

    /**
     * Forward a pending approval to a messaging channel.
     * Sends an approval card with Approve/Reject buttons.
     */
    public async forwardToChannel(
        approvalId: string,
        channel: ChannelAdapter,
        senderId: string
    ): Promise<void> {
        const approval = this.#pendingApprovals.get(approvalId);
        if (!approval) {
            throw new Error(`[Approval] Not found: ${approvalId}`);
        }

/* istanbul ignore next */
        const riskEmoji = approval.risk === "dangerous" ? "🔴" : approval.risk === "moderate" ? "🟡" : "🟢";

        await channel.sendApprovalCard(
            senderId,
            `${riskEmoji} ${approval.source.toUpperCase()} — Yêu cầu phê duyệt`,
            `Risk: ${approval.risk}\nCommand: ${approval.command}\n\n${approval.context.substring(0, 3000)}`,
            approvalId
        );

        approval.forwardedTo = channel.channelName;
        approval.forwardedSenderId = senderId;
        this.#audit(approvalId, "forwarded", `→ ${channel.channelName}:${senderId}`);
    }

    // ═══════════════════════════════════════
    //  Resolve
    // ═══════════════════════════════════════

    /**
     * Resolve a pending approval (approve or reject).
     * Emits "approval_granted" or "approval_denied" event.
     */
    public resolveApproval(approvalId: string, approved: boolean): void {
        const approval = this.#pendingApprovals.get(approvalId);
        if (!approval) {
            logger.warn(`[Approval] Cannot resolve: ${approvalId} (not found or expired)`);
            return;
        }

        if (approval.resolvedAt) {
            logger.warn(`[Approval] Already resolved: ${approvalId}`);
            return;
        }

        approval.approved = approved;
        approval.resolvedAt = Date.now();

        const action = approved ? "approved" : "rejected";
        this.#audit(approvalId, action);

        logger.info(`[Approval] ${approved ? "✅" : "❌"} ${action}: ${approvalId}`);
        this.emit(approved ? "approval_granted" : "approval_denied", approval);

        // Clean up after short delay (keep for audit)
        setTimeout(() => this.#pendingApprovals.delete(approvalId), 60_000);
    }

    // ═══════════════════════════════════════
    //  Query
    // ═══════════════════════════════════════

    /** Get a pending approval by ID */
    public getApproval(id: string): PendingApproval | undefined {
        return this.#pendingApprovals.get(id);
    }

    /** Get all pending (unresolved) approvals */
    public getPendingApprovals(): PendingApproval[] {
        return [...this.#pendingApprovals.values()].filter(a => !a.resolvedAt);
    }

    /** Get audit trail (last N entries) */
    public getAuditTrail(limit: number = 50): ApprovalAuditEntry[] {
        return this.#auditTrail.slice(-limit);
    }

    /** Count of pending approvals */
    public get pendingCount(): number {
        return this.getPendingApprovals().length;
    }

    // ═══════════════════════════════════════
    //  Lifecycle
    // ═══════════════════════════════════════

    public dispose(): void {
        if (this.#expiryTimer) {
            clearInterval(this.#expiryTimer);
            this.#expiryTimer = null;
        }
    }

    // ═══════════════════════════════════════
    //  Private
    // ═══════════════════════════════════════

    #expireStale(): void {
        const now = Date.now();
        for (const [id, approval] of this.#pendingApprovals) {
            if (!approval.resolvedAt && (now - approval.createdAt) > this.#defaultTTL) {
                this.#pendingApprovals.delete(id);
                this.#audit(id, "expired");
                logger.info(`[Approval] ⏰ Expired: ${id}`);
                this.emit("approval_expired", approval);
            }
        }
    }

    #audit(approvalId: string, action: ApprovalAuditEntry["action"], details?: string): void {
        this.#auditTrail.push({
            approvalId,
            action,
            timestamp: Date.now(),
            details,
        });

        // Keep audit trail bounded
        if (this.#auditTrail.length > 500) {
            this.#auditTrail.splice(0, this.#auditTrail.length - 500);
        }
    }
}
