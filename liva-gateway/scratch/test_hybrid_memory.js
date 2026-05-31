import { WebSocket } from 'ws';
import { DatabaseSync } from 'node:sqlite';
import * as path from 'path';
import * as fs from 'fs';
import { unpack } from 'msgpackr';

console.log('=== LIVA Unified Hierarchical Memory (UHM) Multi-Layer Test ===');

const wsUrl = 'ws://localhost:8082';
console.log(`Connecting to LIVA Gateway at: ${wsUrl}...`);

const ws = new WebSocket(wsUrl);

ws.on('open', () => {
    console.log('✅ WebSocket Connected!');
    
    // Send a rich factual message
    const richMessage = 'Tôi rất thích ăn phở bò Hà Nội và uống cà phê trứng vào mỗi buổi sáng.';
    console.log(`\n1. Sending rich user statement: "${richMessage}"`);
    
    ws.send(JSON.stringify({
        event: 'user_voice_command',
        payload: { text: richMessage }
    }));
});

ws.on('message', (message, isBinary) => {
    let msg;
    if (isBinary) {
        const buffer = Buffer.isBuffer(message) ? message : Buffer.from(message);
        if (buffer.length > 0) {
            const type = buffer[0];
            if (type === 0x02) {
                try {
                    msg = unpack(buffer.subarray(1));
                } catch (e) {
                    console.error("❌ Error unpacking binary message:", e.message);
                    return;
                }
            } else {
                return; // ignore raw audio or other binary types
            }
        }
    } else {
        const rawData = message.toString();
        try {
            msg = JSON.parse(rawData);
        } catch (e) {
            console.error("❌ Error parsing JSON message:", e.message);
            return;
        }
    }

    if (!msg) return;

    try {
        if (msg.event === 'ai_stream_chunk') {
            process.stdout.write(msg.payload.textChunk || '');
        } else if (msg.event === 'ai_spoken_response') {
            console.log('\n\n✅ AI Response Complete!');
            console.log('\n2. Waiting 21 seconds for Idle Episode Flush timer to run...');
            setTimeout(triggerConsolidation, 21000);
        } else if (msg.event === 'consolidation_complete') {
            console.log(`\n\n✅ Consolidation Complete! Consolidated ${msg.payload.consolidated} event(s).`);
            console.log('\n3. Requesting Memory Data from Gateway...');
            ws.send(JSON.stringify({ event: 'get_memory_data' }));
        } else if (msg.event === 'memory_data') {
            console.log('\n📊 Memory Data Received from Gateway:');
            console.log(`- RAM Cache (L0) count: ${msg.payload.l0?.length || 0}`);
            console.log(`- Session State (L0.5) length: ${msg.payload.l0_5?.length || 0} bytes`);
            console.log(`- Facts (L3) count: ${msg.payload.facts?.length || 0}`);
            console.log(`- Events (L2) count: ${msg.payload.events?.length || 0}`);
            console.log(`- Vectors (L1) count: ${msg.payload.vectors?.length || 0}`);
            
            if (msg.payload.facts?.length > 0) {
                console.log('\n🏆 Insights/Facts (L3):');
                console.log(msg.payload.facts.slice(0, 3));
            }
            ws.close();
        }
    } catch (e) {
        // Ignored
    }
});

function triggerConsolidation() {
    console.log('\nTriggering manual memory consolidation (force: true)...');
    ws.send(JSON.stringify({
        event: 'consolidate_memory',
        payload: { force: true }
    }));
}

ws.on('error', (err) => {
    console.error('❌ WebSocket Error:', err.message);
});

ws.on('close', () => {
    console.log('\nWebSocket Connection Closed.');
    inspectDatabase();
});

function inspectDatabase() {
    console.log('\n4. Inspecting SQLite Database of [global]...');
    const dbPath = path.join('data', 'global', 'structured_memory.sqlite');
    if (!fs.existsSync(dbPath)) {
        console.error('❌ Database file not found at:', dbPath);
        return;
    }

    try {
        const db = new DatabaseSync(dbPath);
        
        const facts = db.prepare("SELECT * FROM facts").all();
        const events = db.prepare("SELECT * FROM events").all();
        const vectors = db.prepare("SELECT * FROM vectors_meta").all();
        const turns = db.prepare("SELECT * FROM turn_layer_nodes").all();
        
        console.log('\n🏆 SQLite Final Results:');
        console.log(`- [L0.5] turn_layer_nodes: ${turns.length} record(s)`);
        console.log(`- [L2] events: ${events.length} record(s)`);
        console.log(`- [L3] facts: ${facts.length} record(s)`);
        console.log(`- [L1] vectors_meta: ${vectors.length} record(s)`);

        if (facts.length > 0) {
            console.log('\n🧠 Consolidated L3 Facts (First 3):');
            console.log(facts.slice(0, 3).map(f => `Key: ${f.key} => Value: ${f.value}`));
        }
        
        if (vectors.length > 0) {
            console.log('\n📍 L1 Vector Meta (First 3):');
            console.log(vectors.slice(0, 3).map(v => `ID: ${v.vec_id} => Content: ${v.content}`));
        }
    } catch (err) {
        console.error('❌ Database error:', err.message);
    }
}
