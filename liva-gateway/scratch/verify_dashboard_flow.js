import WebSocket from "ws";
import { unpack } from "msgpackr";

console.log("Connecting to LIVA Gateway WebSocket on ws://127.0.0.1:8082...");
const ws = new WebSocket("ws://127.0.0.1:8082");

let systemStatusReceived = false;
let skillsListReceived = false;

// Set a timeout of 10s
const timeoutId = setTimeout(() => {
  console.error("❌ Timeout waiting for dashboard flow response!");
  ws.close();
  process.exit(1);
}, 10000);

ws.on("open", () => {
  console.log("✅ Connected! Requesting system status and skills list...");
  
  // Request system status
  ws.send(JSON.stringify({ event: "get_system_status" }));
  
  // Request skills list
  ws.send(JSON.stringify({ event: "get_skills_list" }));
});

ws.on("message", (message, isBinary) => {
  let data;
  
  if (isBinary) {
    const buffer = Buffer.isBuffer(message) ? message : Buffer.from(message);
    if (buffer.length > 0) {
      const type = buffer[0];
      if (type === 0x02) {
        try {
          data = unpack(buffer.subarray(1));
        } catch (e) {
          console.error("❌ Error unpacking msgpack:", e.message);
          return;
        }
      } else {
        // Raw audio or other binary types are ignored
        return;
      }
    }
  } else {
    const rawText = message.toString();
    try {
      data = JSON.parse(rawText);
    } catch (e) {
      console.error("❌ Error parsing WS message as JSON:", e.message);
      return;
    }
  }
  
  if (!data) return;
  
  const { event, payload } = data;
  
  if (event === "system_status") {
    console.log("\n📬 Received 'system_status' event!");
    console.log("-----------------------------------------");
    console.log(`- Model: ${payload.model}`);
    console.log(`- Provider: ${payload.provider}`);
    console.log(`- Engine Mode: ${payload.engineMode}`);
    console.log(`- Uptime: ${payload.uptime.toFixed(1)}s`);
    console.log(`- RAM Heap: ${(payload.memoryUsage / 1024 / 1024).toFixed(1)} MB`);
    console.log(`- Network Status: ${payload.osStats?.networkStatus}`);
    console.log(`- AI Engine Health: ${payload.healthChecks?.aiEngine?.status} (${payload.healthChecks?.aiEngine?.detail})`);
    console.log(`- Voice Engine Health: ${payload.healthChecks?.voiceEngine?.status} (${payload.healthChecks?.voiceEngine?.detail})`);
    console.log(`- Gateway Health: ${payload.healthChecks?.gateway?.status} (${payload.healthChecks?.gateway?.detail})`);
    console.log(`- Skills Loaded: ${payload.healthChecks?.gateway?.wsClients} connection(s), ${payload.healthChecks?.gateway?.skillsLoaded} skills`);
    console.log(`- Geolocation Status: ${payload.healthChecks?.remoteControl?.enabled ? "Enabled" : "Disabled"}`);
    console.log(`- Telemetry log count: ${payload.telemetry?.length || 0}`);
    
    // Basic assertions
    if (payload.model && payload.healthChecks?.gateway?.status === "online") {
      systemStatusReceived = true;
    } else {
      console.error("❌ 'system_status' payload is missing crucial fields or gateway is offline!");
    }
  } 
  
  else if (event === "skills_list") {
    console.log("\n📬 Received 'skills_list' event!");
    console.log("-----------------------------------------");
    console.log(`- Total Skills: ${payload.skills?.length || 0}`);
    if (payload.skills && payload.skills.length > 0) {
      console.log(`- Sample Skill: ${payload.skills[0].name} (${payload.skills[0].enabled ? "Enabled" : "Disabled"})`);
      skillsListReceived = true;
    } else {
      console.error("❌ 'skills_list' payload has no skills!");
    }
  }
  
  // Check if both events received successfully
  if (systemStatusReceived && skillsListReceived) {
    console.log("\n🎉 All dashboard data flow verifications passed successfully!");
    clearTimeout(timeoutId);
    ws.close();
    process.exit(0);
  }
});

ws.on("error", (err) => {
  console.error("❌ WebSocket Error:", err);
  clearTimeout(timeoutId);
  process.exit(1);
});

ws.on("close", () => {
  console.log("WebSocket connection closed.");
});
