import { safeFetch } from "@utils/HttpClient";
import { logger } from "@utils/logger";
import { HITLGuard } from "@security/HITLGuard";

export const metadata = {
  name: "send_zalo_bot",
  search_keywords: ["send_zalo_bot","send zalo bot","gửi","nhắn tin","zalo","báo cáo","report","notify","thông báo","gửi báo cáo"],
  description:
    "[ASK_FIRST] ONLY for sending REPORTS, SUMMARIES, or SYSTEM NOTIFICATIONS to THE USER THEMSELVES via Zalo Bot. NEVER use this for messaging friends/family/contacts (use send_zalo_rpa for that).",
  parameters: {
    type: "object",
    properties: {
      message: {
        type: "string",
        description: "Message content to send (Message payload).",
      },
    },
    required: ["message"],
  },
};

export const execute = async (args: {
  message?: string;
  text?: string;
  content?: string;
}): Promise<string> => {
  try {
    logger.info(
      `[Skill: send_zalo_bot] Đang chuẩn bị gửi tin nhắn qua Zalo Bot API...`,
    );

    const rawMessage = args.message || args.text || args.content || "";

    if (!rawMessage.trim()) {
      logger.error(`[Skill: send_zalo_bot] LLM truyền tham số rỗng!`);
      return `LỖI TỪ API: Tham số nội dung bị rỗng! Bạn CHƯA ĐIỀN nội dung tóm tắt vào trong biến "message". Hãy GỌI LẠI công cụ này NGAY LẬP TỨC và chèn đúng nội dung vào.`;
    }



    // [AUTO-TAG] Append #Liva so recipients know this is AI-generated
    const finalMessage = rawMessage.includes("#Liva") ? rawMessage : `${rawMessage}\n\n#Liva`;


    const accessToken = process.env.ZALO_OA_ACCESS_TOKEN;
    let userId = process.env.ZALO_USER_ID;

    if (!accessToken || accessToken.includes("NHẬP_TOKEN")) {
      return `Lỗi cấu hình Zalo Bot (Config Error): Chưa có ZALO_OA_ACCESS_TOKEN trong file .env.`;
    }

    const isBotCreatorToken = accessToken.includes(":");

    if (isBotCreatorToken) {
      // HỆ SINH THÁI MỚI: BOT CREATOR (API giống Telegram)
      if (!userId || userId.includes("NHẬP_USER_ID")) {
        logger.info(
          `[Skill: send_zalo_bot] Dùng Bot mới nhưng chưa có User ID. Đang quét API dò tìm ID tự động...`,
        );
        try {
          const updateRes = await safeFetch(
            `https://bot-api.zaloplatforms.com/bot${accessToken}/getUpdates`,
            {
               method: "POST",
               headers: { "Content-Type": "application/json" },
               body: JSON.stringify({ timeout: "5" })
            },
            7000
          );
          const data = await updateRes.json() as Record<string, unknown>;
          if (data && data.ok && data.result) {
            const updates = Array.isArray(data.result) ? data.result : [data.result];
            for (const update of updates) {
              if (update && update.message && update.message.chat) {
                userId = String(update.message.chat.id);
                break;
              }
            }
            if (userId) {
              logger.info(
                `[Skill: send_zalo_bot] Magic! Đã tự động bắt được User ID mới: ${userId}`,
              );
            } else {
              return `Lỗi hệ thống Bot Mới: Không tìm thấy User ID. Vui lòng nhắn 1 tin bất kỳ cho Bot rồi thử lại để hệ thống tự bắt ID.`;
            }
          } else {
            return `Lỗi hệ thống Bot Mới: Không tìm thấy User ID. Vui lòng nhắn 1 tin bất kỳ cho Bot rồi thử lại để hệ thống tự bắt ID.`;
          }
        } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          return `Lỗi tự động dò User ID (Token copy sai hoặc API lỗi): ${errMsg}`;
        }
      }

      const endpoint = `https://bot-api.zaloplatforms.com/bot${accessToken}/sendMessage`;
      const payload = {
        chat_id: userId,
        text: finalMessage.substring(0, 2000), // Zalo giới hạn 2000 ký tự per message
      };

      const response = await safeFetch(endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload)
      });
      const data = await response.json() as Record<string, unknown>;
      if (data && data.ok) { // NOSONAR
        logger.info(
          `[Skill: send_zalo_bot] Gửi tin nhắn thành công qua hệ mới Bot Creator!`,
        );
        return `Hoàn tất (Success): Đã gửi tin nhắn thành công qua Zalo Bot Creator.`;
      } else {
        return `Zalo Bot API Error: ${data.description}`;
      }
    } else {
      // HỆ SINH THÁI CŨ: OFFICIAL ACCOUNT
      if (!userId || userId.includes("NHẬP_USER_ID")) {
        return `Lỗi cấu hình Zalo OA (Config Error): Chưa có ZALO_USER_ID trong file .env.`;
      }

      const endpoint = "https://openapi.zalo.me/v3.0/oa/message/cs";
      const payload = {
        recipient: { user_id: userId },
        message: { text: args.message },
      };

      const response = await safeFetch(endpoint, {
        method: "POST",
        headers: {
          access_token: accessToken,
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload)
      });

      const data = await response.json() as Record<string, unknown>;

      if (data && data.error === 0) { // NOSONAR
        logger.info(
          `[Skill: send_zalo_bot] Gửi tin nhắn thành công qua Zalo OA!`,
        );
        return `Hoàn tất (Success): Đã gửi tin nhắn thành công qua Zalo OA.`;
      } else {
        return `Zalo OA API Error: ${data.message || "Lỗi không xác định từ API"}`;
      }
    }
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    if (errMsg === "REJECTED_BY_TIMEOUT" || errMsg === "REJECTED_BY_USER") {
      return `[HỆ THỐNG BẢO MẬT TỪ CHỐI]: Yêu cầu gửi tin nhắn bị từ chối bởi người dùng hoặc quá thời gian phê duyệt (timeout 300s).`;
    }
    logger.error({ err: errMsg }, `[Skill: send_zalo_bot] Lỗi ngoại lệ:`);
    return `Zalo Fetch Error: ${errMsg}`;
  }
};
