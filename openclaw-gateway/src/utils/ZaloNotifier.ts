import { safeFetch } from "./HttpClient";
import { logger } from "./logger";

export async function notifyZalo(msg: string) {
  const token = process.env.ZALO_OA_ACCESS_TOKEN;
  const userId = process.env.ZALO_USER_ID;
  if (!token || !userId) return;

  // [AUTO-TAG] Append #Liva signature so recipients know this is AI-generated
  const taggedMsg = msg.includes("#Liva") ? msg : `${msg}\n\n#Liva`;

  try {
     const isBotToken = token.includes(":");
     const endpoint = isBotToken 
         ? `https://bot-api.zaloplatforms.com/bot${token}/sendMessage`
         : "https://openapi.zalo.me/v3.0/oa/message/cs";
     
     if (isBotToken) {
         await safeFetch(endpoint, {
             method: "POST",
             headers: { "Content-Type": "application/json" },
             body: JSON.stringify({ chat_id: userId, text: taggedMsg })
         });
     } else {
         await safeFetch(endpoint, {
             method: "POST",
             headers: { 
                 access_token: token,
                 "Content-Type": "application/json" 
             },
             body: JSON.stringify({
                recipient: { user_id: userId },
                message: { text: taggedMsg }
             })
         });
     }
  } catch(e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[ZaloNotifier] Nhắn Zalo thất bại: ${errMsg}`);
  }
}
