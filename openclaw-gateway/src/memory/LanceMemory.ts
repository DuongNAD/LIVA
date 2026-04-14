import * as lancedb from "@lancedb/lancedb";
import * as path from "path";
import axios from "axios";

export class LanceMemoryManager {
    private db: lancedb.Connection | null = null;
    private table: lancedb.Table | null = null;
    
    // We assume the EXPERT_API_URL or some embedding endpoint exists
    private embeddingUrl = "http://127.0.0.1:8001/v1/embeddings"; 

    async connect() {
        const dbDir = path.join(process.cwd(), "data", "lancedb");
        this.db = await lancedb.connect(dbDir);
        
        try {
            this.table = await this.db.openTable("episodic_reflexion");
        } catch {
            // Create table with empty data, LanceDB requires schema inference from first data
            // We'll define schema on first insert
        }
    }

    private async getEmbeddings(text: string): Promise<number[]> {
        try {
            const res = await axios.post(this.embeddingUrl, {
                input: text,
                model: "expert-embed" // placeholder
            });
            return res.data.data[0].embedding;
        } catch(e) {
            // Fallback mock array if no embedding model is loaded physically yet
            return new Array(768).fill(0.01);
        }
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
            this.table = await this.db!.createTable("episodic_reflexion", data);
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
            const results = await this.table.vectorSearch(queryVector).limit(limit).execute();
            return results.map(r => `[${r.type}] (Target: ${r.fileTarget}): ${r.text}`);
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
            const results = await this.table.filter("type != 'AXIOM'").execute();
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
