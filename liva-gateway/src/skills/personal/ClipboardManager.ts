import { z } from "zod";
import { logger } from "@utils/logger";
import clipboardy from "clipboardy";

const ClipboardSchema = z.object({
  action: z.enum(["read", "write"]).describe("Hành động: đọc (read) hoặc ghi (write)"),
  content: z.string().optional().describe("Nội dung cần ghi vào clipboard (chỉ dùng khi action là write)")
});

export const metadata = {
  name: "clipboard_manager",
  search_keywords: ["clipboard", "copy", "paste", "bộ nhớ tạm", "dán", "sao chép"],
  description: "[AUTO_RUN] OS Clipboard management. Read copied content or write content to clipboard for the user to paste elsewhere.",
  kit: "PERSONAL_KIT", // Dynamic Gating: Chỉ bật khi có context liên quan đến cá nhân/OS
  parameters: {
    type: "object",
    properties: {
      action: { type: "string", enum: ["read", "write"] },
      content: { type: "string" }
    },
    required: ["action"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = ClipboardSchema.parse(argsObj);

        if (parsed.action === "read") {
            const data = await clipboardy.read();
            logger.info(`[ClipboardManager] Đã đọc thành công nội dung từ Clipboard (${data.length} ký tự).`);
            
            if (!data || data.trim() === "") {
                return "[CLIPBOARD EMPTY] Bộ nhớ đệm hiện đang trống.";
            }
            
            return `[CLIPBOARD DATA]\n${data}`;
        } 
        
        if (parsed.action === "write") {
            if (!parsed.content) {
                throw new Error("Cần cung cấp 'content' để ghi vào clipboard.");
            }
            await clipboardy.write(parsed.content);
            logger.info(`[ClipboardManager] Đã ghi thành công vào Clipboard.`);
            return `[CLIPBOARD WRITE SUCCESS] Đã sao chép nội dung vào bộ nhớ đệm (người dùng có thể Ctrl+V ngay bây giờ).`;
        }

        return "Hành động không hợp lệ.";
    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[ClipboardManager] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[CLIPBOARD ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[CLIPBOARD ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
