import { Worker } from "node:worker_threads";
import * as path from "node:path";
import * as url from "node:url";

const __filename = url.fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const workerPath = path.join(__dirname, "src", "workers", "EmbeddingWorker.ts");

console.log("=========================================");
console.log("Local Embedding Worker Stress Test Script");
console.log("=========================================");

function dotProduct(a: number[], b: number[]): number {
    if (a.length !== b.length) throw new Error("Vector length mismatch");
    return a.reduce((sum, val, i) => sum + val * b[i], 0);
}

// Event Loop lag monitoring
let maxLag = 0;
let lagInterval: NodeJS.Timeout;
function startLagMonitoring() {
    let lastTime = Date.now();
    lagInterval = setInterval(() => {
        const now = Date.now();
        const lag = now - lastTime - 10; // 10ms is the interval
        if (lag > maxLag) {
            maxLag = lag;
        }
        lastTime = now;
    }, 10);
}

function stopLagMonitoring() {
    clearInterval(lagInterval);
}

async function runTests() {
    startLagMonitoring();
    console.log("Initializing worker thread...");

    const worker = new Worker(`
      const { pathToFileURL } = require('url');
      const { resolve } = require('path');
      require('tsx/cjs');
      require(${JSON.stringify(workerPath)});
    `, { eval: true });

    // Helper to request embedding and return a promise
    const embedRequests = new Map<string, (vec: number[]) => void>();
    const errorRequests = new Map<string, (err: string) => void>();

    worker.on("message", (msg) => {
        if (msg.type === "embed_result") {
            const cb = embedRequests.get(msg.id);
            if (cb) {
                cb(msg.vector);
                embedRequests.delete(msg.id);
            }
        } else if (msg.type === "embed_batch_result") {
            const cb = embedRequests.get(msg.id);
            if (cb) {
                cb(msg.vectors);
                embedRequests.delete(msg.id);
            }
        } else if (msg.type === "error") {
            const cb = errorRequests.get(msg.id || "init");
            if (cb) {
                cb(msg.message);
                errorRequests.delete(msg.id || "init");
            } else {
                console.error("Worker error message:", msg);
            }
        }
    });

    worker.on("error", (err) => {
        console.error("Worker error event:", err);
    });

    const readyPromise = new Promise<void>((resolve, reject) => {
        worker.once("message", (msg) => {
            if (msg.type === "ready") {
                resolve();
            } else if (msg.type === "error") {
                reject(new Error(msg.message));
            }
        });
    });

    worker.postMessage({ type: "init" });
    await readyPromise;
    console.log("Worker ready!");

    // Helper wrapper
    function getEmbedding(text: string, id: string): Promise<number[]> {
        return new Promise((resolve, reject) => {
            embedRequests.set(id, resolve);
            errorRequests.set(id, reject);
            worker.postMessage({ type: "embed", id, text });
        });
    }

    function getEmbeddingBatch(texts: string[], id: string): Promise<number[][]> {
        return new Promise((resolve, reject) => {
            embedRequests.set(id, resolve);
            errorRequests.set(id, reject);
            worker.postMessage({ type: "embed_batch", id, texts });
        });
    }

    console.log("\n--- TEST 1: Verification of format and dimensions ---");
    const vec1 = await getEmbedding("This is a test sentence.", "t1");
    console.log(`Vector dimensions: ${vec1.length} (Expected: 384)`);
    if (vec1.length !== 384) {
        throw new Error(`Dimension mismatch: expected 384, got ${vec1.length}`);
    }
    const allFloats = vec1.every(n => typeof n === "number" && !Number.isNaN(n));
    console.log(`Is float array: ${allFloats} (Expected: true)`);
    if (!allFloats) {
        throw new Error("Vector elements are not all numbers");
    }

    // Verify L2 normalization (norm should be ~1.0)
    const norm = Math.sqrt(vec1.reduce((sum, val) => sum + val * val, 0));
    console.log(`Vector norm: ${norm} (Expected: 1.0)`);
    if (Math.abs(norm - 1.0) > 1e-4) {
        throw new Error(`Vector is not normalized: norm = ${norm}`);
    }

    console.log("\n--- TEST 2: Semantic similarity testing ---");
    const tIdenticalA = "The quick brown fox jumps over the lazy dog.";
    const tIdenticalB = "The quick brown fox jumps over the lazy dog.";
    const tSimilarA = "The weather is very hot today.";
    const tSimilarB = "It is extremely hot outside today.";
    const tDifferentA = "The weather is very hot today.";
    const tDifferentB = "I love learning programming languages.";

    const vIdA = await getEmbedding(tIdenticalA, "idA");
    const vIdB = await getEmbedding(tIdenticalB, "idB");
    const vSimA = await getEmbedding(tSimilarA, "simA");
    const vSimB = await getEmbedding(tSimilarB, "simB");
    const vDiffA = await getEmbedding(tDifferentA, "diffA");
    const vDiffB = await getEmbedding(tDifferentB, "diffB");

    const simIdentical = dotProduct(vIdA, vIdB);
    const simSimilar = dotProduct(vSimA, vSimB);
    const simDifferent = dotProduct(vDiffA, vDiffB);

    console.log(`Cosine similarity (Identical texts): ${simIdentical.toFixed(6)} (Expected: 1.000000)`);
    console.log(`Cosine similarity (Similar texts):   ${simSimilar.toFixed(6)} (Expected: high, e.g. > 0.7)`);
    console.log(`Cosine similarity (Different texts): ${simDifferent.toFixed(6)} (Expected: low, e.g. < 0.5)`);

    if (Math.abs(simIdentical - 1.0) > 1e-4) {
        throw new Error(`Identical texts similarity mismatch: ${simIdentical}`);
    }
    if (simSimilar <= simDifferent) {
        throw new Error(`Semantic similarity inversion: similar (${simSimilar}) <= different (${simDifferent})`);
    }

    console.log("\n--- TEST 3: Sequential Stress Testing ---");
    const startTimeSeq = Date.now();
    const count = 50;
    console.log(`Running ${count} sequential embedding requests...`);
    for (let i = 0; i < count; i++) {
        await getEmbedding(`Sentence number ${i} for stress testing our worker.`, `seq-${i}`);
    }
    const durationSeq = Date.now() - startTimeSeq;
    const avgLatency = durationSeq / count;
    console.log(`Completed ${count} requests in ${durationSeq}ms (Avg latency: ${avgLatency.toFixed(2)}ms/req)`);

    console.log("\n--- TEST 4: Batch Stress Testing ---");
    const batchTexts = Array.from({ length: 50 }, (_, i) => `This is sentence ${i} in our batch request stress test.`);
    const startTimeBatch = Date.now();
    console.log(`Sending batch of 50 texts...`);
    const batchResult = await getEmbeddingBatch(batchTexts, "batch-1");
    const durationBatch = Date.now() - startTimeBatch;
    console.log(`Batch processed in ${durationBatch}ms (Avg latency: ${(durationBatch / 50).toFixed(2)}ms/text)`);
    console.log(`Batch result array length: ${batchResult.length}`);
    if (batchResult.length !== 50 || batchResult[0].length !== 384) {
        throw new Error("Batch result length or dimension mismatch");
    }

    console.log("\n--- TEST 5: Concurrent Stress Testing ---");
    const startTimeConc = Date.now();
    const concurrentCount = 20;
    console.log(`Firing ${concurrentCount} concurrent requests...`);
    const promises = Array.from({ length: concurrentCount }, (_, i) => 
        getEmbedding(`Concurrent query number ${i} processed at the same time.`, `conc-${i}`)
    );
    const results = await Promise.all(promises);
    const durationConc = Date.now() - startTimeConc;
    console.log(`All ${concurrentCount} concurrent requests resolved in ${durationConc}ms`);
    console.log(`Results count: ${results.length}`);

    console.log("\n--- TEST 6: Event Loop Responsiveness / Non-blocking ---");
    stopLagMonitoring();
    console.log(`Maximum event loop lag recorded during tests: ${maxLag}ms`);
    // Event loop lag should be very low (usually < 20ms) since the cpu heavy tasks run in worker thread.
    // If it was on main thread, it would block for the duration of model loading and inference (several seconds).
    if (maxLag > 50) {
        console.warn(`WARNING: Event loop lag is high (${maxLag}ms). Check if main thread is doing heavy sync CPU work.`);
    } else {
        console.log(`Pass! Event loop remains responsive (Lag: ${maxLag}ms < 50ms)`);
    }

    console.log("\n--- TEST 7: Long input truncation / No-crash testing ---");
    const longString = "This is a very long string designed to exceed 512 tokens. ".repeat(100);
    console.log(`Sending long text of length ${longString.length} characters...`);
    const vecLong = await getEmbedding(longString, "t7");
    console.log(`Long vector dimensions: ${vecLong.length} (Expected: 384)`);
    if (vecLong.length !== 384) {
        throw new Error(`Dimension mismatch for long input: expected 384, got ${vecLong.length}`);
    }

    console.log("\n--- Cleaning up ---");
    await new Promise<void>((resolve) => {
        worker.on("exit", () => resolve());
        worker.postMessage({ type: "dispose" });
    });
    console.log("Worker disposed and thread exited.");
    console.log("\nALL TESTS PASSED SUCCESSFULLY!");
}

runTests().catch(err => {
    console.error("Test failed with error:", err);
    process.exit(1);
});
