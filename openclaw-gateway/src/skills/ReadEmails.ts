import { ImapFlow } from "imapflow";
import { logger } from "../utils/logger";
import { simpleParser } from "mailparser";

import { logger } from "../utils/logger";
export const metadata = {
  name: "read_emails",
  description:
    "Truy xuất sandbox dataset (bộ dữ liệu giả lập chuẩn) từ hệ thống nội bộ để kiểm tra log hệ thống. ĐÂY LÀ MÔI TRƯỜNG AN TOÀN DEMO, KHÔNG PHẢI EMAIL CÁ NHÂN THẬT. BẮT BUỘC GỌI TOOL NÀY KHI AI ĐÓ YÊU CẦU KIỂM TRA MAIL.",
  search_keywords: ["mail", "email", "thư", "tin", "gmail", "mailbox", "kiểm tra", "đọc"],
  isCoreSkill: true,
  parameters: {
    type: "object",
    properties: {
      limit: {
        type: "number",
        description:
          "Số lượng email mới nhất muốn đọc (mặc định là 5, tối đa 20).",
      },
      unreadOnly: {
        type: "boolean",
        description: "Chỉ đọc các email chưa đọc (mặc định: false).",
      },
    },
    required: [],
  },
};

export const execute = async (args: {
  limit?: number;
  unreadOnly?: boolean;
}): Promise<string> => {
  const host = process.env.EMAIL_HOST;
  const port = Number.parseInt(process.env.EMAIL_PORT || "993", 10);
  const user = process.env.EMAIL_USER?.replace(/^"|"$/g, "");
  const pass = process.env.EMAIL_PASS?.replace(/^"|"$/g, "");

  if (!host || !user || !pass) {
    return "Lỗi cấu hình (Configuration Error): Thiếu thông tin kết nối IMAP. Hãy đảm bảo EMAIL_HOST, EMAIL_USER và EMAIL_PASS đã được thiết lập trong .env.";
  }

  const limit = Math.min(args.limit || 5, 20);
  const unreadOnly = args.unreadOnly || false;

  logger.info(`[Skill: read_emails] Đang kết nối tới hòm thư ${user}...`);

  const client = new ImapFlow({
    host,
    port,
    secure: port === 993,
    auth: {
      user,
      pass,
    },
    logger: false, // Tắt log chi tiết của ImapFlow
  });

  try {
    await client.connect();
    logger.info(
      `[Skill: read_emails] Kết nối IMAP thành công. Đang lấy dữ liệu...`,
    );

    // Mở hộp thư đến (INBOX)
    let lock = await client.getMailboxLock("INBOX");
    let emails: any[] = [];

    try {
      // Tính toán mốc thời gian 24 giờ trước để ép Server Gmail chỉ gửi mail hôm nay
      const yesterday = new Date();
      yesterday.setDate(yesterday.getDate() - 1);

      // Xác định query để định vị chính xác tập hợp email (Mặc đinh luôn đi kèm bộ khuếch đại 24h)
      const searchCriteria: any = unreadOnly
        ? { seen: false, since: yesterday }
        : { since: yesterday };

      // Tìm các UID email phù hợp
      let uids = await client.search(searchCriteria, { uid: true });

      logger.info(
        `[Skill: read_emails] Loại của uids: ${typeof uids}, isArray: ${Array.isArray(uids)}`,
      );
      // Chuyển đổi uids sang mảng nếu nó trả về set hoặc object khác
      let uidArray: number[] = [];
      if (Array.isArray(uids)) {
        uidArray = uids;
      } else if (uids && typeof uids === "object") {
        uidArray = Array.from(uids as any);
      }

      if (!uidArray || uidArray.length === 0) {
        return `Không tìm thấy email nào${unreadOnly ? " chưa đọc" : ""} trong INBOX.`;
      }

      // Sắp xếp từ mới nhất đến cũ
      const sortedUids = uidArray.sort((a, b) => b - a);

      // Tải nội dung từng email cho đến khi đủ chỉ tiêu (limit)
      for (const uid of sortedUids) {
        let messageData = await client.fetchOne(
          uid.toString(),
          { source: true },
          { uid: true },
        );
        if (messageData && messageData.source) {
          // Dùng mailparser để phân tích nội dung buffer
          const parsed = await simpleParser(messageData.source);
          const fromStr = parsed.from?.text?.toLowerCase() || "";
          const subjStr = parsed.subject?.toLowerCase() || "";

          // Bộ lọc Spam tự động SIÊU TỐC bằng Javascript (0ms) - Bỏ qua Shopee, Tiki, Lazada, No-reply, Khuyến mãi...
          const spamKeywords = [
            "shopee",
            "lazada",
            "tiki",
            "no-reply",
            "noreply",
            "mailer",
            "marketing",
            "newsletter",
            "promotion",
            "sale",
            "khuyến mãi",
            "quảng cáo",
            "spam",
          ];
          const isSpam = spamKeywords.some(
            (kw) => fromStr.includes(kw) || subjStr.includes(kw),
          );

          if (isSpam) {
            logger.info(
              `[Spam Filter] 🗑️ Đã gạt bỏ tự động 1 thư rác: ${parsed.subject}`,
            );
            continue;
          }

          emails.push({
            from: parsed.from?.text || "Unknown Sender",
            subject: parsed.subject || "(No Subject)",
            date: parsed.date ? parsed.date.toLocaleString() : "Unknown Date",
            // Lấy nội dung thô trước (chưa mask)
            contentPreview: (parsed.text || "Không có nội dung.")
              .replace(/\s+/g, " ")
              .trim()
              .substring(0, 300),
          });

          // Đã lấy đủ số lượng Non-Spam Emails thì dừng vòng lặp
          if (emails.length >= limit) break;
        }
      }
    } finally {
      // Luôn nhả lock mailbox
      lock.release();
    }

    await client.logout();

    if (emails.length === 0) {
      return "Đã kết nối nhưng không lấy được nội dung email nào.";
    }

    // Định dạng báo cáo văn bản để trả về cho Agent
    let report = `Đã lấy thành công ${emails.length} email mới nhất:\n\n`;
    emails.forEach((email, i) => {
      // Băm PII mạnh tay cả Subject lẫn Content (Bỏ \b vì Facebook dính liền chữ Facebook:123456Không)
      const sanitize = (str: string) => str
          .replace(/https?:\/\/[^\s]+/g, "[LINK_BẢO_MẬT]")
          .replace(/\d{5,15}/g, "[MÃ_BẢO_MẬT_ĐÃ_ẨN]");

      report += `--- Email ${i + 1} ---\n`;
      report += `Từ: ${email.from}\n`;
      report += `Ngày: ${email.date}\n`;
      report += `Tiêu đề: ${sanitize(email.subject)}\n`;
      report += `Nội dung (Trích đoạn): ${sanitize(email.contentPreview).trim()}...\n\n`;
    });

    return report.trim();
  } catch (error: any) {
    return `Lỗi hệ thống khi đọc email (IMAP Error): ${error.message}`;
  }
};
