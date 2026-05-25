import dotenv from "dotenv";
import path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load env
dotenv.config({ path: path.join(__dirname, "..", ".env") });

import { safeFetch } from "../src/utils/HttpClient.js";

async function test() {
  const token = process.env.ZALO_OA_ACCESS_TOKEN;
  console.log("Using Token:", token ? `${token.substring(0, 10)}...` : "None");
  if (!token) {
    console.error("No token found!");
    return;
  }
  
  try {
    console.log("Fetching updates from Zalo Bot API...");
    const updateRes = await safeFetch(
      `https://bot-api.zaloplatforms.com/bot${token}/getUpdates`,
      {
         method: "POST",
         headers: { "Content-Type": "application/json" },
         body: JSON.stringify({ timeout: "5" })
      },
      7000
    );
    const data = await updateRes.json() as any;
    console.log("Response data:", JSON.stringify(data, null, 2));
    
    if (data && data.ok && data.result) {
      const updates = Array.isArray(data.result) ? data.result : [data.result];
      let foundUserId = "";
      for (const update of updates) {
        if (update && update.message && update.message.chat) {
          foundUserId = update.message.chat.id;
          console.log("Found User ID:", foundUserId);
        }
      }
      
      if (foundUserId) {
        console.log("Sending test message to Zalo user:", foundUserId);
        const sendRes = await safeFetch(
          `https://bot-api.zaloplatforms.com/bot${token}/sendMessage`,
          {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
              chat_id: foundUserId,
              text: "Chào sếp! LIVA đã kết nối Zalo thành công và tự động nhận diện được ID của sếp nhé. 🤖✨ #Liva"
            })
          }
        );
        const sendData = await sendRes.json() as any;
        console.log("Send response:", sendData);
      } else {
        console.log("No messages found in updates. Please send a message to the bot first.");
      }
    } else {
      console.log("No updates returned or API error.");
    }
  } catch (err) {
    console.error("Error:", err);
  }
}

test();
