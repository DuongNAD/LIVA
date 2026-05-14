import * as nodemailer from "nodemailer";
import { logger } from "@utils/logger";
import { HITLGuard } from "@security/HITLGuard";

export const metadata = {
    name: "send_email",
    search_keywords: ["send_email", "send email", "gửi mail", "gửi thư"],
    description: "[ASK_FIRST] Send email to contacts or clients via SMTP.",
    parameters: {
        type: "object",
        properties: {
            to: { type: "string", description: "Recipient email address" },
            cc: { type: "string", description: "CC recipient email address (optional)" },
            subject: { type: "string", description: "Email subject" },
            body_text: { type: "string", description: "Email body (plain text)" }
        },
        required: ["to", "subject", "body_text"]
    }
};

export const execute = async (args: { to: string; cc?: string; subject: string; body_text: string }): Promise<string> => {
    try {
        // Yêu cầu HITL Approval trước khi gửi
        const approved = await HITLGuard.requestApproval({
            toolName: "send_email",
            args,
            reason: `Yêu cầu duyệt gửi email đến ${args.to}`
        });

        if (!approved) {
            return "Lỗi: Người dùng đã từ chối gửi email này.";
        }

        const host = process.env.EMAIL_HOST;
        const user = process.env.EMAIL_USER;
        const pass = process.env.EMAIL_PASS;

        if (!host || !user || !pass) {
            return "Lỗi cấu hình hệ thống: Thiếu EMAIL_HOST, EMAIL_USER, hoặc EMAIL_PASS.";
        }

        const transporter = nodemailer.createTransport({
            host,
            port: 465,
            secure: true,
            auth: { user, pass }
        });

        const mailOptions: any = {
            from: user,
            to: args.to,
            subject: args.subject,
            // [AUTO-TAG] Append #Liva so recipients know this is AI-generated
            text: args.body_text.includes("#Liva") ? args.body_text : `${args.body_text}\n\n#Liva`
        };
        if (args.cc) {
            mailOptions.cc = args.cc;
        }

        await transporter.sendMail(mailOptions);

        logger.info(`[SendEmail] Đã gửi email đến ${args.to}`);
        return "Email đã được gửi thành công.";

    } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
        if (errMsg === "REJECTED_BY_TIMEOUT" || errMsg === "REJECTED_BY_USER") {
            throw new Error(`HITLRejectedError: ${errMsg}`);
        }
        logger.error(`[SendEmail] Lỗi gửi mail: ${errMsg}`);
        throw e;
    }
};
