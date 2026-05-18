import { z } from "zod";
import { StructuredMemory } from "@memory/StructuredMemory";
import { EmbeddingService } from "@services/EmbeddingService";
import { logger } from "@utils/logger";
import * as path from "node:path";
import * as fs from "node:fs/promises";
import { Worker } from "node:worker_threads";

// [v19] Will be injected via DI in future; for now, lazy singleton pattern
let _structuredMemory: StructuredMemory | null = null;
const getStructuredMemory = async (): Promise<StructuredMemory> => {
    if (!_structuredMemory) {
        _structuredMemory = await StructuredMemory.create("liva_core");
    }
    return _structuredMemory;
};

// Zod Validation cho cấu trúc tham số
const DocumentParserSchema = z.object({
  filePath: z.string().min(1, "Đường dẫn file không được để trống"),
  chunkSize: z.number().optional().default(1000)
});

export const metadata = {
  name: "parse_document_pdf",
  description: "[AUTO_RUN] Extract text from PDF (pure JS) via native Worker Thread and auto-chunk into StructuredMemory. Avoids blocking WS Heartbeat.",
  kit: "DATA_KIT",
  parameters: {
    type: "object",
    properties: {
      filePath: {
        type: "string",
        description: "PDF file path (e.g., 'Path2/Report.pdf').",
      }
    },
    required: ["filePath"],
  },
};

export const execute = async (args: unknown): Promise<string> => {
    try {
        // 1. Zod Validation
        const { filePath } = DocumentParserSchema.parse(args);
        const targetPath = path.resolve(process.cwd(), filePath);
        
        await fs.access(targetPath);

        logger.info(`[DocumentParser] Chuẩn bị parse PDF qua Worker Thread: ${targetPath}`);

        const workerCode = `
            import { parentPort, workerData } from 'node:worker_threads';
            
            async function run() {
                try {
                    // Load module dynamically inside worker
                    const pdfjsLib = await import("pdfjs-dist/legacy/build/pdf.mjs");
                    const doc = await pdfjsLib.getDocument(workerData.targetPath).promise;
                    const numPages = doc.numPages;
                    
                    let previewText = "";

                    for (let i = 1; i <= numPages; i++) {
                        const page = await doc.getPage(i);
                        const textContent = await page.getTextContent();
                        const pageText = textContent.items.map(item => item.str).join(" ");
                        
                        if (i <= 3) {
                            previewText += \`[Trang \${i}]\\n\${pageText}\\n\\n\`;
                        }
                        
                        if (pageText.trim().length > 50) {
                            parentPort.postMessage({ type: 'chunk', i, pageText });
                        }
                    }

                    parentPort.postMessage({ type: 'done', numPages, previewText });
                } catch (err) {
                    parentPort.postMessage({ type: 'error', error: err.message || String(err) });
                }
            }
            run();
        `;

        return new Promise((resolve, reject) => {
            let isDone = false;
            const worker = new Worker(workerCode, {
                eval: true,
                workerData: { targetPath }
            });

            let watchdog: NodeJS.Timeout;
            const resetWatchdog = () => {
                if (watchdog) clearTimeout(watchdog);
                watchdog = setTimeout(() => {
                    if (!isDone) {
                        isDone = true;
                        logger.error(`[Watchdog] DocumentParser worker deadlocked. Terminating...`);
                        worker.terminate();
                        reject(new Error(`Error: PDF parsing timed out after 45 seconds of inactivity.`));
                    }
                }, 45000);
            };

            const cleanup = () => {
                isDone = true;
                if (watchdog) clearTimeout(watchdog);
            };

            resetWatchdog();

            let numPages = 0;
            let previewText = "";
            let chunkCount = 0;

            worker.on('message', async (msg) => {
                resetWatchdog();
                if (msg.type === 'error') {
                    cleanup();
                    reject(new Error(`PDF Parsing Error: ${msg.error}`));
                } else if (msg.type === 'chunk') {
                    try {
                        const sm = await getStructuredMemory();
                        const embedding = EmbeddingService.getInstance();
                        const vec = await embedding.embed(msg.pageText.substring(0, 500));
                        sm.upsertVector({
                            vecId: `pdf_${path.basename(targetPath)}_p${msg.i}`,
                            type: 'ANCHOR',
                            content: `[PDF Chunk - ${path.basename(targetPath)} - Trang ${msg.i}]: ${msg.pageText}`,
                            vector: vec,
                            fileTarget: targetPath,
                        });
                        chunkCount++;
                    } catch (e) {
                        logger.error(`[DocumentParser] Lỗi nhúng Vector trang ${msg.i}: ${e}`);
                    }
                } else if (msg.type === 'done') {
                    cleanup();
                    numPages = msg.numPages;
                    previewText = msg.previewText;
                    
                    resolve(`[PDF PARSE & CHUNKING SUCCESS] File: ${path.basename(targetPath)}
Tổng số trang: ${numPages} (Đã nhúng ${chunkCount} chunks)
Trạng thái: Hệ thống Worker Thread đã băm (chunking) toàn bộ tài liệu và nhúng vào sqlite-vec (Vector Space).
--- Preview 3 trang đầu tiên ---
${previewText.substring(0, 2500)}...

(Lưu ý: Không gọi công cụ này lần thứ 2 cho cùng 1 file. Dữ liệu đã vào RAG sqlite-vec.)`);
                }
            });

            worker.on('error', (err: any) => {
                cleanup();
                logger.error(`[DocumentParser] Worker Lỗi: ${err.message}`);
                reject(new Error(`Worker error: ${err.message}`));
            });

            worker.on('exit', (code) => {
                cleanup();
                if (code !== 0) {
                    reject(new Error(`Worker stopped with exit code ${code}`));
                }
            });
        });

    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[DocumentParser] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[DOCUMENT ERROR] Sai định dạng tham số: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[DOCUMENT ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
