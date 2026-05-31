import { StructuredMemory } from "../src/memory/StructuredMemory";
import { SemanticCache } from "../src/memory/SemanticCache";
import { performance } from "node:perf_hooks";
import * as path from "node:path";
import * as fs from "node:fs/promises";
import * as fsSync from "node:fs";
import { logger } from "../src/utils/logger";
import { Worker } from "node:worker_threads";
import { fileURLToPath } from "node:url";

logger.level = "silent";

const BENCHMARK_DIR = path.join(process.cwd(), "data", "global");

async function cleanup() {
    try {
        await fs.rm(BENCHMARK_DIR, { recursive: true, force: true });
        console.log(`\n🧹 Cleaned up benchmark directory: ${BENCHMARK_DIR}`);
    } catch (err) {
        // Ignore
    }
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function runWorker(agentId: string, numEvents: number): Promise<{ duration: number; errors: number }> {
    return new Promise((resolve, reject) => {
        // To run ts file in worker thread, we can use tsx wrapper or compile to js.
        // Node 22+ supports --experimental-strip-types, but here we can just use tsx.
        // A safer way without compiling is to use worker_threads with tsx:
        const workerPath = path.join(__dirname, "worker_insert.ts");
        
        // Use Node's regular Worker if running via tsx
        const worker = new Worker(workerPath, {
            workerData: { agentId, numEvents },
            // TSX magic to execute TS in workers:
            execArgv: ['--import', 'tsx']
        });

        worker.on("message", (msg) => {
            if (msg.status === "error") reject(new Error(msg.error));
            else resolve({ duration: msg.duration, errors: msg.errors });
        });
        worker.on("error", reject);
        worker.on("exit", (code) => {
            if (code !== 0) reject(new Error(`Worker stopped with exit code ${code}`));
        });
    });
}

async function runBenchmark() {
    console.log("======================================================");
    console.log("🚀 LIVA-UHM ENTERPRISE SWARM MEMORY BENCHMARK");
    console.log("======================================================\n");

    await cleanup();

    // Init Global Memory for Primary Thread
    const primaryAgent = "agent_alpha";
    const primaryMemory = await StructuredMemory.create(primaryAgent);

    // --- SCENARIO 1: L1 Write Contention & Mixed Workload ---
    console.log("--- SCENARIO 1: Mixed Workload & Write Contention (SQLITE_BUSY check) ---");
    const agents = ["agent_alpha", "agent_beta", "agent_gamma", "agent_delta", "agent_epsilon"];
    const numEventsPerAgent = 1000;
    console.log(`Spawning 5 Worker Threads to write ${numEventsPerAgent} events each...`);
    console.log(`Simultaneously running Read queries on Main Thread to measure Read Latency Spike...`);
    
    let totalErrors = 0;
    const startS1 = performance.now();
    
    // Start Read workload on Main Thread
    let readCount = 0;
    let maxReadLatency = 0;
    let totalReadLatency = 0;
    let isWriting = true;
    
    const readTask = async () => {
        const dummyVec = Array.from({ length: 384 }, () => Math.random() * 2 - 1);
        while(isWriting) {
            const startRead = performance.now();
            await primaryMemory.searchSimilarVectors(dummyVec, 5);
            const rLat = performance.now() - startRead;
            if (rLat > maxReadLatency) maxReadLatency = rLat;
            totalReadLatency += rLat;
            readCount++;
            await new Promise(r => setTimeout(r, 0)); // Yield to event loop
        }
    };
    
    const readPromise = readTask();

    // Start Write Workload on Worker Threads
    const workerPromises = agents.map(agent => runWorker(agent, numEventsPerAgent));
    const workerResults = await Promise.all(workerPromises).catch(e => {
        console.error("Worker Error:", e);
        return [];
    });
    
    isWriting = false; // Stop reads
    await readPromise;
    
    const endS1 = performance.now();
    const durationS1 = endS1 - startS1;
    
    let maxWorkerDuration = 0;
    for (const res of workerResults) {
        if (res) {
            totalErrors += res.errors;
            if (res.duration > maxWorkerDuration) maxWorkerDuration = res.duration;
        }
    }
    
    const throughputS1 = (numEventsPerAgent * 5) / (maxWorkerDuration / 1000);
    
    console.log(`✅ L1 Write completed by 5 Workers in ${(maxWorkerDuration).toFixed(2)} ms`);
    console.log(`📊 Write Throughput: ${throughputS1.toFixed(2)} ops/sec`);
    console.log(`🔴 Write Drop/Error Rate (SQLITE_BUSY): ${totalErrors} errors`);
    console.log(`📖 Read Operations completed during Write: ${readCount}`);
    console.log(`⏱️ Max Read Latency Spike: ${maxReadLatency.toFixed(2)} ms (Avg: ${(totalReadLatency/readCount).toFixed(2)} ms)\n`);

    // Verify Isolation
    const countAlpha = await primaryMemory.getUnconsolidatedCount();
    console.log(`Isolation Check: Agent Alpha sees ${countAlpha} events (Expected: ${numEventsPerAgent}) - ${countAlpha === numEventsPerAgent ? '✅ PASS' : '❌ FAIL'}\n`);

    console.log("\n======================================================");
    console.log("🏁 ENTERPRISE BENCHMARK COMPLETED.");
    console.log("======================================================");
    
    process.exit(0);
}

runBenchmark().catch(err => {
    console.error("Benchmark failed:", err);
    process.exit(1);
});
