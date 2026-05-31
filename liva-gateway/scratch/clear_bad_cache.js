import { DatabaseSync } from "node:sqlite";
import path from "node:path";
import * as sqliteVec from "sqlite-vec";

const dbPath = path.resolve("data/global/structured_memory.sqlite");
console.log("Connecting to:", dbPath);
const db = new DatabaseSync(dbPath, { allowExtension: true });
sqliteVec.load(db);

db.exec("BEGIN TRANSACTION;");

try {
    // 1. Delete matching L1 turns
    const turnResult = db.prepare(`
        DELETE FROM turn_layer_nodes 
        WHERE userMsg = 'Trời nay như nào' 
          AND aiReply LIKE '%trợ lý AI thông minh%'
    `).run();
    console.log(`Deleted ${turnResult.changes} rows from turn_layer_nodes.`);

    // 2. Delete matching L2 events
    const eventResult = db.prepare(`
        DELETE FROM events 
        WHERE rawUserMsg = 'Trời nay như nào' 
          AND rawAiReply LIKE '%trợ lý AI thông minh%'
    `).run();
    console.log(`Deleted ${eventResult.changes} rows from events.`);

    // 3. Find matching vectors
    // Find vectors of assistant greeting or matching user query from the contaminated timestamp
    const pollutedVectors = db.prepare(`
        SELECT id FROM vectors_meta 
        WHERE content LIKE '%trợ lý AI thông minh%' 
           OR (content = 'Trời nay như nào' AND created_at >= 1779850000000)
    `).all();
    
    console.log(`Found ${pollutedVectors.length} polluted vector entries to delete.`);

    if (pollutedVectors.length > 0) {
        const placeholders = pollutedVectors.map(() => '?').join(',');
        const ids = pollutedVectors.map(v => BigInt(v.id));

        const vecIdxResult = db.prepare(`DELETE FROM vec_idx WHERE rowid IN (${placeholders})`).run(...ids);
        console.log(`Deleted ${vecIdxResult.changes} entries from vec_idx.`);

        const ftsResult = db.prepare(`DELETE FROM vectors_fts WHERE rowid IN (${placeholders})`).run(...ids);
        console.log(`Deleted ${ftsResult.changes} entries from vectors_fts.`);

        const metaResult = db.prepare(`DELETE FROM vectors_meta WHERE id IN (${placeholders})`).run(...ids);
        console.log(`Deleted ${metaResult.changes} entries from vectors_meta.`);
    }

    db.exec("COMMIT;");
    console.log("Database transaction committed successfully.");
} catch (e) {
    db.exec("ROLLBACK;");
    console.error("Database transaction rolled back due to error:", e.message);
} finally {
    db.close();
}
