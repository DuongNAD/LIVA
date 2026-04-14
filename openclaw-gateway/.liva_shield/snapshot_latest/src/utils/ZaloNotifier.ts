import axios from "axios";

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
         await axios.post(endpoint, { chat_id: userId, text: msg }).catch(() => {});
     } else {
         await axios.post(endpoint, {
            recipient: { user_id: userId },
            message: { text: msg }
         }, { headers: { access_token: token } }).catch(() => {});
     }
  } catch(e: any) {
      console.error("[ZaloNotifier] Nhắn Zalo thất bại:", e.message);
  }
}
