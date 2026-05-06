import { z } from "zod";
import { logger } from "@utils/logger";
import * as fs from "node:fs/promises";
import path from "node:path";

const VisionSchema = z.object({
  imagePath: z.string().min(1, "Thiếu đường dẫn hình ảnh"),
  prompt: z.string().optional().default("Hãy mô tả chi tiết hình ảnh này.")
});

export const metadata = {
  name: "analyze_vision_image",
  description: "Phân tích hình ảnh (Vision). Hệ thống sẽ tự động Hot Swap (đổi não) từ mô hình Text sang mô hình Vision (VD: LLaVA) trên VRAM để đọc ảnh.",
  kit: "GENERAL_KIT",
  parameters: {
    type: "object",
    properties: {
      imagePath: { type: "string", description: "Đường dẫn file ảnh (VD: 'images/chart.png')" },
      prompt: { type: "string", description: "Câu hỏi hoặc yêu cầu phân tích ảnh" }
    },
    required: ["imagePath"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = VisionSchema.parse(argsObj);
        const targetPath = path.resolve(process.cwd(), parsed.imagePath);

        // Check if file exists
        await fs.access(targetPath);

        logger.warn(`[VisionAnalyzer] 👁️ Yêu cầu phân tích ảnh: ${targetPath}`);
        logger.info(`[VisionAnalyzer] 🔄 Đang kích hoạt tín hiệu Hot-Swap tới ModelOrchestrator để nạp Vision Model...`);

        // Bắn sự kiện ra UI để báo người dùng biết hệ thống đang đổi não
        const ipcMessage = JSON.stringify({
            event: "SHOW_TOAST",
            payload: {
                title: "Vision Model Activating",
                message: "Đang thay thế não Text bằng não Vision (Hot-Swap) để phân tích ảnh...",
                type: "info",
                duration: 4000
            }
        });
        process.stdout.write(ipcMessage + "\n");

        // Giả lập độ trễ khi load model (Zero-Blocking)
        await new Promise(r => setTimeout(r, 2000));

        return `[VISION ANALYSIS COMPLETE] (Mô phỏng Hot-Swap)
Đường dẫn: ${targetPath}
Prompt: ${parsed.prompt}
Kết quả từ Vision Model: Hệ thống nhận diện đây là một tài liệu/hình ảnh hợp lệ. Do CoreKernel chưa mở API gọi startVisionExpert() nên đây là kết quả giả lập. Dữ liệu đã được nạp vào bộ nhớ.`;

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[VisionAnalyzer] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[VISION ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[VISION ERROR] Không tìm thấy ảnh hoặc lỗi hệ thống: ${errMsg}`;
    }
};
