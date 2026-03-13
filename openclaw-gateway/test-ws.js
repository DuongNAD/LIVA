const ws = new (require('ws'))('ws://localhost:8082');

ws.on('open', () => {
    console.log('Test Client Connected');
    ws.send(JSON.stringify({
        event: 'user_voice_command',
        payload: { text: "Xin chào Liva, tốc độ phản hồi của bạn thế nào?" }
    }));
});

ws.on('message', (data) => {
    console.log('Test Client Received:', JSON.parse(data.toString()));
});

ws.on('close', () => {
    console.log('Test Client Disconnected');
});
