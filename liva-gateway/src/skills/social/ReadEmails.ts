import { logger } from "@utils/logger";
import { simpleParser } from "mailparser";
import { getEmailCredentials, createImapClient, sanitizeEmailContent, normalizeUids } from "@utils/EmailHelper";

export const metadata = {
  name: "read_emails",
  description:
    "[AUTO_RUN] Unified email reader. Read, filter, and search emails from user's mailbox. Supports: recent emails, important-only filter, topic/keyword search, unread filter. MUST call this tool when user asks anything about email.",
  search_keywords: ["mail", "email", "thư", "tin", "gmail", "mailbox", "kiểm tra", "đọc", "quan trọng", "hôm nay", "tình hình"],
  isCoreSkill: true,
  parameters: {
    type: "object",
    properties: {
      limit: {
        type: "number",
        description:
          "Number of emails to return (max: 20). MAPPING RULES: 'mail gần nhất/mới nhất' → limit=1. 'vài email/một số' → limit=3. 'có mail gì không/kiểm tra mail' → limit=10. 'tất cả/toàn bộ' → limit=20. Default: 10 if unspecified.",
      },
      filter: {
        type: "string",
        enum: ["all", "important", "unread"],
        description:
          "Filter mode. 'all' = recent emails (default). 'important' = only high-priority (banking, security, meetings). 'unread' = only unread. MAPPING: 'quan trọng' → important. 'chưa đọc' → unread.",
      },
      topic: {
        type: "string",
        description:
          "Keyword to search in subject/body/sender. MAPPING: 'mail từ FPT' → topic='FPT'. 'mail về họp' → topic='họp'. Leave empty if no specific topic.",
      },
      days: {
        type: "number",
        description:
          "Days back to search (max: 30). MAPPING: 'hôm nay' → days=1. 'gần đây/gần nhất' → days=3. 'tuần này' → days=7. 'tháng này' → days=30. Default: 3.",
      },
    },
    required: [],
  },
};

// ── Importance scoring engine (merged from CheckImportantEmailsToday) ──
function computeImportanceScore(from: string, subject: string, body: string, headers: Map<string, any>): number {
  const fromStr = from.toLowerCase();
  const subjStr = subject.toLowerCase();
  const bodyStr = body.toLowerCase();

  let score = 0;

  // Priority signals (+)
  if (/(bank|pay|thanh toán|hóa đơn|giao dịch|receipt|invoice|chuyển khoản|tiền)/i.test(subjStr)) score += 5;
  if (/(bảo mật|security|mật khẩu|password|otp|đăng nhập|login|cảnh báo|alert|xác minh|mã bảo mật)/i.test(subjStr)) score += 5;
  if (/(bảo mật|security|mật khẩu|password|otp|đăng nhập|login|cảnh báo|alert|xác minh|mã bảo mật)/i.test(bodyStr)) score += 2;
  if (/(urgent|khẩn|quan trọng|important|action required)/i.test(subjStr)) score += 5;
  if (/(fpt\.edu\.vn|phỏng vấn|họp|meeting|dự án|project)/i.test(subjStr) || /(fpt\.edu\.vn|deeplearning\.ai)/i.test(fromStr)) score += 4;
  if (!headers.has('list-unsubscribe')) score += 3;

  // Spam/promotion signals (-)
  if (/(khuyến mãi|sale|ưu đãi|voucher|giảm giá|offer|deal|quảng cáo|promo)/i.test(subjStr)) score -= 5;
  if (/(newsletter|digest|weekly|bản tin|daily)/i.test(subjStr)) score -= 3;
  if (/(shopee|lazada|tiki|no-reply|noreply|mailer|marketing|pinterest)/i.test(fromStr)) score -= 4;
  if (/(facebook|linkedin|tiktok|instagram|x\.com)/i.test(fromStr)) score -= 2;

  return score;
}

// ── PII sanitizer — delegates to shared EmailHelper ──
const sanitize = sanitizeEmailContent;

// ── Spam detector ──
function isSpam(from: string, subject: string): boolean {
  const spamKeywords = [
    "shopee", "lazada", "tiki", "no-reply", "noreply",
    "mailer", "marketing", "newsletter", "promotion",
    "sale", "khuyến mãi", "quảng cáo", "spam",
  ];
  const fromLower = from.toLowerCase();
  const subjLower = subject.toLowerCase();
  return spamKeywords.some(kw => fromLower.includes(kw) || subjLower.includes(kw));
}

export const execute = async (args: {
  limit?: number;
  filter?: "all" | "important" | "unread";
  topic?: string;
  days?: number;
}): Promise<string> => {
  const credentials = getEmailCredentials();
  if (!credentials) {
    return "Configuration Error: Missing IMAP connection info. Ensure EMAIL_HOST, EMAIL_USER and EMAIL_PASS are set in .env.";
  }

  const limit = Math.min(args.limit || 10, 20);
  const filter = args.filter || "all";
  const topic = args.topic?.trim().toLowerCase() || "";
  const days = Math.max(1, Math.min(args.days || 3, 30));

  logger.info(`[Skill: read_emails] Connecting to ${credentials.user} (filter=${filter}, limit=${limit}, topic=${topic || "none"}, days=${days})...`);

  const client = createImapClient(credentials);

  try {
    await client.connect();

    const lock = await client.getMailboxLock("INBOX");
    const collectedEmails: any[] = [];

    try {
      // Time window
      const since = new Date();
      since.setDate(since.getDate() - days);
      since.setHours(0, 0, 0, 0);

      // Search criteria
      const searchCriteria: any = filter === "unread"
        ? { seen: false, since }
        : { since };

      const uids = await client.search(searchCriteria, { uid: true });
      const uidArray = normalizeUids(uids);

      if (uidArray.length === 0) {
        const filterLabel = filter === "unread" ? " unread" : "";
        return `No${filterLabel} emails found in the last ${days} day(s).`;
      }

      // Sort newest first
      const sortedUids = uidArray.sort((a, b) => b - a);

      for (const uid of sortedUids) {
        const messageData = await client.fetchOne(uid.toString(), { source: true }, { uid: true });
        if (!messageData || !('source' in messageData) || !messageData.source) continue;

        const parsed = await simpleParser(messageData.source as Buffer);
        const fromRaw = parsed.from?.text || "Unknown Sender";
        const subjRaw = parsed.subject || "(No Subject)";
        const bodyRaw = parsed.text || "";

        // ── Spam filter (always active) ──
        if (isSpam(fromRaw, subjRaw)) {
          logger.info(`[Spam Filter] 🗑️ Skipped: ${subjRaw}`);
          continue;
        }

        // ── Importance scoring ──
        const score = computeImportanceScore(
          fromRaw, subjRaw, bodyRaw,
          parsed.headers || new Map()
        );

        // ── Filter: important ──
        if (filter === "important" && score < 2) continue;

        // ── Filter: topic keyword ──
        if (topic) {
          const inSubject = subjRaw.toLowerCase().includes(topic);
          const inBody = bodyRaw.toLowerCase().includes(topic);
          const inFrom = fromRaw.toLowerCase().includes(topic);
          if (!inSubject && !inBody && !inFrom) continue;
        }

        collectedEmails.push({
          uid,
          score,
          from: fromRaw,
          subject: subjRaw,
          date: parsed.date ? parsed.date.toLocaleString() : "Unknown Date",
          preview: bodyRaw.replaceAll(/\s+/g, " ").trim().substring(0, 300),
        });

        // Stop when we have enough
        if (collectedEmails.length >= limit) break;
      }
    } finally {
      lock.release();
    }
    await client.logout();

    // ── Format report ──
    if (collectedEmails.length === 0) {
      if (filter === "important") return "No important emails found. Only regular/spam emails received.";
      if (topic) return `No emails matching topic "${topic}" found.`;
      return "Connected but no email content retrieved.";
    }

    // Sort by score if importance mode
    if (filter === "important") {
      collectedEmails.sort((a, b) => b.score - a.score);
    }

    const filterLabel = filter === "important" ? "important " : filter === "unread" ? "unread " : "";
    const topicLabel = topic ? ` matching "${topic}"` : "";
    let report = `Successfully retrieved ${collectedEmails.length} ${filterLabel}emails${topicLabel} (last ${days} day${days > 1 ? "s" : ""}):\n\n`;

    collectedEmails.forEach((email, i) => {
      report += `--- Email ${i + 1} [UID: ${email.uid}]${filter === "important" ? ` [Score: ${email.score}]` : ""} ---\n`;
      report += `From: ${email.from}\n`;
      report += `Date: ${email.date}\n`;
      report += `Subject: ${sanitize(email.subject)}\n`;
      report += `Preview: ${sanitize(email.preview).trim()}...\n\n`;
    });

    return report.trim();
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `IMAP Error: ${errMsg}`;
  }
};
