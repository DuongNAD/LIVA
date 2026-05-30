import { DatabaseSync } from "node:sqlite";
import path from "node:path";

const dbPath = path.resolve("data/global/structured_memory.sqlite");
console.log("Connecting to:", dbPath);
const db = new DatabaseSync(dbPath);

const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table'").all();
console.log("Tables:", tables);

for (const t of tables) {
    const tableName = t.name;
    try {
        const count = db.prepare(`SELECT COUNT(*) as cnt FROM ${tableName}`).get();
        console.log(`Table ${tableName}: ${count.cnt} rows`);
        if (tableName.includes("message") || tableName.includes("turn") || tableName.includes("node")) {
            const rows = db.prepare(`SELECT * FROM ${tableName} ORDER BY rowid DESC LIMIT 5`).all();
            console.log(`Last 5 rows of ${tableName}:`, JSON.stringify(rows, null, 2));
        }
    } catch (e) {
        console.error(`Error querying ${tableName}:`, e.message);
    }
}
db.close();
