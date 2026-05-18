import { WebSocket } from 'ws';
import { DatabaseSync } from 'node:sqlite';
import * as path from 'path';
import * as fs from 'fs';

const wsUrl = 'ws://localhost:8082';
console.log(`Connecting to LIVA Gateway to trigger consolidation...`);

const ws = new WebSocket(wsUrl);

ws.on('open', () => {
    console.log('✅ WebSocket Connected!');
    console.log('Sending consolidate_memory with force: true...');
    ws.send(JSON.stringify({
        event: 'consolidate_memory',
        payload: { force: true }
    }));
});

ws.on('message', (data) => {
    const rawData = data.toString();
    try {
        const msg = JSON.parse(rawData);
        if (msg.event === 'consolidation_complete') {
            console.log(`\n✅ Consolidation Complete! Consolidated ${msg.payload.consolidated} event(s).`);
            ws.close();
        }
    } catch (e) {}
});

ws.on('close', () => {
    inspectDatabase();
});

function inspectDatabase() {
    console.log('\nInspecting SQLite Database of [liv_async_core]...');
    const dbPath = path.join('data', 'agents', 'liv_async_core', 'structured_memory.sqlite');
    try {
        const db = new DatabaseSync(dbPath);
        
        const facts = db.prepare("SELECT * FROM facts").all();
        const events = db.prepare("SELECT * FROM events").all();
        const vectors = db.prepare("SELECT * FROM vectors_meta").all();
        
        console.log('\n🏆 SQLite Final Results:');
        console.log(`- [L2] events: ${events.length} record(s)`);
        console.log(`- [L3] facts: ${facts.length} record(s)`);
        console.log(`- [L1] vectors_meta: ${vectors.length} record(s)`);

        if (facts.length > 0) {
            console.log('\n🧠 Consolidated L3 Facts:');
            facts.forEach(f => console.log(`  Key: ${f.key} => Value: ${f.value} (Category: ${f.category})`));
        }
        
        if (vectors.length > 0) {
            console.log('\n📍 L1 Vector Meta:');
            vectors.forEach(v => console.log(`  ID: ${v.vec_id} => Content: ${v.content}`));
        }
    } catch (err) {
        console.error('❌ Database error:', err.message);
    }
}
