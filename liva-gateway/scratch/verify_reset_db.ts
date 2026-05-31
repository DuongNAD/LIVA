import "dotenv/config";
import { MemoryManager } from "../src/MemoryManager";
import { logger } from "../src/utils/logger";
import * as path from "node:path";
import * as fs from "node:fs/promises";

async function run() {
    logger.info("Initializing MemoryManager...");
    const mm = new MemoryManager("liva_core");
    await mm.initialize();

    // 1. Check current facts count
    const sm = mm.getStructuredMemoryInstance();
    const dbPath = sm.getDbPath();
    logger.info(`Database path: ${dbPath}`);

    // Check facts count in StructuredMemory
    const initialFacts = sm.getAllFacts();
    logger.info(`Initial facts count: ${initialFacts.length}`);

    // If facts count is 0, let's insert a test fact
    if (initialFacts.length === 0) {
        logger.info("No facts found. Adding a test fact...");
        await sm.setFact("test_key", "test_value", 30, "user", "TEST");
        const factsAfterAdd = sm.getAllFacts();
        logger.info(`Facts count after adding: ${factsAfterAdd.length}`);
    }

    // 2. Perform reset
    logger.info("Triggering resetAllMemory...");
    const result = await mm.resetAllMemory();
    logger.info(`Reset result: ${JSON.stringify(result)}`);

    // 3. Verify database is empty/reset
    const smAfter = mm.getStructuredMemoryInstance();
    const factsAfter = smAfter.getAllFacts();
    logger.info(`Facts count after reset: ${factsAfter.length}`);

    if (factsAfter.length === 0) {
        logger.info("✅ SUCCESS: Database has been reset to 0 facts!");
    } else {
        logger.error("❌ FAILURE: Database still contains facts after reset.");
    }

    await mm.dispose();
}

run().catch(e => {
    logger.error(`Error running verification: ${e instanceof Error ? e.stack : String(e)}`);
});
