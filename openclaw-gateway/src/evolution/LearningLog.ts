/**
 * LearningLog — Evolution Learning Memory (v19: sqlite-vec)
 * ==========================================================
 * Stores evolution attempt vectors (SUCCESS/DEAD-END) for recall.
 * Migrated to sqlite-vec via StructuredMemory.
 */
import { EmbeddingService } from "../services/EmbeddingService";
import { StructuredMemory } from "../memory/StructuredMemory";
import { logger } from "../utils/logger";

export interface LogEntry {
    id: string;
    vector: number[];
    timestamp: number;
    targetFile: string;
    action: string;
    asiContext: string;
    success: boolean;
    occurrence_count: number;
}

export class LearningLog {
    private readonly embeddingService: EmbeddingService;
    private structuredMemory: StructuredMemory | null = null;

    constructor(embeddingService?: EmbeddingService) {
        this.embeddingService = embeddingService ?? EmbeddingService.getInstance();
    }

    public async connect(): Promise<void> {
        if (!this.structuredMemory) {
            this.structuredMemory = await StructuredMemory.create("liva_core");
        }
    }

    /** Distill context to reduce noise in vectors */
    private distillContext(rawContext: string): string {
        if (!rawContext) return "Unknown error";
        let distilled = rawContext.replaceAll(/[A-Za-z]:\\[^\s]+\\src\\/gi, "src/");
        distilled = distilled.replaceAll(/\/home\/[^\s]+\/src\//gi, "src/");
        if (distilled.length > 2000) {
            distilled = distilled.substring(0, 2000) + "... (distilled)";
        }
        return distilled.trim();
    }

    public async recordAttempt(targetFile: string, action: string, asiContext: string, success: boolean): Promise<void> {
        if (!this.structuredMemory) await this.connect();
        if (!this.structuredMemory) return;

        const distilled = this.distillContext(asiContext);
        const text = `[${success ? 'SUCCESS' : 'DEAD-END'}] ${action} on ${targetFile}: ${distilled}`;
        const vec = await this.embeddingService.embed(text.substring(0, 500));

        this.structuredMemory.upsertVector({
            vecId: `evo_log_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
            type: success ? 'SUCCESS' : 'DEAD-END',
            content: text,
            vector: vec,
            domain: targetFile,
        });

        logger.debug(`[LearningLog] Recorded ${success ? 'SUCCESS' : 'FAIL'}: ${action} on ${targetFile}`);
    }

    /**
     * Retrieve relevant axioms for evolution planning.
     */
    public async getRelevantAxioms(targetFile: string, proposedAction: string, topK: number = 5): Promise<string> {
        if (!this.structuredMemory) await this.connect();
        if (!this.structuredMemory?.vecReady) {
            return "<system_memory>\n  [No evolution memory available]\n</system_memory>";
        }

        const queryText = `${proposedAction} ${targetFile}`;
        const vec = await this.embeddingService.embed(queryText);
        const results = this.structuredMemory.searchSimilarVectors(vec, topK);

        if (results.length === 0) {
            return "<system_memory>\n  [No relevant evolution experiences found]\n</system_memory>";
        }

        const successes = results.filter(r => r.type === 'SUCCESS').map(r => `  <success>${r.content}</success>`);
        const failures = results.filter(r => r.type === 'DEAD-END').map(r => `  <failure>${r.content}</failure>`);

        return `<system_memory>\n${successes.join('\n')}\n${failures.join('\n')}\n</system_memory>`;
    }
}
