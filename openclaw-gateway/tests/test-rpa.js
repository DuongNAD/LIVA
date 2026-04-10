const WebSocket = require('ws');
const ws = new WebSocket('ws://localhost:8082');

ws.on('open', () => {
    console.log('Connected to Gateway. Sending message...');
    ws.send(JSON.stringify({
        event: 'user_voice_command',
        payload: {
            text: 'Giúp tôi gửi tin nhắn messenger cho bạn Phạm Vũ với nội dung: Mày quá béo để có thể play'
        }
    }));
});

ws.on('message', (data) => {
    console.log('Received:', data.toString());
    process.exit(0);
});

ws.on('error', (err) => {
    console.error('Error:', err);
    process.exit(1);
});
