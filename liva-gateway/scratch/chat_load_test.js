import WebSocket from 'ws';
import { unpack } from 'msgpackr';
import { performance } from 'node:perf_hooks';

const WS_URL = 'ws://127.0.0.1:8082';
const CONCURRENT_USERS = 5;
const SEQUENTIAL_CHATS = 3;
const REQUEST_TIMEOUT_MS = 120000; // 120 seconds to allow TaskQueue to process sequentially

console.log('======================================================');
console.log('🤖 LIVA Chat Bar Load & Concurrency Benchmark (v2)');
console.log('======================================================');
console.log(`Target: ${WS_URL}`);
console.log(`Client Timeout: ${REQUEST_TIMEOUT_MS / 1000}s`);
console.log(`Modes to Test:`);
console.log(`1. Sequential chats (3 turns) - measuring baseline response time.`);
console.log(`2. Concurrent chats (${CONCURRENT_USERS} users simultaneously) - verifying TaskQueue serialization & load tolerance.`);
console.log('======================================================\n');

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function runChatTurn(clientId, query, isDryRun = true) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(WS_URL);
    const startTime = performance.now();
    let tokensReceived = 0;
    let completed = false;
    let streamStartTime = null;
    let connectTime = null;

    const timeout = setTimeout(() => {
      if (!completed) {
        ws.close();
        reject(new Error(`[Client ${clientId}] Request timed out after ${REQUEST_TIMEOUT_MS / 1000}s.`));
      }
    }, REQUEST_TIMEOUT_MS);

    ws.on('open', () => {
      connectTime = performance.now();
      ws.send(JSON.stringify({
        event: 'user_voice_command',
        payload: { text: query, isDryRun }
      }));
    });

    ws.on('message', (message, isBinary) => {
      let data;
      if (isBinary) {
        const buffer = Buffer.isBuffer(message) ? message : Buffer.from(message);
        if (buffer.length > 0 && buffer[0] === 0x02) {
          try {
            data = unpack(buffer.subarray(1));
          } catch (e) {
            console.error(`[Client ${clientId}] MsgPack unpack failed:`, e.message);
            return;
          }
        }
      } else {
        try {
          data = JSON.parse(message.toString());
        } catch (e) {
          return;
        }
      }

      if (!data) return;
      const { event, payload } = data;

      if (event === 'ai_stream_start') {
        streamStartTime = performance.now();
      } else if (event === 'ai_stream_chunk') {
        tokensReceived++;
      } else if (event === 'ai_spoken_response') {
        completed = true;
        clearTimeout(timeout);
        const endTime = performance.now();
        const totalDuration = (endTime - startTime) / 1000;
        const ttft = streamStartTime ? (streamStartTime - startTime) / 1000 : null;
        ws.close();
        resolve({
          clientId,
          ttft,
          totalDuration,
          tokensReceived,
          responseText: payload.text
        });
      }
    });

    ws.on('error', (err) => {
      clearTimeout(timeout);
      reject(err);
    });
  });
}

async function runSequentialTest() {
  console.log('\n--- Phase 1: Sequential Load Test (Baseline) ---');
  const results = [];
  const queries = [
    'LIVA ơi, giới thiệu về bản thân em đi.',
    'Kiến trúc bộ nhớ của em gồm mấy tầng?',
    'LIVA hoạt động trên hệ điều hành nào là chính?'
  ];

  for (let i = 0; i < SEQUENTIAL_CHATS; i++) {
    console.log(`[Turn ${i + 1}/${SEQUENTIAL_CHATS}] Sending: "${queries[i]}" (isDryRun: true)`);
    try {
      const res = await runChatTurn(`Seq-${i + 1}`, queries[i], true);
      console.log(`  └─ Success: TTFT = ${res.ttft ? res.ttft.toFixed(2) + 's' : 'N/A'}, Total Time = ${res.totalDuration.toFixed(2)}s, Tokens = ${res.tokensReceived}`);
      results.push(res);
      await sleep(1000); // 1s rest between chats
    } catch (err) {
      console.error(`  └─ Failed:`, err.message);
    }
  }
  return results;
}

async function runConcurrentTest() {
  console.log(`\n--- Phase 2: Concurrent Concurrency Test (${CONCURRENT_USERS} Rapid Requests) ---`);
  console.log('Sending requests from multiple clients simultaneously to test Gateway TaskQueue...');
  const promises = [];
  const query = 'Em có thể làm được những việc gì cho anh?';

  for (let i = 0; i < CONCURRENT_USERS; i++) {
    promises.push(
      runChatTurn(`User-${i + 1}`, query, true)
        .then((res) => {
          console.log(`  [User-${i + 1}] Finished: Total Time = ${res.totalDuration.toFixed(2)}s, Tokens = ${res.tokensReceived}`);
          return { success: true, data: res };
        })
        .catch((err) => {
          console.error(`  [User-${i + 1}] Error:`, err.message);
          return { success: false, error: err.message };
        })
    );
    // Introduce a very tiny stagger (100ms) to preserve order in the console
    await sleep(100);
  }

  const results = await Promise.all(promises);
  return results;
}

async function main() {
  try {
    const healthRes = await fetch(`${WS_URL.replace('ws', 'http')}/health`);
    const healthJson = await healthRes.json();
    console.log('Gateway Status:', JSON.stringify(healthJson, null, 2));
  } catch (err) {
    console.error('❌ Failed to connect to Gateway health check. Is the Gateway running on port 8082?', err.message);
    process.exit(1);
  }

  // 1. Run Sequential
  const seqResults = await runSequentialTest();

  // 2. Run Concurrent
  const conResults = await runConcurrentTest();

  // 3. Summarize Results
  console.log('\n======================================================');
  console.log('📊 BENCHMARK SUMMARY REPORT');
  console.log('======================================================');
  
  console.log('\n--- Sequential Chats ---');
  if (seqResults.length > 0) {
    const avgTotal = seqResults.reduce((acc, r) => acc + r.totalDuration, 0) / seqResults.length;
    const avgTtft = seqResults.filter(r => r.ttft !== null).reduce((acc, r) => acc + r.ttft, 0) / seqResults.length;
    console.log(`- Average Time to First Token (TTFT): ${avgTtft.toFixed(2)}s`);
    console.log(`- Average Total Response Duration:    ${avgTotal.toFixed(2)}s`);
    console.log(`- Successful Chats:                   ${seqResults.length}/${SEQUENTIAL_CHATS}`);
  } else {
    console.log('No successful sequential chats.');
  }

  console.log(`\n--- Concurrent Concurrency (${CONCURRENT_USERS} clients) ---`);
  const successCon = conResults.filter(r => r.success);
  if (successCon.length > 0) {
    const totalTimes = successCon.map(r => r.data.totalDuration);
    const minTime = Math.min(...totalTimes);
    const maxTime = Math.max(...totalTimes);
    const avgTime = totalTimes.reduce((acc, t) => acc + t, 0) / totalTimes.length;
    console.log(`- Success Rate:                  ${successCon.length}/${CONCURRENT_USERS} (${(successCon.length / CONCURRENT_USERS * 100).toFixed(0)}%)`);
    console.log(`- Minimum Response Duration:      ${minTime.toFixed(2)}s`);
    console.log(`- Maximum Response Duration:      ${maxTime.toFixed(2)}s`);
    console.log(`- Average Response Duration:      ${avgTime.toFixed(2)}s`);
    console.log(`- TaskQueue Behavior:             ${maxTime > minTime * 1.5 ? 'Tuần tự (TaskQueue hoạt động tốt)' : 'Song song'}`);
  } else {
    console.log(`- Success Rate:                  0/${CONCURRENT_USERS} (0%)`);
  }
  
  console.log('======================================================');
}

main().catch((err) => {
  console.error('Benchmark crashed:', err);
});
