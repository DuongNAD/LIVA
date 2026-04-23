const WebSocket = require('ws');

const ws = new WebSocket('ws://localhost:8082');

ws.on('open', function open() {
  console.log('Connected to LIVA Gateway');
  
  const payload = {
    event: 'user_voice_command',
    payload: {
      text: 'Giúp tôi tóm tắt 5 gmail gần nhất rồi gửi qua zalo cho tôi'
    }
  };

  ws.send(JSON.stringify(payload));
  console.log('Sent message');
});

ws.on('message', function incoming(data) {
  console.log('Received:', data.toString());
  const parsed = JSON.parse(data.toString());
  if (parsed.event === 'ai_spoken_response') {
      console.log('Test completed. Closing connection.');
      ws.close();
      process.exit(0);
  }
});

ws.on('error', function error(err) {
  console.error('WebSocket error:', err);
});
