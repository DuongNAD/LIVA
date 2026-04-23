const WebSocket = require('ws');
const ws = new WebSocket('ws://localhost:8082');

ws.on('open', () => {
    console.log('[System] Connected to Gateway via WebSocket.');
    const commandText = 'Hỏi "Mẹ" ăn cơm chưa';
    console.log(`[User] Sending command to AI: "${commandText}"`);
    
    ws.send(JSON.stringify({
        event: 'user_voice_command',
        payload: {
            text: commandText
        }
    }));
});

ws.on('message', (data) => {
    const response = JSON.parse(data.toString());
    console.log('\n[LIVA AI Reply Event]', response.event);
    
    if (response.event === 'ai_speaking') {
        console.log(`[LIVA AI Speaks]: ${response.payload.text}`);
    } else if (response.event === 'action_start') {
        console.log(`[LIVA AI Action]: ${response.payload.description}`);
    } else {
        console.log('[LIVA AI Raw Response data]:', JSON.stringify(response, null, 2));
    }
});

ws.on('error', (err) => {
    console.error('[Error] WebSocket error:', err.message);
    process.exit(1);
});
