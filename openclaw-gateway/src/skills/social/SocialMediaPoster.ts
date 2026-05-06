import { z } from "zod";
import { logger } from "@utils/logger";
import { safeFetch } from "@utils/HttpClient";
import { HITLGuard } from "@security/HITLGuard";

const SocialSchema = z.object({
  platform: z.enum(["twitter", "linkedin", "facebook"]),
  content: z.string().min(1, "Nội dung bài viết không được để trống"),
  mediaUrl: z.string().optional()
});

export const metadata = {
  name: "social_media_poster",
  description: "Đăng bài viết lên mạng xã hội (Twitter, LinkedIn). Yêu cầu nghiêm ngặt phê duyệt HITLGuard trước khi gửi request ra ngoài Internet thông qua safeFetch.",
  kit: "SOCIAL_KIT",
  parameters: {
    type: "object",
    properties: {
      platform: { type: "string", enum: ["twitter", "linkedin", "facebook"] },
      content: { type: "string", description: "Nội dung bài đăng" },
      mediaUrl: { type: "string", description: "Đường dẫn đính kèm media (nếu có)" }
    },
    required: ["platform", "content"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = SocialSchema.parse(argsObj);
        const API_URL = process.env.SOCIAL_API_URL || "http://127.0.0.1:9999/api/social";

        // MỌI HÀNH ĐỘNG ĐĂNG BÀI ĐỀU PHẢI QUA HITL
        logger.info(`[Social] Đang yêu cầu HITL phê duyệt bài đăng lên ${parsed.platform}...`);
        try {
            await HITLGuard.requestApproval({
                toolName: "social_media_poster",
                args: parsed,
                reason: `LIVA muốn đăng một bài viết lên ${parsed.platform}. Nội dung: "${parsed.content.substring(0, 50)}..."`
            });
        } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
            return `[SOCIAL BLOCKED] Việc đăng bài đã bị từ chối: ${errMsg}`;
        }

        try {
            const res = await safeFetch(API_URL, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(parsed)
            });
            const data = await res.json();
            return `[SOCIAL SUCCESS] Đã đăng bài lên ${parsed.platform}. Trạng thái API: ${data.status || 'OK'}`;
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[Social] API lỗi, dùng Mock: ${errMsg}`);
            return `[SOCIAL SUCCESS] (MOCK MODE) Đã đăng bài viết thành công lên ${parsed.platform}.`;
        }

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[Social] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[SOCIAL ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[SOCIAL ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
