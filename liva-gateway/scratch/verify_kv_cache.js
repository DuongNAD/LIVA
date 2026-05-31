import WebSocket from "ws";
import { unpack } from "msgpackr";

console.log("Connecting to LIVA Gateway WebSocket on ws://127.0.0.1:8082...");
const ws = new WebSocket("ws://127.0.0.1:8082");

let turn = 1;
let responseBuffer = "";

ws.on("open", () => {
  console.log("✅ Connected! Sending Turn 1 query...");
  
  const query = {
    event: "user_voice_command",
    payload: {
      text: "Cho mình hỏi là thời tiết ở khu vực quận Hoàn Kiếm, thành phố Hà Nội bây giờ thế nào, có mưa không nhỉ?"
    }
  };
  
  ws.send(JSON.stringify(query));
});

ws.on("message", (message, isBinary) => {
  if (isBinary) {
    const buffer = Buffer.isBuffer(message) ? message : Buffer.from(message);
    if (buffer.length > 0) {
      const type = buffer[0];
      if (type === 0x02) {
        try {
          const unpacked = unpack(buffer.subarray(1));
          const { event, payload } = unpacked;
          
          if (event === "ai_stream_chunk") {
            const chunk = payload.text || "";
            responseBuffer += chunk;
            process.stdout.write(chunk);
          } else if (event === "ai_spoken_response") {
            console.log("\n\n[Full Response Received]:", payload.text);
            
            if (turn === 1) {
              console.log("\n--- Waiting 3 seconds before sending Turn 2 query to test KV Cache... ---");
              turn = 2;
              responseBuffer = "";
              setTimeout(() => {
                console.log("Sending Turn 2 query...");
                const nextQuery = {
                  event: "user_voice_command",
                  payload: {
                    text: "Thế còn ngày mai và mấy ngày tới thì thời tiết có biến động gì lớn không?"
                  }
                };
                ws.send(JSON.stringify(nextQuery));
              }, 3000);
            } else {
              console.log("\nBoth turns complete. Closing connection.");
              ws.close();
              process.exit(0);
            }
          }
        } catch (e) {
          console.error("Error unpacking msgpack:", e);
        }
      }
    }
  } else {
    console.log("Received text message:", message.toString());
  }
});

ws.on("error", (err) => {
  console.error("WebSocket Error:", err);
});

ws.on("close", () => {
  console.log("WebSocket connection closed.");
});
