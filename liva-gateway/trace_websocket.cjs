const WebSocket = require('ws');
const { unpack } = require('msgpackr');

const ws = new WebSocket('ws://localhost:8082');

ws.on('open', () => {
    console.log('Connected to LIVA Gateway.');
    
    const message = {
        event: "user_voice_command",
        payload: {
            text: "thời tiết ở Sa Pa hôm nay ra sao",
            isFinal: true
        }
    };
    
    ws.send(JSON.stringify(message));
    console.log('Sent message:', message.payload.text);
});

ws.on('message', (data) => {
    try {
        let eventObj = null;
        if (data instanceof Buffer) {
            const firstByte = data[0];
            if (firstByte === 0x02) {
                const payload = data.slice(1);
                eventObj = unpack(payload);
            } else {
                console.log(`[Binary Raw] First byte: ${firstByte}, length: ${data.length}`);
                return;
            }
        } else {
            eventObj = JSON.parse(data.toString());
        }
        
        // Suppress printing the massive audio payload to keep logs clean
        if (eventObj.event === "ai_audio_chunk" || eventObj.name === "ai_audio_chunk") {
            console.log(`[EVENT] ai_audio_chunk (Suppressed binary audio data, length: ${eventObj.payload?.audio?.length || 0})`);
            return;
        }

        console.log(`\n[EVENT] ${eventObj.event || eventObj.name}`);
        if (eventObj.payload) {
            console.log('Payload:', JSON.stringify(eventObj.payload, null, 2));
        } else if (eventObj.data) {
            console.log('Data:', JSON.stringify(eventObj.data, null, 2));
        } else {
            console.log('Raw:', JSON.stringify(eventObj, null, 2));
        }
    } catch (e) {
        console.error('Failed to parse message:', e.message);
    }
});

ws.on('error', console.error);
ws.on('close', () => console.log('\nDisconnected.'));
