import { Project, SourceFile } from "ts-morph";
import * as fs from "fs";
import * as path from "path";

/**
 * Lớp Đột Biến AST (Host Gateway Actuator)
 * Tinh chỉnh 1: Chống Xung đột Ghi đè (Shadow Workspace)
 * Tinh chỉnh 2: Bẫy ngoại lệ Host (Exception Fallbacks to ASI)
 */
export class ASTActuator {
    private project: Project;
    private workspace: string;

    constructor(workspace: string) {
        this.workspace = workspace;
        // Host gateway setup: tránh nạp toàn node_modules gây nổ RAM
        this.project = new Project({
            skipAddingFilesFromTsConfig: true,
            compilerOptions: { allowJs: true, strict: false }
        });
    }

    /**
     * Tinh chỉnh 1: Shadow Workspace chống Race Conditions
     * Nhân bản file ra thư mục tạm theo ứng viên (cand_A, cand_B) thay vì ghi đè trực tiếp.
     */
    private async createShadowClone(targetFileRelativePath: string, candidateId: string): Promise<string> {
        const originalPath = path.isAbsolute(targetFileRelativePath) 
            ? targetFileRelativePath 
            : path.join(this.workspace, targetFileRelativePath);
            
        if (!fs.existsSync(originalPath)) {
            throw new Error(`File không tồn tại: ${originalPath}`);
        }
        
        // Tinh chỉnh V6 Ultimate: Lưu shadow file ngay trong CÙNG thư mục gốc
        // để trình biên dịch ts-morph tự giải quyết được các Relative Imports (../xxx.ts)
        const parsedPath = path.parse(originalPath);
        const shadowFilePath = path.join(parsedPath.dir, `.shadow_${candidateId}_${parsedPath.name}${parsedPath.ext}`);
        
        fs.copyFileSync(originalPath, shadowFilePath);
        return shadowFilePath;
    }

    /**
     * Tinh chỉnh 2: Bẫy ngoại lệ & sinh ASI
     * Phẫu thuật thân hàm (Function/Method) một cách an toàn.
     */
    public async replaceMethod(
        targetFileName: string, 
        candidateId: string,
        className: string, 
        methodName: string, 
        newCode: string
    ): Promise<{ success: boolean; asi?: string; shadowPath?: string }> {
        let shadowPath = "";
        try {
            shadowPath = await this.createShadowClone(targetFileName, candidateId);
            const sourceFile = this.project.addSourceFileAtPath(shadowPath);
            
            const cleanCode = newCode.replace(/^\`\`\`(?:typescript|ts)?\n/i, "").replace(/\n\`\`\`$/g, "");

            console.log(`[ASTActuator] ShadowPath mục tiêu: ${shadowPath}`);

            // Trường hợp 1: Hàm độc lập (Standalone Function)
            if (!className || className === "") {
                console.log(`[ASTActuator] Đang phẫu thuật hàm độc lập '${methodName}'...`);
                const funcNode = sourceFile.getFunction(methodName);
                if (!funcNode) {
                    const availableFuncs = sourceFile.getFunctions().map(f => f.getName()).join(", ");
                    this.project.removeSourceFile(sourceFile);
                    return {
                        success: false,
                        asi: `[ASTActuator] Thất bại: Không tìm thấy hàm độc lập '${methodName}'. Các hàm khả dụng: [${availableFuncs}].`
                    };
                }
                funcNode.setBodyText(cleanCode);
            } 
            // Trường hợp 2: Phương thức trong Lớp (Class Method)
            else {
                console.log(`[ASTActuator] Đang tìm Lớp '${className}' để phẫu thuật phương thức '${methodName}'...`);
                const classNode = sourceFile.getClass(className);
                if (!classNode) {
                    const availableClasses = sourceFile.getClasses().map(c => c.getName()).join(", ");
                    this.project.removeSourceFile(sourceFile);
                    return { 
                        success: false, 
                        asi: `[ASTActuator] Thất bại: Không tìm thấy lớp '${className}'. Các lớp khả dụng trong tệp này: [${availableClasses}].` 
                    };
                }

                const methodNode = classNode.getMethod(methodName);
                if (!methodNode) {
                    const availableMethods = classNode.getMethods().map(m => m.getName()).join(", ");
                    this.project.removeSourceFile(sourceFile);
                    return {
                        success: false,
                        asi: `[ASTActuator] Thất bại: Không tìm thấy phương thức '${methodName}' trong lớp '${className}'. Các phương thức: [${availableMethods}].`
                    };
                }
                methodNode.setBodyText(cleanCode);
            }

            console.log(`[ASTActuator] Cập nhật AST thành công. Đang Save file nháp...`);
            await sourceFile.save();
            
            this.project.removeSourceFile(sourceFile);
            console.log(`[ASTActuator] Hoàn tất phẫu thuật Candidate [${candidateId}].`);
            return { success: true, shadowPath };
            
        } catch (error: any) {
            return { 
                success: false, 
                asi: `[ASTActuator] Lỗi hệ thống bất ngờ khi phân giải Node AST: ${error.message}. Bạn có thể đã làm mất cân bằng cấu trúc ở đâu đó.` 
            };
        }
    }

    /**
     * Thêm property cục bộ vào Class thay vì tự viết.
     */
    public async addProperty(
        targetFileName: string,
        candidateId: string,
        className: string,
        propertyName: string,
        typeStr: string = "any",
        initializer?: string
    ): Promise<{ success: boolean; asi?: string; shadowPath?: string }> {
        let shadowPath = "";
        try {
            shadowPath = await this.createShadowClone(targetFileName, candidateId);
            const sourceFile = this.project.addSourceFileAtPath(shadowPath);
            
            const classNode = sourceFile.getClass(className);
            if (!classNode) {
                this.project.removeSourceFile(sourceFile);
                return { success: false, asi: `[ASTActuator] Không tìm thấy lớp '${className}'.` };
            }

            if (classNode.getProperty(propertyName)) {
                 this.project.removeSourceFile(sourceFile);
                 return { success: false, asi: `[ASTActuator] Thuộc tính '${propertyName}' đã tồn tại.` };
            }

            classNode.addProperty({
                name: propertyName,
                type: typeStr,
                initializer: initializer
            });

            await sourceFile.save();
            this.project.removeSourceFile(sourceFile);
            return { success: true, shadowPath };

        } catch (error: any) {
            return { success: false, asi: `[ASTActuator] Error: ${error.message}` };
        }
    }
}
