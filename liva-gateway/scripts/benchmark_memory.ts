import { StructuredMemory } from "../src/memory/StructuredMemory";
import { randomUUID } from "node:crypto";
import { performance } from "node:perf_hooks";

async function run() {
    console.log("Initializing StructuredMemory for benchmark (Agent: benchmark_agent)...");
    const mem = await StructuredMemory.create('benchmark_agent');
    mem.initVecDimension(384);
    
    console.log("Memory DB initialized successfully.");

    const BATCH_SIZE = 5000;
    const TOTAL_RECORDS = 100000; // Mocking 100,000 records
    
    console.log(`\n--- PHASE 1: DATA INGESTION (${TOTAL_RECORDS} records) ---`);
    const dummyVector = new Array(384).fill(0).map(() => Math.random());

    const words = ["apple", "banana", "server", "gateway", "ai", "database", "memory", "latency", "project", "test", "liva", "agent", "query", "vector", "index"];
    function getRandomContent() {
        return Array.from({length: 15}, () => words[Math.floor(Math.random() * words.length)]).join(" ");
    }

    // Check if we already have 100k vectors, to skip ingestion if rerunning
    if (mem.vectorCount >= TOTAL_RECORDS) {
        console.log(`Database already has ${mem.vectorCount} vectors. Skipping ingestion.`);
    } else {
        console.log(`Starting data ingestion of ${TOTAL_RECORDS} records in batches of ${BATCH_SIZE}...`);
        const startIngest = performance.now();
        
        for (let i = mem.vectorCount; i < TOTAL_RECORDS; i += BATCH_SIZE) {
            const vectorBatch = [];
            
            // 1. Insert L1 Turn Layer (Events) via SQLite transaction for speed
            mem.db.exec("BEGIN");
            const insertEventStmt = mem.db.prepare(`
                INSERT OR REPLACE INTO events 
                (eventId, timestamp, phi_facts, phi_entities, psi_sentiment, psi_intent, psi_relational, rawUserMsg, rawAiReply, consolidated, domain, category, trace_keywords, last_accessed_at, consolidation_status, retry_count)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, 'pending', 0)
            `);
            
            for (let j = 0; j < BATCH_SIZE && i + j < TOTAL_RECORDS; j++) {
                const id = randomUUID();
                const content = getRandomContent();
                
                // L1 Event Insert
                insertEventStmt.run(
                    id,
                    Date.now(),
                    "[]",
                    "[]",
                    "neutral",
                    "inform",
                    "none",
                    content,
                    "AI reply placeholder",
                    "General",
                    "Uncategorized",
                    "[]",
                    0
                );

                // Prepare L2 Vector Insert
                vectorBatch.push({
                    vecId: "vec_" + id,
                    type: "AXIOM",
                    content: content + " - " + id,
                    // Slightly modify the dummy vector to have variety
                    vector: dummyVector.map(v => v * Math.random()), 
                    domain: "General",
                    category: "Uncategorized",
                    traceKeywords: ["benchmark"],
                    sourceEventIds: [id]
                });
            }
            mem.db.exec("COMMIT");
            
            // 2. Insert L2 Event Layer (Vectors + FTS5) via Repository
            // This handles vectors_meta, vec_idx (KNN), and vectors_fts (FTS5)
            mem.upsertVectorsBatch(vectorBatch);
            
            console.log(`Ingested ${i + vectorBatch.length} / ${TOTAL_RECORDS} records...`);
        }
        const endIngest = performance.now();
        console.log(`✅ Ingestion completed in ${((endIngest - startIngest)/1000).toFixed(2)} seconds.`);
    }

    // --- PHASE 2: BENCHMARKING ---
    console.log(`\n--- PHASE 2: QUERY LATENCY BENCHMARK (Dataset Size: ${mem.vectorCount} records) ---`);
    console.log("Measuring average latency over 100 queries...\n");

    const searchVector = dummyVector.map(v => v * Math.random());
    const searchQueryText = "server ai database memory";

    // Warm up
    mem.searchSimilarVectors(searchVector, 5);
    mem.searchHybridVectors(searchQueryText, searchVector, 5);

    const RUNS = 100;

    // 1. Vector Search Latency (sqlite-vec KNN)
    let vecStart = performance.now();
    for(let i = 0; i < RUNS; i++) {
        mem.searchSimilarVectors(searchVector, 10); // topK=10
    }
    let vecEnd = performance.now();
    const vecAvg = ((vecEnd - vecStart) / RUNS).toFixed(2);
    console.log(`🟢 Vector Search (KNN) Latency     : ${vecAvg} ms / query`);

    // 2. Simple SQL FTS5 Search Latency
    let ftsStart = performance.now();
    for(let i = 0; i < RUNS; i++) {
        const stmt = mem.db.prepare("SELECT rowid FROM vectors_fts WHERE content MATCH ? LIMIT 10");
        stmt.all("server* OR ai* OR database*");
    }
    let ftsEnd = performance.now();
    const ftsAvg = ((ftsEnd - ftsStart) / RUNS).toFixed(2);
    console.log(`🟢 FTS5 Only Search Latency        : ${ftsAvg} ms / query`);

    // 3. Hybrid Search Latency (KNN + FTS5 RRF)
    let hybridStart = performance.now();
    for(let i = 0; i < RUNS; i++) {
        mem.searchHybridVectors(searchQueryText, searchVector, 10);
    }
    let hybridEnd = performance.now();
    const hybridAvg = ((hybridEnd - hybridStart) / RUNS).toFixed(2);
    console.log(`🔴 Hybrid Search (KNN + FTS5 RRF)  : ${hybridAvg} ms / query`);

    console.log("\nDone.");
    process.exit(0);
}

run().catch(console.error);
