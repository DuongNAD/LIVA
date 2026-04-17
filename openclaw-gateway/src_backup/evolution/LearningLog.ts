import * as lancedb from "@lancedb/lancedb";
import OpenAI from "openai";

export interface LogEntry {
    vector: number[];
    timestamp: number;
    targetFile: string;
    action: string;
    asiContext: string;
    success: boolean;
}

/**
 * Nhật Ký Học Tập (Learning Log) - Hệ Thống Trí Nhớ Tiến Hóa
 * Lưu trữ Vector các thất bại AST và RAG (Recurrence Avoidance Guided) 
 * giúp Kỹ Sư Trưởng không lặp lại lỗi cũ mà không làm nổ Context Window.
 */
export class LearningLog {
    private dbPath = "liva_learning_vectors";
    private tableName = "evolution_logs";
    private aiClient: OpenAI;

    constructor() {
        this.aiClient = new OpenAI({
            baseURL: process.env.AI_BASE_URL || "http://127.0.0.1:8000/v1", // Thường trực Router Model (Mô hình nhỏ xử lý embedding)
            apiKey: process.env.AI_API_KEY || "local-router"
        });
    }

    private async getEmbedding(text: string): Promise<number[]> {
        try {
            const response = await this.aiClient.embeddings.create({
                model: "nomic-embed-text", // Thay thế bằng ID model embedding thực tế của LIVA
                input: text,
            });
            return response.data[0].embedding;
        } catch (e: any) {
            // Fallback bảo vệ hệ thống nếu API embedding nội bộ chưa load
            console.error("[LearningLog Vector Fallback]: Hỏng Embedding API, trả về mảng rỗng.", e.message);
            return new Array(768).fill(0.01); 
        }
    }

    public async connect(): Promise<void> {
        try {
            const db = await lancedb.connect(this.dbPath);
            const tableNames = await db.tableNames();
            if (!tableNames.includes(this.tableName)) {
                const dummyVector = new Array(768).fill(0);
                await db.createTable(this.tableName, [{
                    vector: dummyVector,
                    timestamp: Date.now(),
                    targetFile: "init",
                    action: "init",
                    asiContext: "init",
                    success: true
                }]);
                console.log("🟢 [LearningLog] Đã khởi tạo cấu trúc Vector DB (LanceDB).");
            }
        } catch(e) {
            console.error("🔴 [LearningLog] Kết nối LanceDB thất bại:", e);
        }
    }

    public async recordAttempt(targetFile: string, action: string, asiContext: string, success: boolean): Promise<void> {
        try {
            const db = await lancedb.connect(this.dbPath);
            const table = await db.openTable(this.tableName);
            
            // Gom nhóm ngữ cảnh để vector hóa (Embedding Document)
            const textToEmbed = `File: ${targetFile}. Action: ${action}. Context: ${asiContext}`;
            const vector = await this.getEmbedding(textToEmbed);
            
            await table.add([{
                vector,
                timestamp: Date.now(),
                targetFile,
                action,
                asiContext,
                success
            }]);
        } catch(e) {
            console.error("[LearningLog] Lỗi lưu trữ Ký ức:", e);
        }
    }

    /**
     * Rút trích Tiên đề Tiến hóa (Semantic RAG)
     * Tránh nổ Token: Chỉ lấy Top-K (Ví dụ 3 bài học) sát sườn nhất thay vì Feed toàn bộ lịch sử.
     */
    public async getRelevantAxioms(targetFile: string, proposedAction: string, topK: number = 3): Promise<string> {
        try {
            const db = await lancedb.connect(this.dbPath);
            const table = await db.openTable(this.tableName);
            
            const queryVector = await this.getEmbedding(`File: ${targetFile}. Action: ${proposedAction}`);
            const results = await table.search(queryVector).limit(topK).toArray();
            
            let axioms = "<evolutionary_axioms>\n";
            axioms += "[TIÊN ĐỀ RÚT RA TỪ NHẬT KÝ TIẾN HÓA (Semantic Top-K)]\n";
            axioms += "Tuyệt đối không lặp lại nguyên nhân từ các thất bại dưới đây:\n";
            
            let found = false;
            for (const r of results) {
                 if (r.targetFile === "init") continue;
                 found = true;
                 const status = r.success ? "THÀNH CÔNG" : "THẤT BẠI (Bị AST Rejects)";
                 axioms += `- Hành động [${r.action}] trên tệp [${r.targetFile}] đã [${status}]:\n  Nhận xét ASI: ${r.asiContext}\n`;
            }
            axioms += "</evolutionary_axioms>";
            
            return found ? axioms : "<evolutionary_axioms>\n[Không có ký ức liên quan]\n</evolutionary_axioms>";

        } catch(e) {
             return `<evolutionary_axioms>\n// Không thể truy xuất ký ức LanceDB: ${e}\n</evolutionary_axioms>`;
        }
    }
}
