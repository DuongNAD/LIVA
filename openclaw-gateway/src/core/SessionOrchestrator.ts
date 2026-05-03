/**
 * SessionOrchestrator — Multi-session Management (Phase 2)
 * =========================================================
 * Manages active remote control sessions for different users.
 * Maps a (Channel + SenderID) to a specific IDE workspace context.
 *
 * Features:
 *   - 1:1 Mapping between sender ID and RemoteSession
 *   - Workspace tracking (which IDE is active, which folder is open)
 *   - Message history tracking (for LLM context)
 *   - Idle session expiration
 *
 * [v5.0] LIVA Remote Control Hub
 */

import { EventEmitter } from "node:events";
import { logger } from "../utils/logger";
import type { NormalizedMessage } from "../channels/ChannelNormalizer";

// ===========================
// Types
// ===========================

export type IDESource = "antigravity" | "vscode" | null;

export interface RemoteSession {
    /** Unique session ID format: `${channel}_${senderId}` */
    id: string;
    /** Original sender ID */
    senderId: string;
    /** Originating channel */
    channel: string;
    /** Currently active IDE for this session */
    activeIDE: IDESource;
    /** Current project workspace path */
    projectPath: string;
    /** Recent conversation history */
    messageHistory: NormalizedMessage[];
    /** Creation timestamp */
    createdAt: number;
    /** Last activity timestamp */
    lastActiveAt: number;
}

// ===========================
// SessionOrchestrator
// ===========================

export class SessionOrchestrator extends EventEmitter {
    readonly #sessions = new Map<string, RemoteSession>();
    readonly #maxHistoryLength = 50;
    readonly #idleTimeoutMs = 1000 * 60 * 60 * 24; // 24 hours
    #gcTimer: ReturnType<typeof setInterval> | null = null;

    constructor() {
        super();
        // Start Garbage Collection for idle sessions
        this.#gcTimer = setInterval(() => this.#cleanupIdleSessions(), 1000 * 60 * 60);
        this.#gcTimer.unref(); // Don't block Node.js exit
    }

    // ═══════════════════════════════════════
    //  Session Management
    // ═══════════════════════════════════════

    /**
     * Get an existing session or create a new one for the user.
     */
    public getOrCreateSession(senderId: string, channel: string): RemoteSession {
        const sessionId = this.#generateSessionId(channel, senderId);
        
        let session = this.#sessions.get(sessionId);
        if (!session) {
            session = {
                id: sessionId,
                senderId,
                channel,
                activeIDE: null,
                projectPath: "/",
                messageHistory: [],
                createdAt: Date.now(),
                lastActiveAt: Date.now()
            };
            this.#sessions.set(sessionId, session);
            logger.info(`[Session] 🆕 Created new session: ${sessionId}`);
        } else {
            session.lastActiveAt = Date.now();
        }

        return session;
    }

    /**
     * Update the active IDE and workspace path for a session.
     */
    public switchWorkspace(sessionId: string, ide: IDESource, projectPath: string): void {
        const session = this.#sessions.get(sessionId);
        if (session) {
            session.activeIDE = ide;
            session.projectPath = projectPath;
            session.lastActiveAt = Date.now();
            logger.info(`[Session] 🔄 ${sessionId} switched to ${ide} at ${projectPath}`);
            this.emit("workspace_changed", session);
        }
    }

    // ═══════════════════════════════════════
    //  History Management
    // ═══════════════════════════════════════

    /**
     * Append a message to the session's history.
     */
    public appendMessage(sessionId: string, message: NormalizedMessage): void {
        const session = this.#sessions.get(sessionId);
        if (!session) return;

        session.messageHistory.push(message);
        session.lastActiveAt = Date.now();

        // Evict old messages to save memory
        if (session.messageHistory.length > this.#maxHistoryLength) {
            session.messageHistory.shift(); // Remove oldest
        }
    }

    /**
     * Get recent message history.
     */
    public getSessionHistory(sessionId: string): NormalizedMessage[] {
        return this.#sessions.get(sessionId)?.messageHistory || [];
    }

    /**
     * Clear message history for a session.
     */
    public clearHistory(sessionId: string): void {
        const session = this.#sessions.get(sessionId);
        if (session) {
            session.messageHistory = [];
            session.lastActiveAt = Date.now();
        }
    }

    // ═══════════════════════════════════════
    //  System
    // ═══════════════════════════════════════

    public get activeSessionCount(): number {
        return this.#sessions.size;
    }

    public dispose(): void {
/* istanbul ignore next */
        if (this.#gcTimer) {
            clearInterval(this.#gcTimer);
            this.#gcTimer = null;
        }
        this.#sessions.clear();
    }

    // ═══════════════════════════════════════
    //  Private
    // ═══════════════════════════════════════

    #generateSessionId(channel: string, senderId: string): string {
        return `${channel}_${senderId}`;
    }

    #cleanupIdleSessions(): void {
        const now = Date.now();
        let evicted = 0;

        for (const [id, session] of this.#sessions.entries()) {
            if (now - session.lastActiveAt > this.#idleTimeoutMs) {
                this.#sessions.delete(id);
                evicted++;
            }
        }

        if (evicted > 0) {
            logger.info(`[Session] 🧹 Evicted ${evicted} idle sessions.`);
        }
    }
}
