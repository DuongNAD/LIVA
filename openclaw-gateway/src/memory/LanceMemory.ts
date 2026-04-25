import * as lancedb from "@lancedb/lancedb";
import * as path from "node:path";
// V16: Migrated to shared EmbeddingService singleton (replaces per-class pipeline loading)
import { EmbeddingService } from "../services/EmbeddingService";
import { logger } from "../utils/logger";

export class LanceMemoryManager {
    private db: lancedb.Connection | null = null;
    private table: lancedb.Table | null = null;
    
    // V16: Shared EmbeddingService (Singleton — single model load for entire system)
    private readonly embeddingService: EmbeddingService;

    constructor(embeddingService?: EmbeddingService) {
        this.embeddingService = embeddingService ?? EmbeddingService.getInstance();
    }

    async connect() {
        const dbDir = path.join(process.cwd(), "data", "lancedb");
        this.db = await lancedb.connect(dbDir);
        
        try {
            logger.info("[LanceDB] 💿 Initializing via shared EmbeddingService (384D)...");
            await this.embeddingService.ensureReady();
            logger.info("[LanceDB] ✅ EmbeddingService connected.");
        } catch(e) {
            logger.error(`[LanceDB] Lỗi kết nối EmbeddingService: ${e}`);
        }

        try {
            // V14: Ép nổ Database cũ (vốn bị lỗi 768D giả lập) và tạo Database mới tinh
            this.table = await this.db.openTable("episodic_reflexion_v14");
        } catch {
            // 
        }
    }

    private async getEmbeddings(text: string): Promise<number[]> {
        return this.embeddingService.embed(text);
    }

    async addMemory(type: "DEAD-END" | "SUCCESS" | "AXIOM", content: string, fileTarget: string) {
        if (!this.db) await this.connect();
        
        const timestamp = Date.now();
        const vector = await this.getEmbeddings(content);
        
        const data = [{
            vector,
            text: content,
            type,
            fileTarget,
            timestamp
        }];

        if (!this.table) {
            this.table = await this.db!.createTable("episodic_reflexion_v14", data);
        } else {
            await this.table.add(data);
        }
    }

    async searchMemory(query: string, limit: number = 3): Promise<string[]> {
        if (!this.db) await this.connect();
        if (!this.table) return [];

        const queryVector = await this.getEmbeddings(query);
        // LanceDB Hybrid Search (Dense + FTS optionally if indexed)
        // For node.js native lancedb, FTS index requires creating index. We fall back to dense first.
        try {
            const results = await this.table.vectorSearch(queryVector).limit(limit).toArray();
            return results.map((r: any) => `[${r.type}] (Target: ${r.fileTarget}): ${r.text}`);
        } catch(e) {
            return [];
        }
    }

    async getAllEpisodicMemories(): Promise<any[]> {
        if (!this.db) await this.connect();
        if (!this.table) return [];
        try {
            // Get all memories that are not AXIOM
            // Currently LanceDB node supports SQL-like filters
            const results = await this.table.query().where("type != 'AXIOM'").toArray();
            return results;
        } catch(e) {
            return [];
        }
    }

    async clearEpisodicMemories() {
        if (!this.db || !this.table) return;
        try {
            await this.table.delete("type != 'AXIOM'");
        } catch (e) { void e; }
    }
}
