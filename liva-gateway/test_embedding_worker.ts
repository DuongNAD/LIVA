import { Worker } from "node:worker_threads";
import * as path from "node:path";
import * as url from "node:url";

const __filename = url.fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const workerPath = path.join(__dirname, "src", "workers", "EmbeddingWorker.ts");

console.log("Worker path:", workerPath);

// using tsx to load the ts worker directly
const worker = new Worker(`
  const { pathToFileURL } = require('url');
  const { resolve } = require('path');
  require('tsx/cjs');
  require(${JSON.stringify(workerPath)});
`, { eval: true });

worker.on("message", (msg) => {
    console.log("Worker message:", msg);
    if (msg.type === "ready") {
        console.log("Sending embed request...");
        worker.postMessage({ type: "embed", id: "test-1", text: "Hello world" });
    } else if (msg.type === "embed_result") {
        console.log("Embed result length:", msg.vector.length);
        console.log("First 5 values:", msg.vector.slice(0, 5));
        worker.postMessage({ type: "dispose" });
    } else if (msg.type === "error") {
        console.error("Worker error:", msg);
        worker.postMessage({ type: "dispose" });
    }
});

worker.on("error", (err) => {
    console.error("Worker error event:", err);
});

worker.on("exit", (code) => {
    console.log("Worker exited with code", code);
});

console.log("Sending init...");
worker.postMessage({ type: "init" });
