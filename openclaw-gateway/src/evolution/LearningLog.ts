import * as lancedb from "@lancedb/lancedb";
import { pipeline, FeatureExtractionPipeline } from "@xenova/transformers";

export interface LogEntry {
    id: string;
    vector: number[];
    timestamp: number;
    targetFile: string;
    action: string;
    asiContext: string;
    success: boolean;
    occurrence_count: number;
}

/**
 * Nhật Ký Học Tập (Learning Log) - Hệ Thống Trí Nhớ Tiến Hóa
 * Lưu trữ Vector các thất bại/thành công, tránh lặp lại lỗi cũ và tái sử dụng pattern chuẩn.
 */
export class LearningLog {
    private dbPath = "liva_learning_vectors";
    private tableName = "evolution_logs_v2"; // Đổi table name v2 để nạp schema mới có ID & occurrence_count
    private embedder: FeatureExtractionPipeline | null = null;

    constructor() { }

    private async initEmbedder() {
        if (!this.embedder) {
            try {
                this.embedder = await pipeline("feature-extraction", "Xenova/all-MiniLM-L6-v2");
            } catch (e) {
                console.error("[LearningLog] Lỗi tải Xenova Pipeline:", e);
            }
        }
    }

    private async getEmbedding(text: string): Promise<number[]> {
        await this.initEmbedder();
        if (this.embedder) {
            try {
                const output = await this.embedder(text, {
                    pooling: "mean",
                    normalize: true,
                });
                return Array.from(output.data);
            } catch (e: any) {
                console.error("[LearningLog Vector Fallback]: Hỏng Embedding API nội bộ, trả về mảng rỗng.", e.message);
            }
        }
        return new Array(384).fill(0.01); 
    }

    public async connect(): Promise<void> {
        try {
            const db = await lancedb.connect(this.dbPath);
            const tableNames = await db.tableNames();
            if (!tableNames.includes(this.tableName)) {
                // Đổi từ 768 sang 384 để đồng bộ với MiniLM-L6-v2
                const dummyVector = new Array(384).fill(0.01);
                await db.createTable(this.tableName, [{
                    id: "init_id",
                    vector: dummyVector,
                    timestamp: Date.now(),
                    targetFile: "init",
                    action: "init",
                    asiContext: "init",
                    success: true,
                    occurrence_count: 1
                }]);
                console.log("🟢 [LearningLog] Đã khởi tạo cấu trúc Vector DB (LanceDB V2) - 384D.");
            }
        } catch(e) {
            console.error("🔴 [LearningLog] Kết nối LanceDB thất bại:", e);
        }
    }

    // Tiền xử lý (Chưng cất) log giảm nhiễu Vector (Semantic Distillation)
    private distillContext(rawContext: string): string {
        if (!rawContext) return "Lỗi không xác định";
        // Loại bỏ rác đường dẫn tuyệt đối dài
        let distilled = rawContext.replace(/[A-Za-z]:\\[^\s]+\\src\\/gi, "src/");
        distilled = distilled.replace(/\/home\/[^\s]+\/src\//gi, "src/");
        // Cắt ngắn nếu dính log MicroVM quá to
        if (distilled.length > 2000) {
            distilled = distilled.substring(0, 2000) + "... (distilled)";
        }
        return distilled.trim();
    }

    public async recordAttempt(targetFile: string, action: string, asiContext: string, success: boolean): Promise<void> {
        try {
            const distilledContext = this.distillContext(asiContext);
            const db = await lancedb.connect(this.dbPath);
            const tableNames = await db.tableNames();
            if (!tableNames.includes(this.tableName)) {
                await this.connect();
            }
            const table = await db.openTable(this.tableName);
            
            // Gom nhóm ngữ cảnh để vector hóa
            const textToEmbed = `File: ${targetFile}. Action: ${action}. Context: ${distilledContext}`;
            const vector = await this.getEmbedding(textToEmbed);
            
            // Vector Deduplication (Tránh Context Bloat)
            const results = await table.search(vector)
                .limit(1)
                .toArray();

            if (results.length > 0 && typeof results[0]._distance === "number" && results[0]._distance < 0.05) {
                // Đã từng gặp lỗi này -> Update tăng bộ đếm occurrence_count
                const existingId = results[0].id;
                const newCount = (results[0].occurrence_count || 1) + 1;
                
                try {
                    // Fallback cho LanceDB Node nếu ko hỗ trợ table.update -> Delete & Add
                    await table.delete(`id = '${existingId}'`);
                    await table.add([{
                        id: existingId,
                        vector,
                        timestamp: Date.now(),
                        targetFile,
                        action,
                        asiContext: distilledContext, // Cập nhật lại context mới nhất
                        success,
                        occurrence_count: newCount
                    }]);
                    // console.log(`[LearningLog] Khử trùng lặp: Đã cập nhật tần suất lỗi (Count: ${newCount})`);
                } catch(updateErr) {
                    console.error("[LearningLog] Lỗi cập nhật bản ghi cũ:", updateErr);
                }
            } else {
                // Bản ghi mới hoàn toàn
                const newId = `log_${Date.now()}_${Math.floor(Math.random() * 9999)}`;
                await table.add([{
                    id: newId,
                    vector,
                    timestamp: Date.now(),
                    targetFile,
                    action,
                    asiContext: distilledContext,
                    success,
                    occurrence_count: 1
                }]);
            }
        } catch(e) {
            console.error("[LearningLog] Lỗi lưu trữ Ký ức:", e);
        }
    }

    /**
     * Rút trích Tiên đề Tiến hóa (XML RAG)
     * Phân tách thành công và thất bại giúp loại bỏ Reward Inversion.
     */
    public async getRelevantAxioms(targetFile: string, proposedAction: string, topK: number = 3): Promise<string> {
        try {
            const db = await lancedb.connect(this.dbPath);
            const tableNames = await db.tableNames();
            if (!tableNames.includes(this.tableName)) return "<system_memory>\n  [Không có ký ức liên quan]\n</system_memory>";
            const table = await db.openTable(this.tableName);
            
            const queryVector = await this.getEmbedding(`File: ${targetFile}. Action: ${proposedAction}`);
            const results = await table.search(queryVector).limit(topK * 2).toArray(); // Lấy dư ra để lọc Decay
            
            let bestPractices = "";
            let antiPatterns = "";
            
            // Memory Decay: Đào thải ký ức cổ đại (> 30 ngày)
            const MAXIMUM_MEMORY_AGE_MS = 30 * 24 * 60 * 60 * 1000;
            const currentTime = Date.now();

            let found = false;
            let counter = 0;

            for (const r of results) {
                 if (counter >= topK) break; 
                 if (r.id === "init_id") continue;
                 
                 // Đào thải bóng ma quá khứ
                 if (currentTime - r.timestamp > MAXIMUM_MEMORY_AGE_MS) continue;

                 found = true;
                 counter++;

                 const freqCount = r.occurrence_count || 1;
                 const freqStr = freqCount > 1 ? `${freqCount}_times` : `1_time`;
                 const severity = freqCount > 5 ? "CRITICAL" : (freqCount > 2 ? "HIGH" : "MEDIUM");

                 if (r.success) {
                     bestPractices += `        <axiom frequency="${freqStr}">Hành động [${r.action}] trên tệp [${r.targetFile}] đem lại kết quả TỐT: ${r.asiContext}</axiom>\n`;
                 } else {
                     antiPatterns += `        <warning frequency="${freqStr}" severity="${severity}">\n            [LỊCH SỬ THẤT BẠI]: Quá trình [${r.action}] trên [${r.targetFile}].\n            [NHẬN XÉT BUG]: ${r.asiContext}\n        </warning>\n`;
                 }
            }
            
            if (!found) {
                return "<system_memory>\n  [Không có ký ức liên quan trực tiếp]\n</system_memory>";
            }

            let axioms = "<system_memory>\n    LIVA System đã rút ra các tiên đề từ lịch sử. BẠN BẮT BUỘC TUÂN THỦ:\n\n";
            if (bestPractices.length > 0) {
                axioms += `    <best_practices>\n${bestPractices}    </best_practices>\n\n`;
            }
            if (antiPatterns.length > 0) {
                axioms += `    <anti_patterns>\n        Tuân thủ nghiêm ngặt để không giẫm lốp xe đổ (Frequency càng cao càng nguy hiểm):\n${antiPatterns}    </anti_patterns>\n`;
            }
            axioms += "</system_memory>";
            
            return axioms;

        } catch(e) {
             return `<system_memory>\n    <!-- Lỗi truy xuất ký ức LanceDB: ${e} -->\n</system_memory>`;
        }
    }
}
