import { ImapFlow } from "imapflow";
import { logger } from "@utils/logger";
import { simpleParser } from "mailparser";

export const metadata = {
  name: "read_email_detail",
  description:
    "[AUTO_RUN] Read the FULL content of a specific email by its UID. Use this AFTER calling read_emails to get the detailed view of one email the user is interested in. The UID is shown in the read_emails output as [UID: xxx].",
  search_keywords: ["mail", "email", "chi tiết", "detail", "đọc", "xem", "nội dung"],
  parameters: {
    type: "object",
    properties: {
      uid: {
        type: "number",
        description:
          "The UID of the email to read in full. This is obtained from the read_emails output (shown as [UID: xxx]).",
      },
    },
    required: ["uid"],
  },
};

// ── PII sanitizer (shared pattern) ──
function sanitize(str: string): string {
  return str
    .replaceAll(/https?:\/\/[^\s]+/g, "[SECURE_LINK]")
    .replaceAll(/\d{5,15}/g, "[REDACTED_CODE]");
}

export const execute = async (args: { uid: number }): Promise<string> => {
  const host = process.env.EMAIL_HOST;
  const port = Number.parseInt(process.env.EMAIL_PORT || "993", 10);
  const user = process.env.EMAIL_USER?.replaceAll(/^"|"$/g, "");
  const pass = process.env.EMAIL_PASS?.replaceAll(/^"|"$/g, "");

  if (!host || !user || !pass) {
    return "Configuration Error: Missing IMAP connection info. Ensure EMAIL_HOST, EMAIL_USER and EMAIL_PASS are set in .env.";
  }

  if (!args.uid || typeof args.uid !== "number") {
    return "Error: Missing or invalid UID. Please provide a valid email UID from the read_emails output.";
  }

  logger.info(`[Skill: read_email_detail] Fetching full content for UID ${args.uid}...`);

  const client = new ImapFlow({
    host,
    port,
    secure: port === 993,
    auth: { user, pass },
    logger: false,
  });

  try {
    await client.connect();

    const lock = await client.getMailboxLock("INBOX");

    try {
      const messageData = await client.fetchOne(
        args.uid.toString(),
        { source: true },
        { uid: true }
      );

      if (!messageData || !('source' in messageData) || !messageData.source) {
        return `Email with UID ${args.uid} not found or has been deleted.`;
      }

      const parsed = await simpleParser(messageData.source as Buffer);

      // ── Build comprehensive report ──
      const from = parsed.from?.text || "Unknown Sender";
      const to = parsed.to
        ? (Array.isArray(parsed.to) ? parsed.to.map(a => a.text).join(", ") : parsed.to.text)
        : "Unknown Recipient";
      const cc = parsed.cc
        ? (Array.isArray(parsed.cc) ? parsed.cc.map(a => a.text).join(", ") : parsed.cc.text)
        : "";
      const subject = parsed.subject || "(No Subject)";
      const date = parsed.date ? parsed.date.toLocaleString() : "Unknown Date";

      // Full body — prefer text, fallback to stripped HTML
      let body = parsed.text || "";
      if (!body && parsed.html) {
        // Strip HTML tags for plain text
        body = parsed.html
          .replaceAll(/<style[^>]*>[\s\S]*?<\/style>/gi, "")
          .replaceAll(/<script[^>]*>[\s\S]*?<\/script>/gi, "")
          .replaceAll(/<[^>]+>/g, " ")
          .replaceAll(/\s+/g, " ")
          .trim();
      }
      if (!body) body = "(No content)";

      // Truncate extremely long emails (token efficiency)
      const MAX_BODY_LENGTH = 3000;
      const truncated = body.length > MAX_BODY_LENGTH;
      if (truncated) {
        body = body.substring(0, MAX_BODY_LENGTH);
      }

      // Attachments summary
      const attachments = parsed.attachments || [];
      const attachmentInfo = attachments.length > 0
        ? attachments.map(a => `${a.filename || "unnamed"} (${(a.size / 1024).toFixed(1)} KB)`).join(", ")
        : "None";

      // Build final report with PII sanitization
      let report = `[EMAIL DETAIL — UID: ${args.uid}]\n\n`;
      report += `From: ${from}\n`;
      report += `To: ${to}\n`;
      if (cc) report += `CC: ${cc}\n`;
      report += `Date: ${date}\n`;
      report += `Subject: ${sanitize(subject)}\n`;
      report += `Attachments: ${attachmentInfo}\n`;
      report += `\n--- Full Content ---\n`;
      report += sanitize(body);
      if (truncated) {
        report += `\n\n[... Content truncated at ${MAX_BODY_LENGTH} characters for token efficiency]`;
      }

      return report.trim();
    } finally {
      lock.release();
    }
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `IMAP Error: ${errMsg}`;
  } finally {
    try { await client.logout(); } catch { /* best-effort */ }
  }
};
