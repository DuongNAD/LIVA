import WebSocket from 'ws';
import { pack, unpack } from 'msgpackr';
import * as fs from 'fs';
import * as path from 'path';

const LOG_FILE = path.join(process.cwd(), 'test_report.log');
const CHECKPOINT_FILE = path.join(process.cwd(), 'checkpoint.json');

function appendLog(text) {
  fs.appendFileSync(LOG_FILE, `${new Date().toISOString()} - ${text}\n`);
  console.log(text);
}

function getCheckpoint() {
  if (fs.existsSync(CHECKPOINT_FILE)) {
    try {
      return JSON.parse(fs.readFileSync(CHECKPOINT_FILE, 'utf8'));
    } catch {}
  }
  return { lastTestedIndex: -1, successCount: 0, failCount: 0 };
}

function saveCheckpoint(data) {
  fs.writeFileSync(CHECKPOINT_FILE, JSON.stringify(data, null, 2));
}

async function runComprehensiveTests() {
  appendLog("\n========== STARTING COMPREHENSIVE SKILL TEST (DRY RUN) ==========");
  
  // 1. Fetch skills list
  const ws = new WebSocket('ws://127.0.0.1:8082');
  let allSkills = [];
  
  ws.on('open', () => {
    const payload = pack({ event: 'get_skills_list' });
    const buf = Buffer.concat([Buffer.from([0x02]), payload]);
    ws.send(buf);
  });

  const getSkillsPromise = new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("Timeout waiting for get_skills_list")), 10000);
    
    ws.on('message', (d, isBinary) => {
      if (isBinary) {
        const buf = Buffer.from(d);
        if (buf[0] === 0x02) {
          try {
            const data = unpack(buf.subarray(1));
            if (data.event === 'skills_list') {
              clearTimeout(timeout);
              allSkills = data.payload?.skills || data.payload || [];
              resolve();
            }
          } catch {}
        }
      }
    });
  });

  try {
    await getSkillsPromise;
    ws.removeAllListeners('message');
  } catch (err) {
    appendLog(`❌ ERROR fetching skills: ${err.message}`);
    process.exit(1);
  }

  // Filter active skills
  const activeSkills = allSkills.filter(s => s.status === 'active' || s.enabled);
  appendLog(`✅ Fetched ${allSkills.length} total skills. ${activeSkills.length} are active.`);

  let cp = getCheckpoint();
  
  for (let i = cp.lastTestedIndex + 1; i < activeSkills.length; i++) {
    const skill = activeSkills[i];
    appendLog(`\n${"=".repeat(60)}`);
    appendLog(`🧪 [${i + 1}/${activeSkills.length}] TEST SKILL: ${skill.name}`);
    appendLog(`${"=".repeat(60)}`);

    // We don't have the full JSON schema from `get_skills_list`, but we have description.
    // LIVA's LLM agent loop pulls the full JSON schema internally when injected into the prompt.
    // The prompt just needs to be highly specific.
    const prompt = `[DRY_RUN_TEST] You are LIVA. Please call the skill (tool) named "${skill.name}". Based on the JSON Schema of the skill provided in the system, generate completely random parameters but they MUST BE EXACTLY COMPLIANT WITH THE SCHEMA, do not leave any required fields blank. This is for testing purposes. Automatically call the tool right now. (DRY-RUN MODE)`;
    
    const result = await testSingleSkill(ws, skill.name, prompt);
    if (result.success) {
      cp.successCount++;
    } else {
      cp.failCount++;
    }
    
    cp.lastTestedIndex = i;
    saveCheckpoint(cp);
    
    // Wait slightly to let connections cool down
    await new Promise(r => setTimeout(r, 2000));
  }

  appendLog("\n========== COMPREHENSIVE SKILL TEST FINISHED ==========");
  appendLog(`Total Tested: ${cp.lastTestedIndex + 1} | Success: ${cp.successCount} | Failed: ${cp.failCount}`);
  process.exit(0);
}

function testSingleSkill(ws, skillName, prompt) {
  return new Promise((resolve) => {
    let chunks = [];
    let toolCalled = false;
    
    const timeout = setTimeout(() => {
      ws.removeAllListeners('message');
      appendLog(`  ❌ FAILED: Timeout after 90s. No tool call detected.`);
      resolve({ success: false });
    }, 90000);

    const messageHandler = (d, isBinary) => {
      if (!isBinary) return;
      const buf = Buffer.from(d);
      if (buf[0] !== 0x02) return;

      try {
        const msg = unpack(buf.subarray(1));
        
        if (msg.event === 'ai_stream_chunk') {
          const text = msg.payload?.textChunk || msg.payload?.data?.textChunk || '';
          if (text) chunks.push(text);
          if (text.includes("Mock data success for dry run test.")) {
            toolCalled = true;
          }
        }
        else if (msg.event === 'test_tool_execution') {
          const payloadStr = JSON.stringify(msg.payload);
          appendLog(`  🔧 TOOL EXECUTED: ${payloadStr}`);
          if (payloadStr.includes(skillName)) {
            toolCalled = true;
          }
        }
        else if (msg.event === 'ai_spoken_response' || msg.event === 'ai_response_done') {
           const payloadStr = JSON.stringify(msg.payload);
           if (payloadStr.includes("Mock data success for dry run test.") || chunks.join('').includes("Mock data success for dry run test.")) {
             toolCalled = true;
           }
           if (toolCalled) {
             clearTimeout(timeout);
             ws.removeListener('message', messageHandler);
             appendLog(`  ✅ SUCCESS: Tool "${skillName}" was successfully called and parsed by the system.`);
             resolve({ success: true });
           } else {
             // Let it timeout if the LLM hallucinated without calling the tool
           }
        }
      } catch {}
    };

    ws.on('message', messageHandler);
    
    // Send user voice command with isDryRun flag
    const payload = pack({ 
      event: 'user_voice_command', 
      payload: { text: prompt, isDryRun: true } 
    });
    const buf = Buffer.concat([Buffer.from([0x02]), payload]);
    ws.send(buf);
  });
}

runComprehensiveTests();
