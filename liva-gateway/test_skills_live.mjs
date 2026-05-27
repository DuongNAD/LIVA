import WebSocket from 'ws';
import { unpack } from 'msgpackr';

const TESTS = [
  { name: "TEST 1: Time Skill (get_current_time)", msg: "Bây giờ là mấy giờ rồi?" },
  { name: "TEST 2: Memory Skill", msg: "Em nhớ gì về anh?" },
  { name: "TEST 3: Translation Skill", msg: "Dịch câu 'Hello World' sang tiếng Việt" },
];

let currentTest = 0;

function runTest(testIdx) {
  if (testIdx >= TESTS.length) {
    console.log("\n========== ALL TESTS COMPLETE ==========");
    process.exit(0);
  }

  const test = TESTS[testIdx];
  console.log(`\n${"=".repeat(60)}`);
  console.log(`🧪 ${test.name}: "${test.msg}"`);
  console.log(`${"=".repeat(60)}`);

  const ws = new WebSocket('ws://127.0.0.1:8082');
  const chunks = [];
  let events = [];

  ws.on('open', () => {
    ws.send(JSON.stringify({ event: 'user_voice_command', payload: { text: test.msg } }));
  });

  ws.on('message', (d, isBinary) => {
    if (!isBinary) return;
    const buf = Buffer.from(d);
    if (buf[0] !== 0x02) return;

    try {
      const msg = unpack(buf.subarray(1));
      events.push(msg.event);
      
      // The payload structure from broadcastUIEvent is:
      // { event: "ai_stream_chunk", payload: { textChunk: "..." } }
      if (msg.event === 'ai_stream_chunk') {
        const text = msg.payload?.textChunk || msg.payload?.data?.textChunk || '';
        if (text) chunks.push(text);
      }
      else if (msg.event === 'ai_tool_call' || msg.event === 'tool_execution') {
        console.log(`  🔧 TOOL: ${JSON.stringify(msg.payload)}`);
      }
      else if (msg.event === 'ai_spoken_response') {
        console.log(`  🔊 SPOKEN: ${JSON.stringify(msg.payload?.text || msg.payload?.data?.text || '').substring(0, 200)}`);
      }
    } catch {}
  });

  const timeout = setTimeout(() => {
    const fullText = chunks.join('');
    const uniqueEvents = [...new Set(events)];
    console.log(`  📡 Events received: ${uniqueEvents.join(', ')}`);
    if (fullText) {
      console.log(`  ✅ RESPONSE (${chunks.length} chunks):`);
      console.log(`  "${fullText.substring(0, 500)}"`);
    } else {
      console.log(`  ⚠️ No text chunks captured`);
    }
    ws.close();
    setTimeout(() => runTest(testIdx + 1), 3000);
  }, 20000);

  ws.on('error', (e) => {
    console.log(`  ❌ WS ERROR: ${e.message}`);
    clearTimeout(timeout);
    setTimeout(() => runTest(testIdx + 1), 3000);
  });
}

runTest(0);
