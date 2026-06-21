import { Worker } from "node:worker_threads";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const workerPath = path.resolve("./src/workers/NemotronWorker.ts");
const workerUrl = pathToFileURL(workerPath).href;

console.log("Loading worker from:", workerUrl);

const worker = new Worker(`
    import { register } from 'node:module';
    import { pathToFileURL } from 'node:url';
    register('tsx', pathToFileURL('./'), { data: {} });
    import('${workerUrl.replace(/\\/g, "\\\\")}');
`, { eval: true });

worker.on("message", (msg) => {
    console.log("Message from worker:", msg);
    if (msg.type === "ready" || msg.type === "error") {
        process.exit(msg.type === "error" ? 1 : 0);
    }
});

worker.on("error", (err) => {
    console.error("Worker error event:", err);
    process.exit(1);
});

// Mock init msg
worker.postMessage({ type: "init", modelDir: "models/nemotron-asr" });
