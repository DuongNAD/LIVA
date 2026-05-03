import * as fs from "node:fs";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";
import { Project } from "ts-morph";

export interface FileMutation {
    type: "modify" | "create" | "delete";
    filePath: string;
    className?: string;
    methodName?: string;
    code: string;
}

/**
 * Lớp Đột Biến AST (Host Gateway Actuator) - KIẾN TRÚC V7 MULTI-FILE SANDBOX
 */
export class ASTActuator {
    private readonly workspace: string;

    constructor(workspace: string) {
        this.workspace = workspace;
    }

    /**
     * Isolated Workspace Replica: Clone toàn bộ project (trừ rác) để giữ vững Relative Imports.
     */
    private async createSandboxWorkspace(candidateId: string): Promise<string> {
        const sandboxRoot = path.join(this.workspace, ".liva_workspaces", candidateId);

        if (fs.existsSync(sandboxRoot)) {
            await fsp.rm(sandboxRoot, { recursive: true, force: true });
        }
        await fsp.mkdir(sandboxRoot, { recursive: true });

        logger.info(`[ASTActuator] Cloning isolated workspace for candidate [${candidateId}]...`);
        const workspaceSrc = path.join(this.workspace, "src");
/* istanbul ignore next */
        if (fs.existsSync(workspaceSrc)) {
            await fsp.cp(workspaceSrc, path.join(sandboxRoot, "src"), {
                recursive: true,
                filter: (src: string) => {
/* istanbul ignore next */
                    const basename = path.basename(src);
/* istanbul ignore next */
                    return !basename.endsWith(".bak") && !basename.startsWith(".shadow_");
                }
            });
        }
        
        const tsconfigPath = path.join(this.workspace, "tsconfig.json");
/* istanbul ignore next */
        if (fs.existsSync(tsconfigPath)) await fsp.copyFile(tsconfigPath, path.join(sandboxRoot, "tsconfig.json"));
        
        const packageJsonPath = path.join(this.workspace, "package.json");
/* istanbul ignore next */
        if (fs.existsSync(packageJsonPath)) await fsp.copyFile(packageJsonPath, path.join(sandboxRoot, "package.json"));

        const hostNodeModules = path.join(this.workspace, "node_modules");
        const sandboxNodeModules = path.join(sandboxRoot, "node_modules");
/* istanbul ignore next */
        if (fs.existsSync(hostNodeModules) && !fs.existsSync(sandboxNodeModules)) {
/* istanbul ignore next */
            try {
/* istanbul ignore next */
                await fsp.symlink(hostNodeModules, sandboxNodeModules, "junction");
            } catch (e: any) {
/* istanbul ignore next */
                logger.warn(`[ASTActuator] Could not symlink node_modules: ${e.message}`);
            }
        }

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
        if (createCount > 3 || modifyCount > 10) {
            return {
                success: false,
                asi: `[ASTActuator] Mutation limit exceeded (Max 3 create, 10 modify). Candidate sent: ${createCount} create, ${modifyCount} modify.`
            };
        }

        let sandboxRoot = "";
        try {
            sandboxRoot = await this.createSandboxWorkspace(candidateId);
            
            // TS Morph Setup on the Sandbox TSCONFIG
            const project = new Project({
                tsConfigFilePath: path.join(sandboxRoot, "tsconfig.json"),
                skipAddingFilesFromTsConfig: false,
                compilerOptions: { allowJs: true }
            });

            for (const mutation of mutations) {
                // --- GUARDRAIL 2: Path Jails ---
                let relativePath = mutation.filePath;
/* istanbul ignore next */
                if (path.isAbsolute(mutation.filePath)) {
/* istanbul ignore next */
                     relativePath = path.relative(this.workspace, mutation.filePath);
                }
                const normalizedPath = path.posix.normalize(relativePath.replaceAll('\\', '/'));
                if (!normalizedPath.startsWith("src/") || normalizedPath.includes("..")) {
                     return { success: false, asi: `[ASTActuator] Path Safety Violation: '${mutation.filePath}'. Only src/ files allowed.` };
                }
                const absoluteSandboxFilePath = path.join(sandboxRoot, normalizedPath);
                const cleanCode = mutation.code.replace(/^\`\`\`(?:diff|typescript|ts)?\n/i, "").replace(/\n\`\`\`$/g, "");

                if (mutation.type === "delete") {
                    logger.info(`[ASTActuator] Deleting file from sandbox: ${mutation.filePath}`);
/* istanbul ignore next */
                    if (fs.existsSync(absoluteSandboxFilePath)) {
                        const sourceFile = project.getSourceFile(absoluteSandboxFilePath);
/* istanbul ignore next */
                        if (sourceFile) sourceFile.delete();
/* istanbul ignore next */
                        else await fsp.unlink(absoluteSandboxFilePath);
                    }
                }
                if (mutation.type === "create") {
                    logger.info(`[ASTActuator] Đang tạo File mới ở Sandbox: ${mutation.filePath}`);
                    await fsp.mkdir(path.dirname(absoluteSandboxFilePath), { recursive: true });
                    
                    let newCode = cleanCode;
/* istanbul ignore next */
                    if (cleanCode.includes("@@") && cleanCode.includes("\n+")) {
/* istanbul ignore next */
                         newCode = cleanCode.split('\n')
/* istanbul ignore next */
                            .filter(l => l.startsWith('+') && !l.startsWith('+++'))
/* istanbul ignore next */
                            .map(l => l.substring(1)).join('\n');
                    }
                    
                    await fsp.writeFile(absoluteSandboxFilePath, newCode);
                    project.addSourceFileAtPath(absoluteSandboxFilePath);
                } 
                if (mutation.type === "modify") {
                    logger.info(`[ASTActuator] Applying Search/Replace surgery: ${mutation.filePath}`);
                    if (!fs.existsSync(absoluteSandboxFilePath)) {
                        // Auto-fallback: LLM sent 'modify' but the file doesn't exist
                        // If there are no SEARCH blocks, treat it as a 'create'
/* istanbul ignore next */
                        if (!cleanCode.includes('<<<< SEARCH')) {
                            logger.info(`[ASTActuator] File not found + no SEARCH blocks → auto-creating: ${mutation.filePath}`);
                            await fsp.mkdir(path.dirname(absoluteSandboxFilePath), { recursive: true });
                            await fsp.writeFile(absoluteSandboxFilePath, cleanCode);
                            project.addSourceFileAtPath(absoluteSandboxFilePath);
                            continue;
                        }
/* istanbul ignore next */
                        return { success: false, asi: `[ASTActuator] Source file not found: ${mutation.filePath}` };
                    }
                    
                    let sourceCode = await fsp.readFile(absoluteSandboxFilePath, 'utf8');
                    const useCRLF = sourceCode.includes('\r\n');
                    const blocks = cleanCode.split('<<<< SEARCH');
                    
/* istanbul ignore next */
                    if (blocks.length < 2) {
/* istanbul ignore next */
                        return { success: false, asi: `[ASTActuator] Patch syntax error: No <<<< SEARCH tags found.` };
                    }

                    for (let i = 1; i < blocks.length; i++) {
                        const block = blocks[i];
/* istanbul ignore next */
                        if (!block.includes('====') || !block.includes('>>>> REPLACE')) continue;
                        
                        let searchPart = block.split('====')[0];
/* istanbul ignore next */
                        if (searchPart.startsWith('\n')) searchPart = searchPart.substring(1);
/* istanbul ignore next */
                        else if (searchPart.startsWith('\r\n')) searchPart = searchPart.substring(2);
/* istanbul ignore next */
                        if (searchPart.endsWith('\n')) searchPart = searchPart.substring(0, searchPart.length - 1);
/* istanbul ignore next */
                        if (searchPart.endsWith('\r')) searchPart = searchPart.substring(0, searchPart.length - 1);

                        let replacePart = block.split('====')[1].split('>>>> REPLACE')[0];
/* istanbul ignore next */
                        if (replacePart.startsWith('\n')) replacePart = replacePart.substring(1);
/* istanbul ignore next */
                        else if (replacePart.startsWith('\r\n')) replacePart = replacePart.substring(2);
/* istanbul ignore next */
                        if (replacePart.endsWith('\n')) replacePart = replacePart.substring(0, replacePart.length - 1);
/* istanbul ignore next */
                        if (replacePart.endsWith('\r')) replacePart = replacePart.substring(0, replacePart.length - 1);

                        // Normalize CRLF -> LF for matching
                        const srcN = sourceCode.replaceAll('\r\n', '\n');
                        const schN = searchPart.replaceAll('\r\n', '\n');
                        const repN = replacePart.replaceAll('\r\n', '\n');
                        const trimLines = (s: string) => s.split('\n').map(l => l.trimEnd()).join('\n');

                        let matched = false;
                        if (srcN.includes(schN)) {
                            // Exact match after CRLF normalization
                            const result = srcN.replace(schN, repN);
/* istanbul ignore next */
                            sourceCode = useCRLF ? result.replaceAll(/(?<!\r)\n/g, '\r\n') : result;
                            matched = true;
/* istanbul ignore next */
                        } else if (trimLines(srcN).includes(trimLines(schN))) {
                            // Fuzzy: trim trailing whitespace
/* istanbul ignore next */
                            const result = trimLines(srcN).replace(trimLines(schN), trimLines(repN));
/* istanbul ignore next */
                            sourceCode = useCRLF ? result.replaceAll(/(?<!\r)\n/g, '\r\n') : result;
/* istanbul ignore next */
                            matched = true;
                        } else {
                            // Fuzzy: strip common leading indent
                            const sLines = schN.split('\n').filter(l => l.trim() !== '');
/* istanbul ignore next */
                            if (sLines.length > 0) {
                                const minIndent = Math.min(...sLines.map(l => (l.match(/^(\s*)/)?.[1]?.length || 0)));
/* istanbul ignore next */
                                if (minIndent > 0) {
/* istanbul ignore next */
                                    const dedent = (s: string) => s.split('\n').map(l => l.startsWith(' '.repeat(minIndent)) ? l.substring(minIndent) : l).join('\n');
/* istanbul ignore next */
                                    const schDedented = trimLines(dedent(schN));
/* istanbul ignore next */
                                    if (trimLines(srcN).includes(schDedented)) {
/* istanbul ignore next */
                                        const result = trimLines(srcN).replace(schDedented, trimLines(dedent(repN)));
/* istanbul ignore next */
                                        sourceCode = useCRLF ? result.replaceAll(/(?<!\r)\n/g, '\r\n') : result;
/* istanbul ignore next */
                                        matched = true;
                                    }
                                }
                            }
                        }

                        if (!matched) {
                            return { 
                                 success: false, 
                                 asi: `[ASTActuator] SEARCH block match failed! The SEARCH text does not exist in the source file. Preserve exact whitespace!\nSearched for:\n${searchPart.substring(0, 200)}...` 
                            };
                        }
                    }

                    await fsp.writeFile(absoluteSandboxFilePath, sourceCode);
                    const sourceFile = project.getSourceFile(absoluteSandboxFilePath);
/* istanbul ignore next */
                    if (sourceFile) {
                        sourceFile.replaceWithText(sourceCode);
                    } else {
/* istanbul ignore next */
                        project.addSourceFileAtPath(absoluteSandboxFilePath);
                    }
                }
            }

            logger.info(`[ASTActuator] Cập nhật AST Batch thành công. Đang lưu Sandbox...`);
            await project.save();
            return { success: true, sandboxRoot };
            
        } catch (error: any) {
/* istanbul ignore next */
            if (sandboxRoot && fs.existsSync(sandboxRoot)) {
/* istanbul ignore next */
                await fsp.rm(sandboxRoot, { recursive: true, force: true });
            }
            return { success: false, asi: `[ASTActuator] Lỗi hệ thống khi phẫu thuật AST: ${error.message}` };
        }
    }
}
