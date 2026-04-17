import { Project, SourceFile } from "ts-morph";
import * as fs from "fs";
import * as path from "path";

export interface FileMutation {
    type: "modify" | "create";
    filePath: string;
    className?: string;
    methodName?: string;
    code: string;
}

/**
 * Lớp Đột Biến AST (Host Gateway Actuator) - KIẾN TRÚC V7 MULTI-FILE SANDBOX
 */
export class ASTActuator {
    private workspace: string;

    constructor(workspace: string) {
        this.workspace = workspace;
    }

    /**
     * Isolated Workspace Replica: Clone toàn bộ project (trừ rác) để giữ vững Relative Imports.
     */
    private createSandboxWorkspace(candidateId: string): string {
        const sandboxRoot = path.join(this.workspace, ".liva_workspaces", candidateId);

        // Dọn rác bẩn nếu còn tồn đọng
        if (fs.existsSync(sandboxRoot)) {
            fs.rmSync(sandboxRoot, { recursive: true, force: true });
        }
        fs.mkdirSync(sandboxRoot, { recursive: true });

        console.log(`[ASTActuator] Đang Clone không gian ảo cho Ứng viên [${candidateId}]...`);
        // Clone các file lõi và source, tránh đệ quy root workspace vào thư mục con
        const workspaceSrc = path.join(this.workspace, "src");
        if (fs.existsSync(workspaceSrc)) fs.cpSync(workspaceSrc, path.join(sandboxRoot, "src"), { recursive: true });
        
        const tsconfigPath = path.join(this.workspace, "tsconfig.json");
        if (fs.existsSync(tsconfigPath)) fs.copyFileSync(tsconfigPath, path.join(sandboxRoot, "tsconfig.json"));
        
        const packageJsonPath = path.join(this.workspace, "package.json");
        if (fs.existsSync(packageJsonPath)) fs.copyFileSync(packageJsonPath, path.join(sandboxRoot, "package.json"));

        return sandboxRoot;
    }

    /**
     * Actuate candidate batch in isolated replica.
     */
    public async actuateCandidateBatch(
        candidateId: string, 
        mutations: FileMutation[]
    ): Promise<{ success: boolean; asi?: string; sandboxRoot?: string }> {
        // --- GUARDRAIL 1: Mutation Quota ---
        let createCount = 0;
        let modifyCount = 0;
        for (const m of mutations) {
            if (m.type === "create") createCount++;
            if (m.type === "modify") modifyCount++;
        }
        if (createCount > 3 || modifyCount > 5) {
            return {
                success: false,
                asi: `[ASTActuator] Vượt hạn mức đột biến (Max 3 create, 5 modify). Candidate gửi: ${createCount} create, ${modifyCount} modify. Yêu cầu chia nhỏ kiến trúc.`
            };
        }

        let sandboxRoot = "";
        try {
            sandboxRoot = this.createSandboxWorkspace(candidateId);
            
            // TS Morph Setup on the Sandbox TSCONFIG
            const project = new Project({
                tsConfigFilePath: path.join(sandboxRoot, "tsconfig.json"),
                skipAddingFilesFromTsConfig: false,
                compilerOptions: { allowJs: true }
            });

            for (const mutation of mutations) {
                // --- GUARDRAIL 2: Path Jails ---
                const normalizedPath = path.posix.normalize(mutation.filePath.replace(/\\/g, '/'));
                if (!normalizedPath.startsWith("src/") || normalizedPath.includes("..")) {
                     return { success: false, asi: `[ASTActuator] Vi phạm Vùng An Toàn: '${mutation.filePath}'. Chỉ được phép thao tác tệp trong thư mục src/.` };
                }
                if (normalizedPath === "src/skills/AIScientist.ts") {
                     return { success: false, asi: `[ASTActuator] Vi phạm Lõi: CẤM tuyệt đối sửa đổi NÃO BỘ AIScientist.ts ở giai đoạn này.` };
                }

                const absoluteSandboxFilePath = path.join(sandboxRoot, normalizedPath);
                const cleanCode = mutation.code.replace(/^\`\`\`(?:typescript|ts)?\n/i, "").replace(/\n\`\`\`$/g, "");

                if (mutation.type === "create") {
                    console.log(`[ASTActuator] Đang tạo File mới ở Sandbox: ${mutation.filePath}`);
                    fs.mkdirSync(path.dirname(absoluteSandboxFilePath), { recursive: true });
                    project.createSourceFile(absoluteSandboxFilePath, cleanCode, { overwrite: true });
                } 
                if (mutation.type === "modify") {
                    if (mutation.className) mutation.className = mutation.className.trim();
                    if (mutation.methodName) mutation.methodName = mutation.methodName.trim();
                    console.log(`[ASTActuator] Đang phẫu thuật File trong Sandbox: ${mutation.filePath} -> ${mutation.className || "Standalone"}`);
                    const sourceFile = project.getSourceFile(absoluteSandboxFilePath);
                    if (!sourceFile) {
                        return { success: false, asi: `[ASTActuator] Không tìm thấy file gốc: ${mutation.filePath} để modify.` };
                    }

                    if (!mutation.className || mutation.className === "") {
                        if (!mutation.methodName || mutation.methodName === "") {
                            // Cả class và method đều rỗng -> Replace Toàn Bộ Tệp (Whole File Modification)
                            sourceFile.replaceWithText(cleanCode);
                        } else {
                            const funcNode = sourceFile.getFunction(mutation.methodName);
                            if (!funcNode) {
                                return { success: false, asi: `[ASTActuator] Không tìm thấy hàm độc lập '${mutation.methodName}'.` };
                            }
                            funcNode.replaceWithText(cleanCode);
                        }
                    } else {
                        const classNode = sourceFile.getClass(mutation.className);
                        if (!classNode) {
                            return { success: false, asi: `[ASTActuator] Không tìm thấy lớp '${mutation.className}'.` };
                        }
                        if (!mutation.methodName) {
                             classNode.replaceWithText(cleanCode);
                        } else {
                             let methodNode: any = classNode.getMethod(mutation.methodName);
                             
                             // Mở rộng tìm kiếm: Tìm trong Property/Private Method nếu ts-morph không hiểu
                             if (!methodNode) {
                                 methodNode = classNode.getMembers().find(m => {
                                     const mName = (m as any).getName ? (m as any).getName() : undefined;
                                     return mName === mutation.methodName || mName === mutation.methodName!.replace('#', '');
                                 });
                             }

                             if (!methodNode && mutation.methodName === "constructor") {
                                 const constructors = classNode.getConstructors();
                                 if (constructors.length > 0) methodNode = constructors[0];
                                 else methodNode = classNode.addConstructor();
                             }

                             // Nếu VẪN không có -> AI Đang muốn TẠO HÀM MỚI (Brand new method)
                             if (!methodNode) {
                                 // Tạo 1 Node ảo (Dummy node) trước khi replaceWithText
                                 const safeName = mutation.methodName!.replace(/[^a-zA-Z0-9_]/g, '') || "dummyLivaMethod";
                                 methodNode = classNode.addMethod({ name: safeName });
                             }

                             methodNode.replaceWithText(cleanCode);
                        }
                    }
                }
            }

            console.log(`[ASTActuator] Cập nhật AST Batch thành công. Đang lưu Sandbox...`);
            await project.save();
            return { success: true, sandboxRoot };
            
        } catch (error: any) {
            if (sandboxRoot && fs.existsSync(sandboxRoot)) {
                fs.rmSync(sandboxRoot, { recursive: true, force: true });
            }
            return { success: false, asi: `[ASTActuator] Lỗi hệ thống khi phẫu thuật AST: ${error.message}` };
        }
    }
}
