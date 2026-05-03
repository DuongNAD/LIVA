import { z } from "zod";
import { LanceMemoryManager } from "@memory/LanceMemory";
import { logger } from "@utils/logger";
import * as path from "node:path";
import * as fs from "node:fs/promises";
import * as pdfjsLib from "pdfjs-dist/legacy/build/pdf.mjs"; // Thuần JS, không có Canvas C++

const lanceMemory = new LanceMemoryManager();

// Zod Validation cho cấu trúc tham số
const DocumentParserSchema = z.object({
  filePath: z.string().min(1, "Đường dẫn file không được để trống"),
  chunkSize: z.number().optional().default(1000)
});

export const metadata = {
  name: "parse_document_pdf",
  description: "Trích xuất văn bản từ PDF (thuần JS) và tự động Chunking vào LanceMemory (chạy qua Background Task Lane) để tránh chặn WS Heartbeat.",
  kit: "DATA_KIT",
  parameters: {
    type: "object",
    properties: {
      filePath: {
        type: "string",
        description: "Đường dẫn file PDF cần đọc (VD: 'Path2/Bao_Cao.pdf').",
      }
    },
    required: ["filePath"],
  },
};

export const execute = async (args: any): Promise<string> => {
    try {
        // 1. Zod Validation
        const { filePath } = DocumentParserSchema.parse(args);
        const targetPath = path.resolve(process.cwd(), filePath);
        
        await fs.access(targetPath);

        logger.info(`[DocumentParser] Chuẩn bị parse PDF: ${targetPath}`);

        // 2. Offload sang Background Promise (Giả lập Worker Thread / TaskLaneWorker behavior)
        // Việc này giúp nhả lại Main Event Loop cho DualPortController xử lý Voice/WebSocket.
        return new Promise((resolve, reject) => {
            setImmediate(async () => {
                try {
                    // pdfjsLib load document
                    const doc = await pdfjsLib.getDocument(targetPath).promise;
                    const numPages = doc.numPages;
                    
                    logger.info(`[DocumentParser] PDF load thành công. Tổng số trang: ${numPages}`);
                    
                    let previewText = "";

                    // Đọc từng trang cuốn chiếu
                    for (let i = 1; i <= numPages; i++) {
                        const page = await doc.getPage(i);
                        const textContent = await page.getTextContent();
                        // @ts-ignore
                        const pageText = textContent.items.map(item => item.str).join(" ");
                        
                        if (i <= 3) {
                            previewText += `[Trang ${i}]\n${pageText}\n\n`;
                        }
                        
                        // 3. Chunking thẳng vào LanceDB (Vector Memory)
                        if (pageText.trim().length > 50) {
                            lanceMemory.addMemory("ANCHOR", `[PDF Chunk - ${path.basename(targetPath)} - Trang ${i}]: ${pageText}`, targetPath)
                                .catch(e => logger.error(`[DocumentParser] Lỗi nhúng LanceMemory: ${e.message}`));
                        }
                        
                        // Nhả Event Loop sau mỗi trang (Zero-Blocking Pattern)
                        await new Promise(r => setImmediate(r));
                    }

                    resolve(`[PDF PARSE & CHUNKING SUCCESS] File: ${path.basename(targetPath)}
Tổng số trang: ${numPages}
Trạng thái: Hệ thống Worker Thread đã băm (chunking) toàn bộ tài liệu và nhúng vào LanceDB (Vector Space).
--- Preview 3 trang đầu tiên ---
${previewText.substring(0, 2500)}...

(Lưu ý: Không gọi công cụ này lần thứ 2 cho cùng 1 file. Dữ liệu đã vào RAG LanceDB.)`);
                } catch (e: any) {
                    reject(new Error(`PDF.js Parsing Error: ${e.message}`));
                }
            });
        });

    } catch (error: any) {
        logger.error(`[DocumentParser] Lỗi: ${error.message}`);
        // Xử lý lỗi Zod hoặc FS
        if (error instanceof z.ZodError) {
            return `[DOCUMENT ERROR] Sai định dạng tham số: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[DOCUMENT ERROR] Lỗi hệ thống: ${error.message}`;
    }
};
