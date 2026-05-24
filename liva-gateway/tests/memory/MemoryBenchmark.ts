import { StructuredMemory } from "../../src/memory/StructuredMemory";
import * as fs from "node:fs";
import * as path from "node:path";

const AGENT_ID = "benchmark_agent_liva_brutal";
const DB_PATH = path.join(process.cwd(), "data", "agents", AGENT_ID, "structured_memory.sqlite");
const DIMENSION = 768;

process.env.LIVA_ENCRYPTION_KEY = "0123456789abcdef0123456789abcdef"; // 32-byte key for encryption engine

// Utility to generate random vectors of unit length (needed for L2 normalized cosine distance)
function generateRandomVector(dim: number): number[] {
    const vec = Array.from({ length: dim }, () => Math.random() * 2 - 1);
    const len = Math.sqrt(vec.reduce((sum, val) => sum + val * val, 0));
    return vec.map(val => val / (len || 1));
}

async function runBenchmark() {
    console.log("=== STARTING LIVA BRUTAL MEMORY ARCHITECTURE BENCHMARK ===");
    console.log(`Database path: ${DB_PATH}`);
    console.log(`Vector dimension: ${DIMENSION}`);

    // Cleanup previous DB
    try {
        if (fs.existsSync(DB_PATH)) {
            fs.unlinkSync(DB_PATH);
            console.log("Cleaned up old benchmark database.");
        }
        const dir = path.dirname(DB_PATH);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }
    } catch (e) {
        console.warn("Error cleaning up old DB:", e);
    }

    // 1. Initialize Memory
    const startTime = Date.now();
    const memory = await StructuredMemory.create(AGENT_ID, DB_PATH);
    memory.initVecDimension(DIMENSION);
    console.log(`Memory initialized in ${Date.now() - startTime}ms`);

    const stats = {
        totalFactInserts: 0,
        totalVectorInserts: 0,
        totalKnnSearches: 0,
        totalHybridSearches: 0,
        totalGraphNodesUpserted: 0,
        totalGraphEdgesUpserted: 0,
        totalGraphSearches: 0,
        errors: 0,
        errorTypes: {} as Record<string, number>,
    };

    function logError(err: any) {
        stats.errors++;
        const errMsg = err.message || String(err);
        stats.errorTypes[errMsg] = (stats.errorTypes[errMsg] || 0) + 1;
    }

    // 2. Pre-population of Vectors and Nodes to have realistic search payload
    console.log("\n--- PHASE 1: Pre-populating Graph Nodes, Edges and Vectors ---");
    const nodeCount = 500;
    const edgeCount = 1000;
    const vectorCount = 1000;

    // Insert 500 graph nodes
    for (let i = 0; i < nodeCount; i++) {
        memory.graph.upsertNode({
            id: `node_${i}`,
            label: i % 2 === 0 ? "CONCEPT" : "ENTITY",
            properties: JSON.stringify({ index: i, desc: `Prepopulated node number ${i}` })
        });
        stats.totalGraphNodesUpserted++;
    }

    // Insert 1000 graph edges
    for (let i = 0; i < edgeCount; i++) {
        const source = `node_${Math.floor(Math.random() * nodeCount)}`;
        const target = `node_${Math.floor(Math.random() * nodeCount)}`;
        if (source !== target) {
            memory.graph.upsertEdge({
                source,
                target,
                relation: i % 3 === 0 ? "RELATES_TO" : i % 3 === 1 ? "PART_OF" : "DEPENDS_ON",
                weight: Math.random(),
                obsolete: 0
            });
            stats.totalGraphEdgesUpserted++;
        }
    }

    // Insert 1000 vectors
    const prepopulatedVectors = Array.from({ length: vectorCount }, (_, i) => ({
        vecId: `vec_pre_${i}`,
        type: i % 2 === 0 ? "ANCHOR" : "AXIOM",
        content: `This is prepopulated vector text number ${i} which discusses AI, Memory, LIVA and Graph Databases.`,
        vector: generateRandomVector(DIMENSION)
    }));
    memory.upsertVectorsBatch(prepopulatedVectors);
    stats.totalVectorInserts += vectorCount;

    console.log(`Pre-population complete:`);
    console.log(`- ${stats.totalGraphNodesUpserted} Graph Nodes`);
    console.log(`- ${stats.totalGraphEdgesUpserted} Graph Edges`);
    console.log(`- ${stats.totalVectorInserts} Vectors`);

    // 3. Brutal Concurrency Load
    console.log("\n--- PHASE 2: Brutal Concurrency Stress Test ---");
    const concurrency = 50; 
    const operationsPerWorker = 1000; // 50,000 total operations!
    
    console.log(`Simulating ${concurrency} parallel workers running ${operationsPerWorker} mixed operations each...`);
    const p2Start = Date.now();

    const workers = Array.from({ length: concurrency }, async (_, workerId) => {
        for (let i = 0; i < operationsPerWorker; i++) {
            const opType = i % 7;
            try {
                if (opType === 0) {
                    // 1. Graph Node Upsert
                    memory.graph.upsertNode({
                        id: `node_worker_${workerId}_${i % 100}`,
                        label: "DYNAMIC",
                        properties: JSON.stringify({ updatedBy: workerId, step: i })
                    });
                    stats.totalGraphNodesUpserted++;
                } else if (opType === 1) {
                    // 2. Graph Edge Upsert
                    const source = `node_worker_${workerId}_${Math.floor(Math.random() * 100)}`;
                    const target = `node_${Math.floor(Math.random() * nodeCount)}`;
                    memory.graph.upsertEdge({
                        source,
                        target,
                        relation: "CREATED_DURING_TEST",
                        weight: Math.random(),
                        obsolete: 0
                    });
                    stats.totalGraphEdgesUpserted++;
                } else if (opType === 2) {
                    // 3. Multi-hop Search (Recursive CTE)
                    const startNode = `node_${Math.floor(Math.random() * nodeCount)}`;
                    const res = memory.graph.multiHopSearch(startNode, 3);
                    stats.totalGraphSearches++;
                } else if (opType === 3) {
                    // 4. Save Encrypted Fact (locks database for write)
                    memory.setFact(`key_w_${workerId}_${i % 200}`, `value_w_${workerId}_${i}`, {
                        ttlDays: 1,
                        source: "stress_test",
                        category: "Brutal"
                    });
                    stats.totalFactInserts++;
                } else if (opType === 4) {
                    // 5. Touch Fact
                    memory.touchFact(`key_w_${workerId}_${Math.floor(Math.random() * 200)}`);
                } else if (opType === 5) {
                    // 6. KNN Vector Search (L2/Cosine Index matching)
                    const vec = generateRandomVector(DIMENSION);
                    const res = memory.searchSimilarVectors(vec, 10);
                    stats.totalKnnSearches++;
                } else if (opType === 6) {
                    // 7. Hybrid RAG Search (FTS5 + KNN + RRF)
                    const vec = generateRandomVector(DIMENSION);
                    const res = memory.searchHybridVectors("Discusses AI and LIVA", vec, 5);
                    stats.totalHybridSearches++;
                }
            } catch (e) {
                logError(e);
            }

            // Yield control back to V8 event loop periodically to prevent event loop starvation
            if (i % 20 === 0) {
                await new Promise(resolve => setImmediate(resolve));
            }
        }
    });

    await Promise.all(workers);
    const p2Duration = Date.now() - p2Start;
    const totalOps = concurrency * operationsPerWorker;
    console.log(`\n🎉 Executed ${totalOps} brutal concurrent operations in ${p2Duration}ms!`);
    console.log(`Throughput: ${(totalOps / (p2Duration / 1000)).toFixed(2)} ops/sec`);

    // 4. Memory Footprint & Safety Check
    console.log("\n--- PHASE 3: Memory Footprint & Safety Check ---");
    const memoryUsage = process.memoryUsage();
    console.log(`Heap Used: ${(memoryUsage.heapUsed / 1024 / 1024).toFixed(2)} MB`);
    console.log(`Heap Total: ${(memoryUsage.heapTotal / 1024 / 1024).toFixed(2)} MB`);
    console.log(`RSS: ${(memoryUsage.rss / 1024 / 1024).toFixed(2)} MB`);

    // Close Connection
    memory.close();
    console.log("\n--- BENCHMARK RESULTS SUMMARY ---");
    console.log(`- Graph Nodes Upserted: ${stats.totalGraphNodesUpserted}`);
    console.log(`- Graph Edges Upserted: ${stats.totalGraphEdgesUpserted}`);
    console.log(`- Graph Multi-hop Searches: ${stats.totalGraphSearches}`);
    console.log(`- Encrypted Fact Writes: ${stats.totalFactInserts}`);
    console.log(`- Vector Batch Inserts: ${stats.totalVectorInserts}`);
    console.log(`- KNN Searches: ${stats.totalKnnSearches}`);
    console.log(`- Hybrid RAG Searches: ${stats.totalHybridSearches}`);
    console.log(`- Total Errors: ${stats.errors}`);

    if (stats.errors > 0) {
        console.log("❌ Benchmark finished with errors. Error details:");
        console.dir(stats.errorTypes);
    } else {
        console.log("🏆 SUCCESS: Liva Memory Architecture handled 50,000 brutal concurrent operations (Recursive CTEs, RAG, Quantized Vector Math, encryption) with ZERO failures!");
    }
    console.log("=================================================");
}

runBenchmark().catch(err => {
    console.error("Benchmark failed with fatal error:", err);
});
