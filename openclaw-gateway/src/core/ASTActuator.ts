import { Project, SourceFile } from "ts-morph";
import * as fs from "fs";
import * as path from "path";
import * as diffLib from "diff";

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
                const absoluteSandboxFilePath = path.join(sandboxRoot, normalizedPath);
                const cleanCode = mutation.code.replace(/^\`\`\`(?:diff|typescript|ts)?\n/i, "").replace(/\n\`\`\`$/g, "");

                if (mutation.type === "create") {
                    console.log(`[ASTActuator] Đang tạo File mới ở Sandbox: ${mutation.filePath}`);
                    fs.mkdirSync(path.dirname(absoluteSandboxFilePath), { recursive: true });
                    
                    let newCode = cleanCode;
                    if (cleanCode.includes("@@") && cleanCode.includes("\n+")) {
                         newCode = cleanCode.split('\n')
                            .filter(l => l.startsWith('+') && !l.startsWith('+++'))
                            .map(l => l.substring(1)).join('\n');
                    }
                    
                    fs.writeFileSync(absoluteSandboxFilePath, newCode);
                    project.addSourceFileAtPath(absoluteSandboxFilePath);
                } 
                if (mutation.type === "modify") {
                    console.log(`[ASTActuator] Đang phẫu thuật File bằng Git Patch: ${mutation.filePath}`);
                    if (!fs.existsSync(absoluteSandboxFilePath)) {
                        return { success: false, asi: `[ASTActuator] Không tìm thấy file gốc: ${mutation.filePath} để patch.` };
                    }
                    
                    const sourceCode = fs.readFileSync(absoluteSandboxFilePath, 'utf8');
                    
                    let pCode = cleanCode;
                    if (!pCode.startsWith('---')) {
                         pCode = `--- a/${mutation.filePath}\n+++ b/${mutation.filePath}\n` + pCode;
                    }

                    let patchedCode: string | false = false;
                    try {
                        patchedCode = diffLib.applyPatch(sourceCode, pCode, { fuzzFactor: 2 });
                    } catch (err: any) {
                        return { success: false, asi: `[ASTActuator] Lỗi thư viện Diff: ${err.message}` };
                    }

                    if (patchedCode === false) {
                        return { 
                            success: false, 
                            asi: `[ASTActuator] Khớp Patch thất bại! Mã Diff không hợp lệ.\nPatch:\n${cleanCode}` 
                        };
                    }

                    fs.writeFileSync(absoluteSandboxFilePath, patchedCode);
                    const sourceFile = project.getSourceFile(absoluteSandboxFilePath);
                    if (sourceFile) {
                        sourceFile.replaceWithText(patchedCode);
                    } else {
                        project.addSourceFileAtPath(absoluteSandboxFilePath);
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
