import { DatabaseSync } from 'node:sqlite';
import * as path from 'path';
import * as fs from 'fs';

const agentsDir = path.join('data', 'agents');
if (!fs.existsSync(agentsDir)) {
    console.log('No agents directory found at:', agentsDir);
    process.exit(0);
}

const folders = fs.readdirSync(agentsDir);
console.log('All agent folders found:', folders);

for (const folder of folders) {
    const dbPath = path.join(agentsDir, folder, 'structured_memory.sqlite');
    if (!fs.existsSync(dbPath)) {
        console.log(`Folder [${folder}] does not contain structured_memory.sqlite`);
        continue;
    }
    
    console.log(`\n--- Inspecting Agent [${folder}] ---`);
    try {
        const db = new DatabaseSync(dbPath);
        const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table'").all();
        
        for (const table of tables) {
            const tableName = table.name;
            if (['vec_idx', 'vectors_fts'].includes(tableName) || tableName.startsWith('vec_idx_') || tableName.startsWith('vectors_fts_')) continue;
            try {
                const countRow = db.prepare(`SELECT count(*) as count FROM ${tableName}`).get();
                console.log(`Table ${tableName}: ${countRow.count} records`);
            } catch(e) {
                console.log(`Table ${tableName}: error - ${e.message}`);
            }
        }
    } catch(err) {
        console.log(`Failed to open DB for [${folder}]:`, err.message);
    }
}
