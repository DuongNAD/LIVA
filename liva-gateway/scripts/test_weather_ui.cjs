const WebSocket = require('ws');

const ws = new WebSocket('ws://localhost:8082');

ws.on('open', () => {
    console.log('Connected to LIVA Gateway.');
    
    // Simulate user sending a voice transcription
    const message = {
        event: "user_voice_command",
        payload: {
            text: "Thời tiết hiện tại của tôi như thế nào?",
            isFinal: true
        }
    };
    ws.send(JSON.stringify(message));
    console.log('Sent message:', message.payload.text);
});

ws.on('message', (data) => {
    const msg = JSON.parse(data.toString());
    
    if (msg.event === 'ai_stream_start') {
        console.log('\n[START] AI Stream Started.');
    } 
    else if (msg.event === 'ai_stream_chunk') {
        process.stdout.write(msg.payload.textChunk);
    } 
    else if (msg.event === 'ai_spoken_response') {
        console.log('\n\n[END] AI Spoken Response:');
        console.log(msg.payload.text);
        ws.close();
    }
});

ws.on('error', console.error);
ws.on('close', () => console.log('\nDisconnected.'));
