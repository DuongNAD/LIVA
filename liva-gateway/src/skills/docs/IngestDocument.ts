import { z } from "zod";
import { logger } from "@utils/logger";
import * as path from "node:path";
import * as fs from "node:fs/promises";
import { RAGIngestionPipeline } from "@services/RAGIngestionPipeline";

const IngestDocumentSchema = z.object({
  filePath: z.string().min(1, "Đường dẫn file không được để trống")
});

export const metadata = {
  name: "ingest_document",
  search_keywords: ["ingest", "đọc file", "tài liệu", "markdown", "text", "nhập tài liệu", "document", "parse"],
  description: "[AUTO_RUN] Parse, chunk, embed, and index a document file (Markdown, text, PDF) into the RAG SQLite Vector Space.",
  kit: "DATA_KIT",
  parameters: {
    type: "object",
    properties: {
      filePath: {
        type: "string",
        description: "File path (e.g. 'doc.md', 'notes.txt', 'report.pdf').",
      }
    },
    required: ["filePath"],
  },
};

export const execute = async (args: unknown): Promise<string> => {
    try {
        const { filePath } = IngestDocumentSchema.parse(args);
        const targetPath = path.resolve(process.cwd(), filePath);
        
        await fs.access(targetPath);

        logger.info(`[IngestDocument] Bắt đầu xử lý tài liệu qua RAGIngestionPipeline: ${targetPath}`);

        const pipeline = RAGIngestionPipeline.getInstance();
        const result = await pipeline.ingestFile(targetPath);

        if (!result.success) {
            throw new Error(result.error || "Unknown error during ingestion");
        }

        const ext = path.extname(targetPath).toLowerCase();
        const detailLabel = ext === ".pdf" ? `Tổng số trang: ${result.numPages}` : "Định dạng: Văn bản thuần/Markdown";

        return `[INGEST DOCUMENT SUCCESS] File: ${path.basename(targetPath)}
${detailLabel}
Đã băm thành: ${result.numChunks} chunks
Trạng thái: Tài liệu đã được chunk và nhúng thành công vào RAG sqlite-vec.
--- Preview ---
${result.previewText ? result.previewText.substring(0, 1000) : ""}...`;

    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[IngestDocument] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[DOCUMENT ERROR] Sai định dạng tham số: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[DOCUMENT ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
