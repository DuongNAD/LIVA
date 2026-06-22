import { logger } from "../../utils/logger";
import { PersonalityEvolution } from "../PersonalityEvolution";
import type { PersonalityState } from "../PersonalityEvolution";
import type { EventRepository, EventBrick, TurnNode } from "../EventRepository";
import type { GraphRepository } from "../GraphRepository";
import type { StructuredMemory } from "../StructuredMemory";
import type { MemoryIO } from "./MemoryIO";
import type { MemorySearch } from "./MemorySearch";

export class MemoryConsolidator {
    constructor(
        private readonly parent: StructuredMemory,
        private readonly eventRepo: EventRepository,
        private readonly graphRepo: GraphRepository,
        private readonly io: MemoryIO,
        private readonly search: MemorySearch
    ) {}

    public queueMemoryTouch(eventId: string): void {
        this.eventRepo.queueMemoryTouch(eventId);
    }

    public async flushTouchQueue(): Promise<void> {
        await this.parent.ensureInitialized();
        return this.eventRepo.flushTouchQueue();
    }

    public async insertEvent(event: EventBrick): Promise<void> {
        await this.parent.ensureInitialized();
        await this.eventRepo.insertEvent(event);
        try {
            await PersonalityEvolution.evolveFromTurn(
                this.parent.db,
                this.parent.dbBridge,
                this.parent.agentId,
                event.rawUserMsg || "",
                event.psi?.sentiment,
                event.psi?.intent
            );
        } catch (err: unknown) {
            logger.error(`[StructuredMemory] Error evolving personality: ${err}`);
        }
    }

    public getPersonalityStateSync(): PersonalityState {
        return PersonalityEvolution.getPersonalityState(this.parent.db, this.parent.agentId);
    }

    public async getPersonalityState(): Promise<PersonalityState> {
        await this.parent.ensureInitialized();
        const row = await this.parent.dbBridge.prepare("SELECT * FROM personality_state WHERE agentId = ?").get(this.parent.agentId) as PersonalityState | null;
        if (!row) {
            return this.getPersonalityStateSync();
        }
        return {
            agentId: row.agentId,
            valence: row.valence,
            arousal: row.arousal,
            friendliness: row.friendliness,
            verbosity: row.verbosity,
            assertiveness: row.assertiveness,
            updatedAt: row.updatedAt
        };
    }

    public async updatePersonalityState(state: Partial<PersonalityState>): Promise<void> {
        await this.parent.ensureInitialized();
        await PersonalityEvolution.updatePersonalityState(this.parent.dbBridge, this.parent.agentId, state);
    }

    public async getUnconsolidatedEvents(): Promise<EventBrick[]> {
        await this.parent.ensureInitialized();
        return this.eventRepo.getUnconsolidatedEvents();
    }

    public async getUnconsolidatedCount(): Promise<number> {
        await this.parent.ensureInitialized();
        return this.eventRepo.getUnconsolidatedCount();
    }

    public async markConsolidated(eventIds: string[]): Promise<void> {
        await this.parent.ensureInitialized();
        await this.eventRepo.markConsolidated(eventIds);
    }

    public async markDLQ(eventIds: string[]): Promise<void> {
        await this.parent.ensureInitialized();
        await this.eventRepo.markDLQ(eventIds);
    }

    public async incrementRetryCount(eventIds: string[]): Promise<void> {
        await this.parent.ensureInitialized();
        await this.eventRepo.incrementRetryCount(eventIds);
    }

    public async gcOldEvents(retentionDays?: number): Promise<number> {
        await this.parent.ensureInitialized();
        return this.eventRepo.gcOldEvents(retentionDays);
    }

    public async deleteAllEvents(): Promise<void> {
        await this.parent.ensureInitialized();
        await this.eventRepo.deleteAllEvents();
    }

    public async insertTurnNode(turnId: string, temporal_anchor: number, userMsg: string, aiReply: string): Promise<void> {
        await this.parent.ensureInitialized();
        await this.eventRepo.insertTurnNode(turnId, temporal_anchor, userMsg, aiReply);
    }

    public async getTurnsByTimeRange(fromTs: number, toTs: number): Promise<TurnNode[]> {
        await this.parent.ensureInitialized();
        return this.eventRepo.getTurnsByTimeRange(fromTs, toTs);
    }

    public async getTurnsByIds(turnIds: string[]): Promise<TurnNode[]> {
        await this.parent.ensureInitialized();
        return this.eventRepo.getTurnsByIds(turnIds);
    }

    public get graph(): GraphRepository {
        return this.graphRepo;
    }

    public async applyMemoryDecay(decayRate: number = 0.1): Promise<{ decayed: number; archived: number }> {
        const factDecay = await this.io.applyFactDecay(decayRate);
        const vecDecay = await this.search.applyVectorDecay(decayRate);

        return {
            decayed: factDecay.decayed + vecDecay.decayed,
            archived: factDecay.archived + vecDecay.archived
        };
    }

    public async close(): Promise<void> {
        await this.eventRepo.flushAndStop();
    }
}
