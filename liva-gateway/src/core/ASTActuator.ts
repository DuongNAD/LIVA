import { promises as fsp, constants as fsc } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";

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
    private static readonly activeSandboxes = new Set<string>();

    constructor(workspace: string) {
        this.workspace = workspace;
    }

    /**
     * Isolated Workspace Replica: Clone toàn bộ project (trừ rác) để giữ vững Relative Imports.
     */
    /** Non-blocking existence check (replaces fs.existsSync) */
    private async pathExists(p: string): Promise<boolean> {
        try { await fsp.access(p, fsc.F_OK); return true; } catch { return false; }
    }

    private async createSandboxWorkspace(candidateId: string): Promise<string> {
        const sandboxRoot = path.join(this.workspace, ".liva_workspaces", candidateId);

        if (await this.pathExists(sandboxRoot)) {
            await fsp.rm(sandboxRoot, { recursive: true, force: true });
        }
        await fsp.mkdir(sandboxRoot, { recursive: true });

        logger.info(`[ASTActuator] Cloning isolated workspace for candidate [${candidateId}]...`);
        const workspaceSrc = path.join(this.workspace, "src");
/* istanbul ignore next */
        if (await this.pathExists(workspaceSrc)) {
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
        if (await this.pathExists(tsconfigPath)) await fsp.copyFile(tsconfigPath, path.join(sandboxRoot, "tsconfig.json"));
        
        const packageJsonPath = path.join(this.workspace, "package.json");
/* istanbul ignore next */
        if (await this.pathExists(packageJsonPath)) await fsp.copyFile(packageJsonPath, path.join(sandboxRoot, "package.json"));

        const hostNodeModules = path.join(this.workspace, "node_modules");
        const sandboxNodeModules = path.join(sandboxRoot, "node_modules");
/* istanbul ignore next */
        if (await this.pathExists(hostNodeModules) && !(await this.pathExists(sandboxNodeModules))) {
/* istanbul ignore next */
            try {
/* istanbul ignore next */
                await fsp.symlink(hostNodeModules, sandboxNodeModules, "junction");
            } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
/* istanbul ignore next */
                logger.warn(`[ASTActuator] Could not symlink node_modules: ${errMsg}`);
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
        // --- GUARDRAIL 0: Concurrency Lock ---
        if (ASTActuator.activeSandboxes.has(candidateId)) {
            return {
                success: false,
                asi: `[ASTActuator] Concurrency Violation: Sandbox for candidate [${candidateId}] is currently locked by another operation.`
            };
        }
        ASTActuator.activeSandboxes.add(candidateId);

        try {
            return await this.executeBatch(candidateId, mutations);
        } finally {
            ASTActuator.activeSandboxes.delete(candidateId);
        }
    }

    private async executeBatch(
        candidateId: string, 
        mutations: FileMutation[]
    ): Promise<{ success: boolean; asi?: string; sandboxRoot?: string }> {
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
            const resolvedSandboxRoot = await fsp.realpath(sandboxRoot).catch(() => sandboxRoot);

            for (const mutation of mutations) {
                let relativePath = mutation.filePath;
                if (path.isAbsolute(mutation.filePath)) {
                     relativePath = path.relative(this.workspace, mutation.filePath);
                }
                const normalizedPath = path.posix.normalize(relativePath.replaceAll('\\', '/'));
                if (!normalizedPath.startsWith("src/") || normalizedPath.includes("..")) {
                     return { success: false, asi: `[ASTActuator] Path Safety Violation: '${mutation.filePath}'. Only src/ files allowed.` };
                }
                const absoluteSandboxFilePath = path.join(sandboxRoot, normalizedPath);

                const binaryExtensions = [
                    ".png", ".jpg", ".jpeg", ".gif", ".ico", ".webp", ".pdf", ".zip", ".tar", ".gz", ".7z",
                    ".woff", ".woff2", ".ttf", ".eot", ".mp3", ".mp4", ".wav", ".wasm", ".exe", ".dll", ".so", ".dylib"
                ];
                const ext = path.extname(mutation.filePath).toLowerCase();
                if (binaryExtensions.includes(ext)) {
                    return { success: false, asi: `[ASTActuator] Binary File Violation: Cannot modify binary file '${mutation.filePath}'.` };
                }
                if (mutation.code && (mutation.code.includes('\0') || mutation.code.length > 1024 * 1024)) {
                    return { success: false, asi: `[ASTActuator] Payload Violation: Mutation code contains null bytes or exceeds 1MB.` };
                }

                if (await this.pathExists(absoluteSandboxFilePath)) {
                    try {
                        const resolvedPath = await fsp.realpath(absoluteSandboxFilePath);
                        const normalizedReal = path.normalize(resolvedPath);
                        const normalizedSandbox = path.normalize(resolvedSandboxRoot);
                        if (!normalizedReal.startsWith(normalizedSandbox)) {
                            return { success: false, asi: `[ASTActuator] Symlink Escape Violation: Path resolves outside sandbox.` };
                        }
                    } catch {
                        return { success: false, asi: `[ASTActuator] Symlink Error: Could not resolve realpath for '${mutation.filePath}'.` };
                    }
                }

                let parentDir = path.dirname(absoluteSandboxFilePath);
                while (parentDir && parentDir !== sandboxRoot && parentDir !== path.dirname(sandboxRoot)) {
                    if (await this.pathExists(parentDir)) {
                        try {
                            const resolvedParent = await fsp.realpath(parentDir);
                            const normalizedParent = path.normalize(resolvedParent);
                            const normalizedSandbox = path.normalize(resolvedSandboxRoot);
                            if (!normalizedParent.startsWith(normalizedSandbox)) {
                                return { success: false, asi: `[ASTActuator] Symlink Escape Violation: Parent directory resolves outside sandbox.` };
                            }
                        } catch {
                            return { success: false, asi: `[ASTActuator] Error resolving parent path for '${parentDir}'.` };
                        }
                        break;
                    }
                    parentDir = path.dirname(parentDir);
                }

                const cleanCode = mutation.code.replace(/^\`\`\`(?:diff|typescript|ts)?\n/i, "").replace(/\n\`\`\`$/g, "");

                if (mutation.type === "delete") {
                    logger.info(`[ASTActuator] Deleting file from sandbox: ${mutation.filePath}`);
                    if (await this.pathExists(absoluteSandboxFilePath)) {
                        try { await fsp.unlink(absoluteSandboxFilePath); } catch {
                            return { success: false, asi: `[ASTActuator] Failed to delete file: ${mutation.filePath}` };
                        }
                    }
                }
                if (mutation.type === "create") {
                    logger.info(`[ASTActuator] Đang tạo File mới ở Sandbox: ${mutation.filePath}`);
                    await fsp.mkdir(path.dirname(absoluteSandboxFilePath), { recursive: true });
                    
                    let newCode = cleanCode;
                    if (cleanCode.includes("@@") && cleanCode.includes("\n+")) {
                         newCode = cleanCode.split('\n')
                            .filter(l => l.startsWith('+') && !l.startsWith('+++'))
                            .map(l => l.substring(1)).join('\n');
                    }
                    
                    await fsp.writeFile(absoluteSandboxFilePath, newCode);
                } 
                if (mutation.type === "modify") {
                    logger.info(`[ASTActuator] Applying Search/Replace surgery: ${mutation.filePath}`);
                    if (!(await this.pathExists(absoluteSandboxFilePath))) {
                        if (!cleanCode.includes('<<<< SEARCH')) {
                            logger.info(`[ASTActuator] File not found + no SEARCH blocks → auto-creating: ${mutation.filePath}`);
                            await fsp.mkdir(path.dirname(absoluteSandboxFilePath), { recursive: true });
                            await fsp.writeFile(absoluteSandboxFilePath, cleanCode);
                            continue;
                        }
                        return { success: false, asi: `[ASTActuator] Source file not found: ${mutation.filePath}` };
                    }
                    
                    const stats = await fsp.stat(absoluteSandboxFilePath);
                    if (stats.size > 1024 * 1024) {
                        return { success: false, asi: `[ASTActuator] File Size Violation: File '${mutation.filePath}' exceeds 1MB limit.` };
                    }

                    let sourceCode = await fsp.readFile(absoluteSandboxFilePath, 'utf8');
                    const useCRLF = sourceCode.includes('\r\n');
                    const blocks = cleanCode.split('<<<< SEARCH');
                    
                    if (blocks.length < 2) {
                        return { success: false, asi: `[ASTActuator] Patch syntax error: No <<<< SEARCH tags found.` };
                    }

                    for (let i = 1; i < blocks.length; i++) {
                        const block = blocks[i];
                        if (!block.includes('====') || !block.includes('>>>> REPLACE')) continue;
                        
                        const searchPart = block.split('====')[0].replace(/^\r?\n/, '').replace(/\r?\n$/, '');
                        const replacePart = block.split('====')[1].split('>>>> REPLACE')[0].replace(/^\r?\n/, '').replace(/\r?\n$/, '');

                        const srcN = sourceCode.replaceAll('\r\n', '\n');
                        const schN = searchPart.replaceAll('\r\n', '\n');
                        const repN = replacePart.replaceAll('\r\n', '\n');
                        
                        const matchCount = srcN.split(schN).length - 1;

                        if (matchCount === 0) {
                             return { 
                                 success: false, 
                                 asi: `[ASTActuator] SEARCH block match failed! Text not found.\nSearched for:\n${searchPart.substring(0, 200)}...` 
                            };
                        }

                        if (matchCount > 1) {
                            return { 
                                 success: false, 
                                 asi: `[ASTActuator] Ambiguity Error: The SEARCH block matched ${matchCount} times in the file. Please provide more surrounding lines in the SEARCH block to make it unique.` 
                            };
                        }

                        const result = srcN.replace(schN, repN);
                        sourceCode = useCRLF ? result.replaceAll(/(?<!\r)\n/g, '\r\n') : result;
                    }

                    await fsp.writeFile(absoluteSandboxFilePath, sourceCode);
                }
            }

            logger.info(`[ASTActuator] Cập nhật AST Batch thành công.`);
            return { success: true, sandboxRoot };
            
        } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
            if (sandboxRoot && await this.pathExists(sandboxRoot)) {
                await fsp.rm(sandboxRoot, { recursive: true, force: true });
            }
            return { success: false, asi: `[ASTActuator] Lỗi hệ thống khi phẫu thuật AST: ${errMsg}` };
        }
    }
}
