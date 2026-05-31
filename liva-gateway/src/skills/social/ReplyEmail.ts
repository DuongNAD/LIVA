import * as nodemailer from "nodemailer";
import { z } from "zod";
import { logger } from "@utils/logger";
import { HITLGuard } from "@security/HITLGuard";
import { simpleParser } from "mailparser";
import { getEmailCredentials, createImapClient, sanitizeEmailContent } from "@utils/EmailHelper";
import { EmailTemplateBuilder } from "@utils/EmailTemplateBuilder";

const ReplyEmailSchema = z.object({
  originalUid: z.number(),
  body_text: z.string().min(1, "body_text is required"),
  replyAll: z.boolean().optional().default(false),
  bypassHITL: z.boolean().optional().default(false),
});

export const metadata = {
  name: "reply_email",
  search_keywords: ["reply_email", "reply email", "rep mail", "rep email", "trả lời mail", "trả lời thư", "reply"],
  description: "[ASK_FIRST] Reply to a specific email by its UID. Automatically sets threading headers (In-Reply-To, References) to reply in the same email conversation.",
  kit: "SOCIAL_KIT",
  requires_hitl: true,
  parameters: {
    type: "object",
    properties: {
      originalUid: { type: "number", description: "The UID of the original email to reply to." },
      body_text: { type: "string", description: "The reply message content (plain text)." },
      replyAll: { type: "boolean", description: "Whether to reply to all recipients (including CC) or just the sender. Default is false." }
    },
    required: ["originalUid", "body_text"]
  }
};

export const execute = async (rawArgs: unknown): Promise<string> => {
  const parsedArgs = ReplyEmailSchema.safeParse(rawArgs);
  if (!parsedArgs.success) {
    throw new Error(`[ValidationError] Invalid input: ${parsedArgs.error.issues.map(i => i.message).join("; ")}`);
  }
  const args = parsedArgs.data;

  const credentials = getEmailCredentials();
  if (!credentials) {
    return "Error: Thiếu cấu hình EMAIL_HOST, EMAIL_USER, hoặc EMAIL_PASS trong hòm bí mật (vault) hoặc .env.";
  }

  logger.info(`[reply_email] Bắt đầu tìm kiếm email UID: ${args.originalUid} để phản hồi...`);
  const imapClient = createImapClient(credentials);

  let originalMailInfo: {
    messageId?: string;
    references?: string;
    subject: string;
    fromAddress: string;
    fromText: string;
    dateText: string;
    bodyText: string;
    bodyHtml?: string;
    toAddresses: string[];
    ccAddresses: string[];
  } | null = null;

  try {
    await imapClient.connect();
    const lock = await imapClient.getMailboxLock("INBOX");
    try {
      const msgData = await imapClient.fetchOne(args.originalUid.toString(), { source: true }, { uid: true });
      if (!msgData || !('source' in msgData) || !msgData.source) {
        throw new Error(`Không tìm thấy email gốc với UID ${args.originalUid}`);
      }

      const parsed = await simpleParser(msgData.source as Buffer);
      
      const fromAddrObj = parsed.from?.value?.[0];
      const fromAddress = fromAddrObj?.address || credentials.user; // default to self if not found
      const fromText = parsed.from?.text || fromAddress;
      
      const toAddresses = parsed.to 
        ? (Array.isArray(parsed.to) ? parsed.to : [parsed.to])
            .flatMap(t => t.value || [])
            .map(v => v.address)
            .filter((addr): addr is string => typeof addr === "string")
        : [];
      
      const ccAddresses = parsed.cc
        ? (Array.isArray(parsed.cc) ? parsed.cc : [parsed.cc])
            .flatMap(t => t.value || [])
            .map(v => v.address)
            .filter((addr): addr is string => typeof addr === "string")
        : [];

      // Extract references header or in-reply-to
      const referencesHeader = parsed.headers.get("references");
      const references = Array.isArray(referencesHeader)
        ? referencesHeader.join(" ")
        : (typeof referencesHeader === "string" ? referencesHeader : "");

      originalMailInfo = {
        messageId: parsed.messageId,
        references: references || parsed.inReplyTo,
        subject: parsed.subject || "",
        fromAddress,
        fromText,
        dateText: parsed.date ? parsed.date.toUTCString() : new Date().toUTCString(),
        bodyText: parsed.text || "",
        bodyHtml: typeof parsed.html === "string" ? parsed.html : "",
        toAddresses,
        ccAddresses
      };
    } finally {
      lock.release();
    }
  } catch (err: any) {
    logger.error(`[reply_email] Lỗi đọc email gốc: ${err.message}`);
    throw err;
  } finally {
    try { await imapClient.logout(); } catch {}
  }

  if (!originalMailInfo) {
    return "Lỗi: Không tìm thấy email gốc để phản hồi.";
  }

  // Determine recipients
  // 'To' is the sender of the original email. If the original email was sent by the user, reply to the first 'To' address.
  let toRecipient = originalMailInfo.fromAddress;
  if (toRecipient.toLowerCase() === credentials.user.toLowerCase() && originalMailInfo.toAddresses.length > 0) {
    toRecipient = originalMailInfo.toAddresses[0];
  }

  const mergedCc: string[] = [];
  if (args.replyAll) {
    // CC is original CC + original To (excluding our user and target To)
    const exclude = new Set([credentials.user.toLowerCase(), toRecipient.toLowerCase()]);
    for (const addr of [...originalMailInfo.toAddresses, ...originalMailInfo.ccAddresses]) {
      if (addr && !exclude.has(addr.toLowerCase())) {
        mergedCc.push(addr);
      }
    }
  }

  // Prepend "Re: " to subject if not present
  let newSubject = originalMailInfo.subject;
  if (!/^re:/i.test(newSubject)) {
    newSubject = `Re: ${newSubject}`;
  }

  // Format quoted body
  const quoted = originalMailInfo.bodyText
    .split("\n")
    .map(line => `> ${line}`)
    .join("\n");

  const fullReplyBody = `${args.body_text}\n\nOn ${originalMailInfo.dateText}, ${originalMailInfo.fromText} wrote:\n${quoted}`;
  const finalBody = fullReplyBody.includes("#Liva") ? fullReplyBody : `${fullReplyBody}\n\n#Liva`;

  // Construct HTML body using EmailTemplateBuilder
  const originalContent = originalMailInfo.bodyHtml || originalMailInfo.bodyText;
  const isHtmlContent = !!originalMailInfo.bodyHtml;
  const replyHtml = EmailTemplateBuilder.buildReplyHtml(
    args.body_text,
    originalMailInfo.fromText,
    originalMailInfo.dateText,
    originalContent,
    isHtmlContent
  );
  const finalHtml = replyHtml.includes("#Liva") ? replyHtml : replyHtml.replace("</body>", "<p>• #Liva</p></body>");

  try {
    if (!args.bypassHITL) {
      // HITL approval
      const approved = await HITLGuard.requestApproval({
        toolName: "reply_email",
        args: {
          originalUid: args.originalUid,
          to: toRecipient,
          cc: mergedCc.join(", "),
          subject: newSubject,
          body: args.body_text
        },
        reason: `Trả lời email gửi đến ${toRecipient} (UID: ${args.originalUid})`
      });

      if (!approved) {
        return "Lỗi: Người dùng đã từ chối gửi email trả lời này.";
      }
    }

    // SMTP Transporter
    const transporter = nodemailer.createTransport({
      host: credentials.host,
      port: 465,
      secure: true,
      auth: { user: credentials.user, pass: credentials.pass }
    });

    const mailOptions: any = {
      from: credentials.user,
      to: toRecipient,
      subject: newSubject,
      text: finalBody,
      html: finalHtml
    };

    if (mergedCc.length > 0) {
      mailOptions.cc = mergedCc;
    }

    // Set threading headers
    const headers: Record<string, string> = {};
    if (originalMailInfo.messageId) {
      headers["In-Reply-To"] = originalMailInfo.messageId.startsWith("<") && originalMailInfo.messageId.endsWith(">")
        ? originalMailInfo.messageId
        : `<${originalMailInfo.messageId}>`;
      
      const newReferences = originalMailInfo.references
        ? `${originalMailInfo.references} ${headers["In-Reply-To"]}`
        : headers["In-Reply-To"];
      
      headers["References"] = newReferences;
    }

    mailOptions.headers = headers;

    await transporter.sendMail(mailOptions);
    logger.info(`[reply_email] Đã gửi email phản hồi thành công đến ${toRecipient}`);
    return `Đã gửi email trả lời thành công đến ${toRecipient} dưới tiêu đề "${newSubject}".`;

  } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
    if (errMsg === "REJECTED_BY_TIMEOUT" || errMsg === "REJECTED_BY_USER") {
      throw new Error(`HITLRejectedError: ${errMsg}`);
    }
    logger.error(`[reply_email] Lỗi gửi email: ${errMsg}`);
    throw e;
  }
};
