import { promises as fsp } from "node:fs";
import * as path from "node:path";
import type { StructuredMemory } from "../StructuredMemory";

export async function createMemory(
    agentId: string,
    customStorePath: string | undefined,
    instances: Map<string, StructuredMemory>,
    instantiate: (storePath: string, agentId: string) => StructuredMemory
): Promise<StructuredMemory> {
    let baseDir = customStorePath ? path.dirname(customStorePath) : path.join(process.cwd(), "data", "global");
    if (process.env.VITEST && !customStorePath) {
        const testId = process.env.VITEST_WORKER_ID || Math.random().toString(36).substring(7);
        baseDir = path.join(process.cwd(), "data", "agents", `__test_default_${testId}`);
    }
    await fsp.mkdir(baseDir, { recursive: true });

    const storePath = customStorePath || path.join(baseDir, "structured_memory.sqlite");
    
    if (instances.has(storePath)) {
        const existing = instances.get(storePath)!;
        await existing.initialize();
        return existing;
    }

    const instance = instantiate(storePath, agentId);
    instances.set(storePath, instance);

    // Start initialization and store its promise
    await instance.initialize();

    // Migrate old JSON if exists (async, non-blocking)
    await instance.migrateFromJson(path.join(baseDir, "structured_memory.json"));

    return instance;
}
