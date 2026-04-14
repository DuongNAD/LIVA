import { ImapFlow } from "imapflow";
import { simpleParser } from "mailparser";

export const metadata = {
  name: "check_important_emails_today",
  description:
    "Tìm kiếm và xuất danh sách rút gọn TOÀN BỘ email nhận được TRONG NGÀY HÔM NAY có độ quan trọng cao. BẮT BUỘC GỌI TOOL NÀY khi user muốn biết tình hình email hoặc hỏi 'hôm nay có mail nào quan trọng không'. ĐÂY LÀ MÔI TRƯỜNG AN TOÀN DEMO.",
  search_keywords: ["hôm nay", "quan trọng", "tình hình", "kiểm tra nhanh", "mail", "email"],
  isCoreSkill: true,
  parameters: {
    type: "object",
    properties: {},
    required: [],
  },
};

export const execute = async (): Promise<string> => {
  const host = process.env.EMAIL_HOST;
  const port = parseInt(process.env.EMAIL_PORT || "993", 10);
  const user = process.env.EMAIL_USER?.replace(/^"|"$/g, "");
  const pass = process.env.EMAIL_PASS?.replace(/^"|"$/g, "");

  if (!host || !user || !pass) {
    return "Lỗi cấu hình (Configuration Error): Thiếu thông tin kết nối IMAP.";
  }

  console.log(`[Skill: check_important_emails_today] Đang kết nối tới hòm thư ${user}...`);

  const client = new ImapFlow({
    host,
    port,
    secure: port === 993,
    auth: { user, pass },
    logger: false, 
  });

  try {
    await client.connect();
    console.log(`[Skill: check_important_emails_today] Kết nối IMAP thành công. Lấy dữ liệu 24h qua...`);

    let lock = await client.getMailboxLock("INBOX");
    let importantEmails: any[] = [];

    try {
      // Ép khung thời gian MẶC ĐỊNH LÀ ĐẦU NGÀY HÔM NAY (00:00:00)
      const startOfToday = new Date();
      startOfToday.setHours(0, 0, 0, 0);

      // Tìm ALL email trong ngày
      let uids = await client.search({ since: startOfToday }, { uid: true });

      let uidArray: number[] = [];
      if (Array.isArray(uids)) uidArray = uids;
      else if (uids && typeof uids === "object") uidArray = Array.from(uids as any);

      if (uidArray.length === 0) {
        return `Không có bất kỳ email nào được gửi đến trong ngày hôm nay. (No emails today).`;
      }

      // Duyệt từ MỚI nhất đến CŨ nhất
      const sortedUids = uidArray.sort((a, b) => b - a);

      for (const uid of sortedUids) {
        let messageData = await client.fetchOne(uid.toString(), { source: true }, { uid: true });
        if (messageData && messageData.source) {
          const parsed = await simpleParser(messageData.source);
          const fromRaw = parsed.from?.text || "Unknown Sender";
          const fromStr = fromRaw.toLowerCase();
          const subjStr = (parsed.subject || "").toLowerCase();
          const bodyStr = (parsed.text || "").toLowerCase();

          let score = 0;

          // 1. TÍN HIỆU ƯU TIÊN (+ ĐIỂM)
          // Tài chính, Giao dịch
          if (/(bank|pay|thanh toán|hóa đơn|giao dịch|receipt|invoice|chuyển khoản|tiền)/i.test(subjStr)) score += 5;
          // Bảo mật, Xác thực
          if (/(bảo mật|security|mật khẩu|password|otp|đăng nhập|login|cảnh báo|alert|xác minh|mã bảo mật)/i.test(subjStr)) score += 5;
          if (/(bảo mật|security|mật khẩu|password|otp|đăng nhập|login|cảnh báo|alert|xác minh|mã bảo mật)/i.test(bodyStr)) score += 2;
          // Công việc, Trường học, Gấp
          if (/(urgent|khẩn|quan trọng|important|action required)/i.test(subjStr)) score += 5;
          if (/(fpt\.edu\.vn|phỏng vấn|họp|meeting|dự án|project)/i.test(subjStr) || /(fpt\.edu\.vn|deeplearning\.ai)/i.test(fromStr)) score += 4;
          
          // Ưu tiên Thư Gửi Cá Nhân trực tiếp (Human to Human) (Không có nhãn Mass Marketing List-Unsubscribe)
          if (!parsed.headers.has('list-unsubscribe')) score += 3;

          // 2. TÍN HIỆU RÁC/QUẢNG CÁO (- ĐIỂM)
          if (/(khuyến mãi|sale|ưu đãi|voucher|giảm giá|offer|deal|quảng cáo|promo)/i.test(subjStr)) score -= 5;
          if (/(newsletter|digest|weekly|bản tin|daily)/i.test(subjStr)) score -= 3;
          if (/(shopee|lazada|tiki|no-reply|noreply|mailer|marketing|pinterest)/i.test(fromStr)) score -= 4;
          if (/(facebook|linkedin|tiktok|instagram|x\.com)/i.test(fromStr)) score -= 2;

          // Nếu Pass mốc điểm chuẩn >= 2 => Coi như là Quan Trọng
          if (score >= 2) {
             importantEmails.push({
               score,
               from: fromRaw,
               subject: parsed.subject || "(No Subject)",
               date: parsed.date ? parsed.date.toLocaleString() : "Unknown Date",
               // Nén cực chặt Text để phòng ngừa nghẽn RAM LLM (Chỉ 100 char)
               snippet: (parsed.text || "Không có nội dung.")
                  .replace(/\s+/g, " ")
                  .trim()
                  .substring(0, 100),
             });
          }
        }
      }
    } finally {
      lock.release();
    }
    await client.logout();

    if (importantEmails.length === 0) {
      return "Hôm nay nhận được một vài email rác/bình thường, nhưng KHÔNG CÓ BẤT KỲ EMAIL NÀO QUAN TRỌNG ĐÁNG CHÚ Ý.";
    }

    // Sếp loại lại theo Điểm số Tầm quan trọng (Giới hạn tối đa 15 cái quan trọng nhất để tránh chết VRAM)
    importantEmails.sort((a, b) => b.score - a.score);
    const topEmails = importantEmails.slice(0, 15);

    let report = `[REPORT] Đã quét toàn kho thư ngày hôm nay. Lọc được ${topEmails.length} email có TẦM QUAN TRỌNG CAO (Đã ẩn các mã bảo mật PII an toàn):\n\n`;
    
    topEmails.forEach((email, i) => {
      // PII Censor Masking để bảo vệ Model 4B
      const sanitize = (str: string) => str
          .replace(/https?:\/\/[^\s]+/g, "[LINK_BẢO_MẬT]")
          .replace(/\d{5,15}/g, "[MÃ_BẢO_MẬT_ĐÃ_ẨN]");

      report += `[${i + 1}] Từ: ${email.from} | Điểm: ${email.score}\n`;
      report += `Tiêu đề: ${sanitize(email.subject)}\n`;
      report += `Nội dung: ${sanitize(email.snippet).trim()}...\n\n`;
    });

    return report.trim();
  } catch (error: any) {
    return `Lỗi truy xuất thư (IMAP Error): ${error.message}`;
  }
};
