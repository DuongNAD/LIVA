import { z } from "zod";
import { logger } from "@utils/logger";
import { DatabaseSync } from "node:sqlite";
import * as path from "node:path";
import * as fs from "node:fs/promises";

const DBSchema = z.object({
  dbPath: z.string().describe("Đường dẫn đến file Database SQLite (VD: data/sales.db)"),
  query: z.string().describe("Câu lệnh SQL cần thực thi (SELECT, INSERT, UPDATE, v.v.)")
});

export const metadata = {
  name: "db_operator",
  description: "[AUTO_RUN] Interact directly with local SQLite database. Run SQL commands for reporting and data analysis without external DB Client software.",
  kit: "DATA_KIT",
  parameters: {
    type: "object",
    properties: {
      dbPath: { type: "string" },
      query: { type: "string" }
    },
    required: ["dbPath", "query"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = DBSchema.parse(argsObj);
        const absolutePath = path.resolve(process.cwd(), parsed.dbPath);

        try {
            await fs.access(absolutePath);
        } catch {
            return `[DB ERROR] Không tìm thấy file Database tại đường dẫn: ${absolutePath}`;
        }

        logger.info(`[DBOperator] Đang thực thi câu lệnh SQL trên DB: ${parsed.dbPath}`);
        
        // Mở kết nối Database
        const db = new DatabaseSync(absolutePath);

        try {
            const isSelect = parsed.query.trim().toUpperCase().startsWith("SELECT") || parsed.query.trim().toUpperCase().startsWith("PRAGMA");
            
            if (isSelect) {
                const stmt = db.prepare(parsed.query);
                const rows = stmt.all();
                
                // Giới hạn kết quả để không tràn context window
                if (rows.length === 0) {
                    return `[DB RESULT] Câu truy vấn thực thi thành công nhưng không có dữ liệu trả về (0 rows).`;
                }

                const MAX_ROWS = 50;
                const displayRows = rows.slice(0, MAX_ROWS);
                
                let resultText = `[DB RESULT] Trả về ${rows.length} dòng dữ liệu.\n`;
                if (rows.length > MAX_ROWS) {
                    resultText += `(Chỉ hiển thị ${MAX_ROWS} dòng đầu tiên để tránh quá tải bộ nhớ LIVA)\n\n`;
                }

                resultText += JSON.stringify(displayRows, null, 2);
                return resultText;
                
            } else {
                const stmt = db.prepare(parsed.query);
                const result = stmt.run();
                return `[DB SUCCESS] Đã thực thi lệnh SQL thay đổi dữ liệu.\nSố dòng bị ảnh hưởng (Changes): ${result.changes}`;
            }

        } finally {
            db.close();
        }

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[DBOperator] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[DB ERROR] Sai định dạng: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[DB ERROR] Cú pháp SQL hoặc lỗi hệ thống: ${errMsg}`;
    }
};
