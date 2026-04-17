import { Project, SourceFile } from "ts-morph";
import * as fs from "fs";
import * as path from "path";

/**
 * Lớp Tự Chữa Lành và Đánh Giá AST (Evaluator Healer on Host)
 * Giúp Evaluator lấy thông tin lỗi tinh sạch và fix tự động các thư viện Import.
 */
export class ASTHealer {
    private project: Project;

    constructor() {
         this.project = new Project({
            skipAddingFilesFromTsConfig: true, // Trọng điểm chống OOM
            compilerOptions: { allowJs: true, strict: false }
        });
    }

    /**
     * Healer Tự Động Định Tuyến Import (trước khi gửi cho Compiler)
     */
    public async autoHealImports(filePath: string): Promise<{ success: boolean; logs: string }> {
        try {
            if (!fs.existsSync(filePath)) return { success: false, logs: "File bị thất lạc." };
            
            const sourceFile = this.project.addSourceFileAtPath(filePath);
            
            // Chuỗi tự chữa lành của ts-morph
            sourceFile.fixMissingImports();
            sourceFile.organizeImports();
            sourceFile.fixUnusedIdentifiers();
            sourceFile.formatText(); // Chuẩn hóa lề tự động
            
            await sourceFile.save();
            this.project.removeSourceFile(sourceFile); // Flush RAM
            
            return { success: true, logs: "✅ Đã tự động vá Imports và làm sạch Code thành công." };
        } catch(e: any) {
            return { success: false, logs: `⚠️ Cảnh báo Healer: Không thể tự vá import. Lỗi: ${e.message}`};
        }
    }

    /**
     * Dịch lỗi Compiler thành ASI (Actionable Side Information) cho DarwinianEvolver
     */
    public getASIFromPreEmitDiagnostics(filePath: string): string {
         try {
            if (!fs.existsSync(filePath)) return "[ASI] Error: Tệp không tồn tại để phân tích.";
            
            const sourceFile = this.project.addSourceFileAtPath(filePath);
            const diagnostics = sourceFile.getPreEmitDiagnostics();
            
            if (diagnostics.length === 0) {
                 this.project.removeSourceFile(sourceFile);
                 return ""; // Không có lỗi
            }

            let asiReport = "<actionable_side_information>\n[CẢNH BÁO TỪ TRÌNH BIÊN DỊCH AST]\nCá thể đột biến này bị vỡ quy tắc Typing/Syntax:\n";
            
            // Giới hạn Token: chỉ vắt 5 lỗi nghiêm trọng nhất
            for (const d of diagnostics.slice(0, 5)) { 
                 const message = d.getMessageText();
                 const msgStr = typeof message === "string" ? message : message.getMessageText();
                 const line = d.getLineNumber() || "Unknown";
                 asiReport += `- Dòng [${line}]: ${msgStr}\n`;
            }
            
            asiReport += "\nHướng dẫn ASI: Hãy thay đổi cách gọi hàm hoặc kiểm tra lại tên thuộc tính (ts2339) dựa trên lỗi cung cấp ở trên.\n</actionable_side_information>";
            
            this.project.removeSourceFile(sourceFile);
            return asiReport;
         } catch(e: any) {
            return `[ASI Engine Fatal Error] Không thể trích xuất Diagnostics: ${e.message}`;
         }
    }
}
