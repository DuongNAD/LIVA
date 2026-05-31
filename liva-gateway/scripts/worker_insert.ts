import { parentPort, workerData } from "node:worker_threads";
import { StructuredMemory } from "../src/memory/StructuredMemory.ts";
import { performance } from "node:perf_hooks";

async function run() {
    const { agentId, numEvents } = workerData;
    
    let memory: StructuredMemory;
    try {
        memory = await StructuredMemory.create(agentId);
    } catch (e: unknown) {
        parentPort?.postMessage({ status: "error", error: e instanceof Error ? e.message : String(e) });
        return;
    }

    let errors = 0;
    const start = performance.now();

    for (let i = 0; i < numEvents; i++) {
        try {
            memory.insertEvent({
                eventId: `evt_${agentId}_${i}`,
                timestamp: Date.now(),
                phi: { facts: [`Fact ${i}`], entities: [] },
                psi: { sentiment: "neutral", intent: "test", relational: "" },
                rawUserMsg: "test message",
                rawAiReply: "test reply"
            });
        } catch (err: unknown) {
            errors++;
        }
    }

    const duration = performance.now() - start;
    
    parentPort?.postMessage({
        status: "done",
        duration,
        errors,
        agentId
    });
}

run();
