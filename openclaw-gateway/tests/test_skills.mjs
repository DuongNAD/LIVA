import WebSocket from 'ws';

const ws = new WebSocket('ws://127.0.0.1:8082');

const tests = [
    "Thời tiết hiện tại ở Tokyo là bao nhiêu độ? Lấy thời tiết thật nha.",
    "Bây giờ lên mạng search google xem đội tuyển Faker T1 vừa thắng giải gì gần đây không?",
];
let currentTest = 0;

ws.on('open', () => {
    console.log("🔥 Đã cắm ống nghe vào Lõi Gateway!");
    sendNextTest();
});

function sendNextTest() {
    if (currentTest < tests.length) {
        console.log(`\n======================\n[TEST CỦA ANTIGRAVITY #${currentTest+1}] Gửi lệnh: ${tests[currentTest]}`);
        ws.send(JSON.stringify({ 
            event: "user_voice_command", 
            payload: { text: tests[currentTest] } 
        }));
    } else {
        console.log("\n🎯 Hoàn thành toàn bộ quy trình Auto-Test Skills!");
        process.exit(0);
    }
}

ws.on('message', (data) => {
    const raw = data.toString();
    try {
        const parsed = JSON.parse(raw);
        if (parsed.event === 'ai_spoken_response') {
            console.log(`\n🤖 [LIVA Trả lời]:\n${parsed.payload.text}\n`);
            currentTest++;
            setTimeout(sendNextTest, 3000); // Đợi 3s trước khi bắn bài Test tiếp theo
        } else if (parsed.event === 'ai_thinking_start') {
            process.stdout.write("⏳ LIVA đang suy nghĩ... ");
        } else if (parsed.event === 'ai_thinking_end') {
            process.stdout.write("✅ Suy nghĩ xong.\n");
        }
    } catch(e) {
        console.log("Raw msg:", raw);
    }
});
