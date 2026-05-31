import { describe, it, expect } from "vitest";
import { DatabaseSync } from "node:sqlite";
import * as sqliteVec from "sqlite-vec";

describe("QuantizationAudit", () => {
    it("should measure the semantic distance precision loss between raw Float32 and INT8 quantized vector retrieval in sqlite-vec", () => {
        const db = new DatabaseSync(":memory:", { allowExtension: true });
        sqliteVec.load(db);

        const DIM = 384;
        const NUM_VECTORS = 100;
        const TOP_K = 10;

        // 1. Create table for float32 vectors and int8 vectors
        db.exec(`
            CREATE VIRTUAL TABLE vec_f32 USING vec0(
                embedding float[${DIM}]
            );
            CREATE VIRTUAL TABLE vec_int8 USING vec0(
                embedding int8[${DIM}]
            );
            CREATE TABLE vec_meta (
                id INTEGER PRIMARY KEY,
                content TEXT
            );
        `);

        // 2. Generate random unit vectors of dimension 384
        const vectors: number[][] = [];
        for (let i = 0; i < NUM_VECTORS; i++) {
            const vec = Array.from({ length: DIM }, () => Math.random() * 2 - 1);
            // Normalize to unit vector
            const norm = Math.sqrt(vec.reduce((sum, val) => sum + val * val, 0));
            const unitVec = vec.map(val => val / norm);
            vectors.push(unitVec);

            const blob = new Uint8Array(new Float32Array(unitVec).buffer);
            // Insert into SQLite f32 table
            db.prepare("INSERT INTO vec_f32(rowid, embedding) VALUES (?, ?)").run(BigInt(i + 1), blob);
            // Insert into SQLite int8 table (quantized)
            db.prepare("INSERT INTO vec_int8(rowid, embedding) VALUES (?, vec_quantize_int8(?, 'unit'))").run(BigInt(i + 1), blob);
            db.prepare("INSERT INTO vec_meta(id, content) VALUES (?, ?)").run(BigInt(i + 1), `Vector Content ${i + 1}`);
        }

        // 3. Perform a query search and compare float32 vs int8 quantized distance
        // Generate a random query unit vector
        const qVec = Array.from({ length: DIM }, () => Math.random() * 2 - 1);
        const qNorm = Math.sqrt(qVec.reduce((sum, val) => sum + val * val, 0));
        const queryUnitVec = qVec.map(val => val / qNorm);
        const qBlob = new Uint8Array(new Float32Array(queryUnitVec).buffer);

        // A. Search using raw Float32 embedding distance
        const sqlF32 = `
            SELECT rowid, distance
            FROM vec_f32
            WHERE embedding MATCH ? AND k = ?
        `;
        const f32Rows = db.prepare(sqlF32).all(qBlob, NUM_VECTORS) as Array<{ rowid: number; distance: number }>;

        // B. Search using INT8 quantized embedding distance
        const sqlInt8 = `
            SELECT rowid, distance
            FROM vec_int8
            WHERE embedding MATCH vec_quantize_int8(?, 'unit') AND k = ?
        `;
        const int8Rows = db.prepare(sqlInt8).all(qBlob, NUM_VECTORS) as Array<{ rowid: number; distance: number }>;

        // Map distances by rowid
        const f32Distances = new Map<number, number>();
        f32Rows.forEach(r => f32Distances.set(Number(r.rowid), r.distance));

        const int8Distances = new Map<number, number>();
        int8Rows.forEach(r => int8Distances.set(Number(r.rowid), r.distance));

        // 4. Calculate error metrics
        let totalAbsoluteError = 0;
        let count = 0;

        for (let i = 1; i <= NUM_VECTORS; i++) {
            const distF32 = f32Distances.get(i);
            const distInt8 = int8Distances.get(i);
            if (distF32 !== undefined && distInt8 !== undefined) {
                // Correct cosine similarity conversion from non-squared L2 distance
                const simF32 = Math.max(0, 1.0 - (distF32 * distF32) / 2.0);
                const simInt8 = Math.max(0, 1.0 - ((distInt8 / 119.5) * (distInt8 / 119.5)) / 2.0);
                totalAbsoluteError += Math.abs(simF32 - simInt8);
                count++;
            }
        }

        const meanAbsoluteError = count > 0 ? totalAbsoluteError / count : 0;
        console.log(`[QuantizationAudit] Mean Absolute Error of Cosine Similarity: ${meanAbsoluteError.toFixed(5)}`);
        
        // Let's query vec_to_json to see what components of quantized vector are
        for (let j = 0; j < 1; j++) {
            const vecBlob = new Uint8Array(new Float32Array(vectors[j]).buffer);
            const sampleRow = db.prepare("SELECT vec_to_json(vec_quantize_int8(?, 'unit')) as json").get(vecBlob) as { json: string };
            const components: number[] = JSON.parse(sampleRow.json);
            const sumOfSquares = components.reduce((sum, val) => sum + val * val, 0);
            const maxVal = Math.max(...components.map(Math.abs));
            console.log(`[QuantizationAudit] Vector ${j} components: ${components.slice(0, 10).join(", ")}...`);
            console.log(`[QuantizationAudit] Vector ${j} max absolute value: ${maxVal}`);
            console.log(`[QuantizationAudit] Vector ${j} sum of squares: ${sumOfSquares} (norm: ${Math.sqrt(sumOfSquares).toFixed(4)})`);
        }
        
        // Log first 5 to see values
        for (let i = 0; i < 5; i++) {
            const f32Row = f32Rows[i];
            const int8Row = int8Rows[i];
            if (f32Row && int8Row) {
                console.log(`[Sample ${i}] F32 rowid=${f32Row.rowid} dist=${f32Row.distance} sim=${((2.0 - f32Row.distance)/2.0).toFixed(4)} | INT8 rowid=${int8Row.rowid} dist=${int8Row.distance}`);
            }
        }

        // Check top K overlap (Jaccard similarity of the top K sets)
        const topKF32 = new Set(f32Rows.slice(0, TOP_K).map(r => Number(r.rowid)));
        const topKInt8 = new Set(int8Rows.slice(0, TOP_K).map(r => Number(r.rowid)));

        let intersection = 0;
        topKF32.forEach(id => {
            if (topKInt8.has(id)) intersection++;
        });

        const jaccardSimilarity = intersection / (2 * TOP_K - intersection);
        console.log(`[QuantizationAudit] Top-${TOP_K} Jaccard Similarity (Rank Overlap): ${(jaccardSimilarity * 100).toFixed(2)}%`);

        // Assertions: 
        // 1. Mean Absolute Error should be reasonably small (e.g. < 0.05)
        expect(meanAbsoluteError).toBeLessThan(0.05);

        // 2. Rank overlap of top K should be high (e.g. > 50%)
        expect(jaccardSimilarity).toBeGreaterThan(0.5);

        db.close();
    });
});
