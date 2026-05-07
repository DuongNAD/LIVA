import { safeFetch } from "@utils/HttpClient";

import { logger } from "@utils/logger";
export const metadata = {
  name: "send_zalo_bot",
  search_keywords: ["send_zalo_bot","send zalo bot","gửi","nhắn tin"],
  description:
    "CHỈ DÙNG để gửi BÁO CÁO, TÓM TẮT, THÔNG BÁO TỪ HỆ THỐNG cho CHÍNH BẢN THÂN NGƯỜI DÙNG (Dương) thông qua con Bot Liva Learning. TUYỆT ĐỐI KHÔNG dùng kỹ năng này để giao tiếp/nhắn tin với Bạn bè, Gia đình, Mẹ, hay người trong danh bạ cá nhân (Hãy dùng send_zalo_rpa cho những việc đó).",
  parameters: {
    type: "object",
    properties: {
      message: {
        type: "string",
        description: "Nội dung tin nhắn cần gửi (Message payload).",
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

    const finalMessage = args.message || args.text || args.content || "";

    if (!finalMessage.trim()) {
      logger.error(`[Skill: send_zalo_bot] LLM truyền tham số rỗng!`);
      return `LỖI TỪ API: Tham số nội dung bị rỗng! Bạn CHƯA ĐIỀN nội dung tóm tắt vào trong biến "message". Hãy GỌI LẠI công cụ này NGAY LẬP TỨC và chèn đúng nội dung vào.`;
    }

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
          const data = await updateRes.json() as any;
          if (
            data && // NOSONAR
            data.ok &&
            data.result && // NOSONAR
            data.result.message
          ) {
            userId = data.result.message.chat.id;
            logger.info(
              `[Skill: send_zalo_bot] Magic! Đã tự động bắt được User ID mới: ${userId}`,
            );
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
      const data = await response.json() as any;
      if (data && data.ok) { // NOSONAR
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

      const data = await response.json() as any;

      if (data && data.error === 0) { // NOSONAR
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
    logger.error({ err: errMsg }, `[Skill: send_zalo_bot] Lỗi ngoại lệ:`);
    return `Zalo Fetch Error: ${errMsg}`;
  }
};
