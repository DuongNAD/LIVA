import * as path from "node:path";
import { Project } from "ts-morph";
/**
 * Lớp Tự Chữa Lành và Đánh Giá AST (Evaluator Healer on Host)
 * Giúp Evaluator lấy thông tin lỗi tinh sạch và fix tự động các thư viện Import.
 */
export class ASTHealer {
    private readonly project: Project;

    constructor() {
         this.project = new Project({
            skipAddingFilesFromTsConfig: true, // Trọng điểm chống OOM
            compilerOptions: { allowJs: true, strict: false }
        });
    }

    /**
     * Healer Tự Động Định Tuyến Import (trên toàn bộ file TS trong Sandbox)
     */
    public async autoHealImportsOnSandbox(sandboxRoot: string): Promise<{ success: boolean; logs: string }> {
        try {
            const project = new Project({
                tsConfigFilePath: path.join(sandboxRoot, "tsconfig.json"),
                skipAddingFilesFromTsConfig: false,
                compilerOptions: { allowJs: true }
            });
            
            const sourceFiles = project.getSourceFiles();
            for (const sourceFile of sourceFiles) {
                if (sourceFile.getFilePath().includes("node_modules")) continue;
                sourceFile.fixMissingImports();
                sourceFile.organizeImports();
                sourceFile.fixUnusedIdentifiers();
                sourceFile.formatText(); 
            }
            
            await project.save();
            return { success: true, logs: "✅ Đã tự động vá Imports và làm sạch Code toàn Sandbox thành công." };
        } catch(e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            return { success: false, logs: `⚠️ Cảnh báo Healer: Không thể tự vá import. Lỗi: ${errMsg}`};
        }
    }

    /**
     * Dịch lỗi Compiler thành ASI (Actionable Side Information) cho DarwinianEvolver
     */
    public getASIFromPreEmitDiagnosticsOnSandbox(sandboxRoot: string): string {
         try {
            const project = new Project({
                tsConfigFilePath: path.join(sandboxRoot, "tsconfig.json"),
                skipAddingFilesFromTsConfig: false,
                compilerOptions: { allowJs: true, noEmit: true }
            });
            
            const diagnostics = project.getPreEmitDiagnostics();
            
            if (diagnostics.length === 0) {
                 return ""; // Không có lỗi trong toàn Sandbox
            }

            let asiReport = "<actionable_side_information>\\n[CẢNH BÁO TỪ TRÌNH BIÊN DỊCH AST MULTI-FILE]\\nKiến trúc đột biến bị vỡ quy tắc Typing/Syntax:\\n";
            
            // Lọc bỏ lỗi từ node_modules nếu lỡ quét dính
            const relevantDiagnostics = diagnostics.filter(d => {
                const f = d.getSourceFile();
                return f ? !f.getFilePath().includes("node_modules") : true;
            });

            if (relevantDiagnostics.length === 0) return "";

            for (const d of relevantDiagnostics.slice(0, 10)) { 
                 const message = d.getMessageText();
                 const msgStr = typeof message === "string" ? message : message.getMessageText();
                 const line = d.getLineNumber() || "Unknown";
                 const file = d.getSourceFile()?.getBaseName() || "UnknownFile";
                 asiReport += `- [File: ${file}] Dòng [${line}]: ${msgStr}\\n`;
            }
            
            asiReport += "\\nHướng dẫn ASI: Hệ thống đã clone riêng Workspace ảo. Lỗi TS ở trên là nguyên bản (True Error). Cần check lại File/Import.\\n</actionable_side_information>";
            
            return asiReport;
         } catch(e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            return `[ASI Engine Fatal Error] Không thể trích xuất Diagnostics Sandbox: ${errMsg}`;
         }
    }
}
