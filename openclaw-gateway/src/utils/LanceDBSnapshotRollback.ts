import * as lancedb from "@lancedb/lancedb";
import * as path from "path";
import { exec } from "child_process";
import { promisify } from "util";
import { logger } from "./logger";

const execAsync = promisify(exec);

export async function snapshotRollback() {
    try {
        const dbDir = path.join(process.cwd(), "data", "lancedb");
        const db = await lancedb.connect(dbDir);
        
        const { stdout } = await execAsync(`git log -1 --format=%ct`);
        const safeCommitTimestamp = Number.parseInt(stdout.trim()) * 1000;

        logger.info(`[LanceDB Snapshot] Secure Commit Timestamp: ${new Date(safeCommitTimestamp).toISOString()}`);

        const tableNames = await db.tableNames();
        
        for (const tableName of tableNames) {
            const table = await db.openTable(tableName);
            // We expect vectors to have a 'timestamp' column
            logger.info(`[LanceDB Snapshot] Pruning dirty vectors from '${tableName}'...`);
            await table.delete(`timestamp > ${safeCommitTimestamp}`);
        }
        
        logger.info(`[LanceDB Snapshot] Rollback complete. Reflexion memories synchronized with code.`);
    } catch(e) {
        logger.warn(`[LanceDB Snapshot] Warning: Could not complete vector pruning: ${e}`);
    }
}

if (require.main === module) {
    snapshotRollback().then(() => process.exit(0));
}
