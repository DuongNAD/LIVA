import { WebSocket } from 'ws';
import { DatabaseSync } from 'node:sqlite';
import * as path from 'path';
import * as fs from 'fs';

console.log('=== LIVA Dual-Channel Memory Segmenter Test ===');

const wsUrl = 'ws://localhost:8082';
console.log(`Connecting to LIVA Gateway at: ${wsUrl}...`);

const ws = new WebSocket(wsUrl);
let messageCount = 0;

ws.on('open', () => {
    console.log('✅ WebSocket Connected!');
    sendFirstMessage();
});

function sendFirstMessage() {
    const testMessage1 = 'Tôi đang phát triển hệ điều hành nhận thức LIVA với kiến trúc bộ nhớ phân tầng UHM và RAG.';
    console.log(`\n1. Sending Topic A user message: "${testMessage1}"`);
    ws.send(JSON.stringify({
        event: 'user_voice_command',
        payload: { text: testMessage1 }
    }));
}

function sendSecondMessage() {
    const testMessage2 = 'Hôm nay trời nắng nóng quá, tôi định rủ bạn bè đi làm vài cốc bia hơi mát lạnh.';
    console.log(`\n2. Sending Topic B user message (Triggers Segmenter): "${testMessage2}"`);
    ws.send(JSON.stringify({
        event: 'user_voice_command',
        payload: { text: testMessage2 }
    }));
}

ws.on('message', (data) => {
    const rawData = data.toString();
    try {
        const msg = JSON.parse(rawData);
        if (msg.event === 'ai_stream_chunk') {
            process.stdout.write(msg.payload.textChunk || '');
        } else if (msg.event === 'ai_spoken_response') {
            console.log('\n\n✅ AI Response Complete!');
            messageCount++;
            if (messageCount === 1) {
                // Send second message after first response complete
                setTimeout(sendSecondMessage, 2000);
            } else if (messageCount === 2) {
                console.log('\n3. Waiting 15 seconds for ReflectionDaemon to process... (12s debounce)');
                setTimeout(requestMemoryData, 15000);
            }
        } else if (msg.event === 'memory_data') {
            console.log('\n📊 Memory Data Received from Gateway:');
            console.log(`- RAM Cache (L0) count: ${msg.payload.l0?.length || 0}`);
            console.log(`- Session State (L0.5) length: ${msg.payload.l0_5?.length || 0} bytes`);
            console.log(`- Facts (L3) count: ${msg.payload.facts?.length || 0}`);
            console.log(`- Events (L2) count: ${msg.payload.events?.length || 0}`);
            console.log(`- Vectors (L1) count: ${msg.payload.vectors?.length || 0}`);
            ws.close();
        }
    } catch (e) {
        // Ignored
    }
});

function requestMemoryData() {
    console.log('\n4. Requesting Memory Data from Gateway...');
    ws.send(JSON.stringify({ event: 'get_memory_data' }));
}

ws.on('error', (err) => {
    console.error('❌ WebSocket Error:', err.message);
});

ws.on('close', () => {
    console.log('\nWebSocket Connection Closed.');
    inspectDatabase();
});

function inspectDatabase() {
    console.log('\n5. Inspecting SQLite Database of [liv_async_core]...');
    const dbPath = path.join('data', 'agents', 'liv_async_core', 'structured_memory.sqlite');
    if (!fs.existsSync(dbPath)) {
        console.error('❌ Database file not found at:', dbPath);
        return;
    }

    try {
        const db = new DatabaseSync(dbPath);
        
        const facts = db.prepare("SELECT count(*) as count FROM facts").get();
        const events = db.prepare("SELECT count(*) as count FROM events").get();
        const vectors = db.prepare("SELECT count(*) as count FROM vectors_meta").get();
        const turns = db.prepare("SELECT count(*) as count FROM turn_layer_nodes").get();
        
        console.log('\n🏆 SQLite Final Results:');
        console.log(`- [L0.5] turn_layer_nodes: ${turns.count} record(s)`);
        console.log(`- [L2] events: ${events.count} record(s)`);
        console.log(`- [L3] facts: ${facts.count} record(s)`);
        console.log(`- [L1] vectors_meta: ${vectors.count} record(s)`);

        if (events.count > 0 || facts.count > 0) {
            console.log('\n🔥 Success! Memory layers consolidated and stored successfully!');
        } else {
            console.log('\n⚠️ Reflection Daemon is still debouncing, or wait duration needs adjustment.');
        }
    } catch (err) {
        console.error('❌ Database error:', err.message);
    }
}
