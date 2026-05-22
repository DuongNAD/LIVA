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

        // Vector Deduplication Check (Cosine Similarity Threshold >= 0.95)
        const similar = this.structuredMemory.searchSimilarVectors(vec, 1, success ? 'SUCCESS' : 'DEAD-END');
        if (similar.length > 0) {
            const bestMatch = similar[0];
            const similarity = (2.0 - (bestMatch.distance ?? 2.0)) / 2.0;
            if (similarity >= 0.95) {
                logger.debug(`[LearningLog] Skipping duplicate attempt recording (similarity: ${similarity.toFixed(4)} >= 0.95)`);
                return;
            }
        }

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

        const successes = results.filter(r => r.type === 'SUCCESS').map(r => `    ${r.content}`);
        const failures = results.filter(r => r.type === 'DEAD-END').map(r => `    ${r.content}`);

        let memoryString = "<system_memory>\n";
        if (successes.length > 0) {
            memoryString += `  <best_practices>\n${successes.join('\n')}\n  </best_practices>\n`;
        }
        if (failures.length > 0) {
            memoryString += `  <anti_patterns>\n${failures.join('\n')}\n  </anti_patterns>\n`;
        }
        memoryString += "</system_memory>";

        return memoryString;
    }
}
