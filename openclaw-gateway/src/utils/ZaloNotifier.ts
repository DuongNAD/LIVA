import { safeFetch } from "./HttpClient";
import { logger } from "./logger";

export async function notifyZalo(msg: string) {
  const token = process.env.ZALO_OA_ACCESS_TOKEN;
  let userId = process.env.ZALO_USER_ID;
  if (!token || !userId) return;

  try {
     const isBotToken = token.includes(":");
     const endpoint = isBotToken 
         ? `https://bot-api.zaloplatforms.com/bot${token}/sendMessage`
         : "https://openapi.zalo.me/v3.0/oa/message/cs";
     
     if (isBotToken) {
         await safeFetch(endpoint, {
             method: "POST",
             headers: { "Content-Type": "application/json" },
             body: JSON.stringify({ chat_id: userId, text: msg })
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
                message: { text: msg }
             })
         });
     }
  } catch(e: any) {
      logger.error(`[ZaloNotifier] Nhắn Zalo thất bại: ${e.message}`);
  }
}
