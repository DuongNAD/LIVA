import * as lancedb from "@lancedb/lancedb";
import * as path from "path";
// V14: Bỏ axios, tích hợp thẳng Bộ tạo sinh không gian Vector Xenova.
import { pipeline, FeatureExtractionPipeline } from "@xenova/transformers";

export class LanceMemoryManager {
    private db: lancedb.Connection | null = null;
    private table: lancedb.Table | null = null;
    
    // V14: Tiến hóa sang Native Local Embedding
    private embedder: FeatureExtractionPipeline | null = null;

    async connect() {
        const dbDir = path.join(process.cwd(), "data", "lancedb");
        this.db = await lancedb.connect(dbDir);
        
        try {
            console.log("\x1b[36m💿 [LanceDB]: Đang khởi động não nhúng Local L6-V2 (384D) để trị bệnh Mù Trí Nhớ Tiến Hóa...\x1b[0m");
            this.embedder = await pipeline("feature-extraction", "Xenova/all-MiniLM-L6-v2");
        } catch(e) {
            console.error("Lỗi tải não nhúng Xenova:", e);
        }

        try {
            // V14: Ép nổ Database cũ (vốn bị lỗi 768D giả lập) và tạo Database mới tinh
            this.table = await this.db.openTable("episodic_reflexion_v14");
        } catch {
            // 
        }
    }

    private async getEmbeddings(text: string): Promise<number[]> {
        if (this.embedder) {
            try {
                const output = await this.embedder(text, {
                    pooling: "mean",
                    normalize: true,
                });
                return Array.from(output.data);
            } catch (e) {
                // Ignore
            }
        }
        return new Array(384).fill(0.01); 
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
        } catch(e) {}
    }
}
