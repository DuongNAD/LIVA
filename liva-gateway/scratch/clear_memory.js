import { DatabaseSync } from 'node:sqlite';
import * as path from 'path';
import * as fs from 'fs';

const dbPath = path.join('data', 'global', 'structured_memory.sqlite');
console.log('Target database:', dbPath);

try {
    if (fs.existsSync(dbPath)) {
        const db = new DatabaseSync(dbPath);
        db.exec("PRAGMA busy_timeout = 5000;");
        
        console.log('Clearing turn_layer_nodes...');
        db.exec("DELETE FROM turn_layer_nodes;");
        
        console.log('Clearing facts...');
        db.exec("DELETE FROM facts;");
        
        console.log('Clearing events...');
        db.exec("DELETE FROM events;");
        
        console.log('Clearing tasks...');
        db.exec("DELETE FROM tasks;");

        console.log('Database tables cleared successfully.');
        db.close();
    } else {
        console.log('Database file not found.');
    }
} catch (e) {
    console.error('Error clearing database:', e.message);
}

// Xóa file short term memory
const shortTermPath = path.join('data', 'agents', 'liva_core', 'short_term_memory.jsonl');
try {
    if (fs.existsSync(shortTermPath)) {
        fs.unlinkSync(shortTermPath);
        console.log('Deleted short_term_memory.jsonl');
    }
} catch (e) {
    console.error('Error deleting short_term_memory.jsonl:', e.message);
}

const sessionStatePath = path.join('data', 'agents', 'liva_core', 'SESSION-STATE.md');
try {
    if (fs.existsSync(sessionStatePath)) {
        fs.unlinkSync(sessionStatePath);
        console.log('Deleted SESSION-STATE.md');
    }
} catch (e) {
    console.error('Error deleting SESSION-STATE.md:', e.message);
}
