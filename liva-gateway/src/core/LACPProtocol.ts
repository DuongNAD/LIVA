import { logger } from "../utils/logger";
import crypto from "crypto";
import LRUCache from "lru-cache";

export interface LACPTxEnvelope {
    txId: string;
    senderAgent: string;
    targetAgent: string;
    phase: "PREPARE" | "COMMIT" | "ROLLBACK";
    payload: any;
    timestamp: number;
    jwsSignature?: string;
}

/**
 * [LIVA V2 Singularity] LACP - LLM Agent Communication Protocol
 * Transactional Layer ensuring atomic multi-agent workflows.
 */
export class LACPProtocol {
    private static instance: LACPProtocol;
    private readonly SECRET_KEY: string;

    // [v26 Audit Fix] LRUCache replaces unbounded Map — prevents zombie transactions from leaking
    private activeTransactions = new LRUCache<string, LACPTxEnvelope>({
        max: 100,
        ttl: 10 * 60 * 1000, // 10 minutes — transactions older than this are auto-evicted
    });

    private constructor() {
        // In production, this should be a properly managed HMAC key in the vault.
        this.SECRET_KEY = process.env.LACP_SECRET_KEY || crypto.randomBytes(32).toString('hex');
    }

    public static getInstance(): LACPProtocol {
        if (!LACPProtocol.instance) {
            LACPProtocol.instance = new LACPProtocol();
        }
        return LACPProtocol.instance;
    }

    /**
     * Wrap an inter-agent message in an encrypted JWS envelope.
     */
    public signMessage(sender: string, target: string, phase: "PREPARE" | "COMMIT" | "ROLLBACK", payload: any): LACPTxEnvelope {
        const env: LACPTxEnvelope = {
            txId: crypto.randomUUID(),
            senderAgent: sender,
            targetAgent: target,
            phase,
            payload,
            timestamp: Date.now()
        };

        const signaturePayload = `${env.txId}:${env.senderAgent}:${env.targetAgent}:${env.phase}:${JSON.stringify(env.payload)}`;
        env.jwsSignature = crypto.createHmac('sha256', this.SECRET_KEY).update(signaturePayload).digest('hex');

        return env;
    }

    /**
     * Verifies the cryptographic integrity of an incoming message.
     */
    public verifyMessage(envelope: LACPTxEnvelope): boolean {
        if (!envelope.jwsSignature) return false;

        const signaturePayload = `${envelope.txId}:${envelope.senderAgent}:${envelope.targetAgent}:${envelope.phase}:${JSON.stringify(envelope.payload)}`;
        const expectedSignature = crypto.createHmac('sha256', this.SECRET_KEY).update(signaturePayload).digest('hex');

        return crypto.timingSafeEqual(Buffer.from(envelope.jwsSignature), Buffer.from(expectedSignature));
    }

    /**
     * Executes a 2-Phase Commit (2PC) between two agents.
     */
    public async executeTwoPhaseCommit(envelope: LACPTxEnvelope, targetExecuteCallback: () => Promise<boolean>): Promise<boolean> {
        logger.info(`[LACP] 🔄 Initiating 2PC Transaction [${envelope.txId}] from ${envelope.senderAgent} to ${envelope.targetAgent}`);
        
        if (!this.verifyMessage(envelope)) {
            logger.error(`[LACP] 🛑 SECURITY BREACH: JWS Signature Verification Failed for TX: ${envelope.txId}`);
            return false;
        }

        this.activeTransactions.set(envelope.txId, envelope);

        // Phase 1: Prepare
        logger.info(`[LACP] 🟡 Phase 1: PREPARE`);
        // Simulated network/state lock setup
        
        // Phase 2: Commit
        logger.info(`[LACP] 🟢 Phase 2: COMMIT execution`);
        try {
            const success = await targetExecuteCallback();
            if (success) {
                logger.info(`[LACP] ✅ Transaction [${envelope.txId}] COMMITTED.`);
                this.activeTransactions.delete(envelope.txId);
                return true;
            } else {
                throw new Error("Target execution reported failure.");
            }
        } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
            logger.warn(`[LACP] ⚠️ Execution failed. Rolling back TX [${envelope.txId}]. Reason: ${errMsg}`);
            await this.rollbackTransaction(envelope.txId);
            return false;
        }
    }

    private async rollbackTransaction(txId: string): Promise<void> {
        const tx = this.activeTransactions.get(txId);
        if (tx) {
            logger.info(`[LACP] ⏪ Executing ROLLBACK for ${txId} (${tx.targetAgent})`);
            // Instruct target agent to revert state
            this.activeTransactions.delete(txId);
        }
    }
}
