import { StructuredMemory } from "../src/memory/StructuredMemory";
import { performance } from "node:perf_hooks";
import * as path from "node:path";
import * as fs from "node:fs/promises";
import { logger } from "../src/utils/logger";

logger.level = "silent";

const BENCHMARK_DIR = path.join(process.cwd(), "data", "accuracy_test");

function cosineSimilarity(vecA: number[], vecB: number[]): number {
    let dotProduct = 0;
    let normA = 0;
    let normB = 0;
    for (let i = 0; i < vecA.length; i++) {
        dotProduct += vecA[i] * vecB[i];
        normA += vecA[i] * vecA[i];
        normB += vecB[i] * vecB[i];
    }
    if (normA === 0 || normB === 0) return 0;
    return dotProduct / (Math.sqrt(normA) * Math.sqrt(normB));
}

async function runAccuracyTest() {
    console.log("=========================================");
    console.log("🎯 LIVA-UHM ACCURACY BENCHMARK (INT8 vs Float32)");
    console.log("=========================================\n");

    try { await fs.rm(BENCHMARK_DIR, { recursive: true, force: true }); } catch (e) {}
    await fs.mkdir(BENCHMARK_DIR, { recursive: true });

    // Use a fresh memory instance
    const storePath = path.join(BENCHMARK_DIR, "structured_memory.sqlite");
    const memory = new StructuredMemory(storePath, "accuracy_tester");

    // 1. Generate 2000 random vectors
    console.log("Generating 2,000 random 384D vectors...");
    const DIM = 384;
    const vectors: { id: string; vec: number[] }[] = [];
    
    memory.db.exec("BEGIN TRANSACTION;");
    for (let i = 0; i < 2000; i++) {
        // Random values between -1 and 1
        const vec = Array.from({ length: DIM }, () => Math.random() * 2 - 1);
        vectors.push({ id: `vec_${i}`, vec });

        // Insert into vectors_meta
        memory.db.prepare(`
            INSERT INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, file_target, source_event_ids, created_at) 
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        `).run(`vec_${i}`, 'AXIOM', `Content ${i}`, 'General', 'Uncategorized', '[]', '', '[]', Date.now());
        
        const row = memory.db.prepare('SELECT id FROM vectors_meta WHERE vec_id = ?').get(`vec_${i}`) as { id: number };
        const blob = new Uint8Array(new Float32Array(vec).buffer);
        // Insert as INT8
        memory.db.prepare(`INSERT INTO vec_idx (rowid, embedding) VALUES (?, vec_quantize_int8(?, 'unit'))`).run(BigInt(row.id), blob);
    }
    memory.db.exec("COMMIT;");
    console.log("✅ Vectors inserted.\n");

    // 2. Perform Accuracy Check (Recall@5 and Recall@10)
    console.log("Testing Recall@5 and Recall@10 over 100 random queries...");
    let recall5Sum = 0;
    let recall10Sum = 0;
    
    // Pick 100 random query vectors from our set
    const numQueries = 100;
    for (let q = 0; q < numQueries; q++) {
        const queryTarget = vectors[Math.floor(Math.random() * vectors.length)];
        
        // --- Float32 Ground Truth (Exact Math) ---
        // Calculate true Cosine Similarity against all 2000 vectors
        const scores = vectors.map(v => ({
            id: v.id,
            score: cosineSimilarity(queryTarget.vec, v.vec)
        }));
        scores.sort((a, b) => b.score - a.score);
        const trueTop5 = scores.slice(0, 5).map(s => s.id);
        const trueTop10 = scores.slice(0, 10).map(s => s.id);

        // --- INT8 sqlite-vec Query ---
        const int8ResultsTop5 = memory.searchSimilarVectors(queryTarget.vec, 5).map(r => r.vecId);
        const int8ResultsTop10 = memory.searchSimilarVectors(queryTarget.vec, 10).map(r => r.vecId);

        // Calculate Intersection
        const intersect5 = trueTop5.filter(id => int8ResultsTop5.includes(id)).length;
        const intersect10 = trueTop10.filter(id => int8ResultsTop10.includes(id)).length;

        recall5Sum += intersect5 / 5;
        recall10Sum += intersect10 / 10;
    }

    const avgRecall5 = (recall5Sum / numQueries) * 100;
    const avgRecall10 = (recall10Sum / numQueries) * 100;

    console.log(`📊 INT8 vs Float32 Recall@5: ${avgRecall5.toFixed(2)}%`);
    console.log(`📊 INT8 vs Float32 Recall@10: ${avgRecall10.toFixed(2)}%`);
    
    if (avgRecall5 >= 95) {
        console.log("✅ Accuracy loss from INT8 Quantization is negligible! (< 5% error rate)");
    } else {
        console.log("⚠️ Accuracy loss is noticeable.");
    }

    console.log("\n=========================================");
    console.log("🏁 ACCURACY TEST COMPLETED.");
    console.log("=========================================");
    process.exit(0);
}

runAccuracyTest().catch(console.error);
