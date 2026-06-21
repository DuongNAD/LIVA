import { safeRename } from '../utils/FileUtils';
import { ASTWorkerBridge } from "../core/ASTWorkerBridge";
import * as fsp from "fs/promises";
import * as path from "path";
import * as prettier from "prettier";
import { logger } from "../utils/logger";
import { jsonrepair } from "jsonrepair";
import { createTwoFilesPatch } from "diff";

export class SecurityViolationError extends Error {
    constructor(msg: string) { super(msg); this.name = "SecurityViolationError"; }
}

export class ASTCodeSurgeon {
    private allowedRoot: string;

    constructor() {
        this.allowedRoot = process.cwd();
    }

    public async applyAstSurgery(targetFile: string, jsonInstructions: string, dryRun: boolean = false): Promise<string> {
        // 1. Path Jail Guard
        const resolvedPath = path.resolve(this.allowedRoot, targetFile);
        if (!resolvedPath.startsWith(this.allowedRoot)) {
            throw new SecurityViolationError(`Truy cập file bị chặn (Path Traversal): ${resolvedPath}`);
        }

        // 2. Parse & Validate JSON
        let instructions: any;
        try {
            const first = jsonInstructions.indexOf('{');
            const last = jsonInstructions.lastIndexOf('}');
            if (first === -1 || last === -1) throw new Error("Missing JSON braces");
            const repaired = jsonrepair(jsonInstructions.substring(first, last + 1));
            instructions = JSON.parse(repaired);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            throw new Error(`JSON parsing failed: ${errMsg}`);
        }

        // 3. Delegate AST operations to ASTWorker
        let newCode: string;
        try {
            newCode = await ASTWorkerBridge.applySurgery(resolvedPath, instructions);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[ASTCodeSurgeon] Pre-flight Diagnostics Failed or Surgery Error: \n${errMsg}`);
            throw new Error(errMsg);
        }

        if (dryRun) {
            let formattedCode = newCode;
            try {
                formattedCode = await prettier.format(newCode, { parser: "typescript" });
            } catch (e) {
                logger.warn(`[ASTCodeSurgeon] Prettier formatting failed, falling back to raw AST output for dry-run.`);
            }

            // Read original content
            const originalCode = await fsp.readFile(resolvedPath, "utf-8");
            const diffPatch = createTwoFilesPatch(targetFile, targetFile, originalCode, formattedCode, "original", "modified");

            // Extract target symbols
            const targetSymbols: string[] = [];
            if (instructions) {
                if (typeof instructions.functionName === "string") {
                    targetSymbols.push(instructions.functionName);
                } else if (Array.isArray(instructions.functionName)) {
                    targetSymbols.push(...instructions.functionName);
                }
                if (typeof instructions.methodName === "string") {
                    targetSymbols.push(instructions.methodName);
                } else if (Array.isArray(instructions.methodName)) {
                    targetSymbols.push(...instructions.methodName);
                }
                if (typeof instructions.symbolName === "string") {
                    targetSymbols.push(instructions.symbolName);
                } else if (Array.isArray(instructions.symbolName)) {
                    targetSymbols.push(...instructions.symbolName);
                }
                if (Array.isArray(instructions)) {
                    for (const inst of instructions) {
                        if (inst) {
                            if (typeof inst.functionName === "string") targetSymbols.push(inst.functionName);
                            if (typeof inst.methodName === "string") targetSymbols.push(inst.methodName);
                            if (typeof inst.symbolName === "string") targetSymbols.push(inst.symbolName);
                        }
                    }
                }
            }

            const targetStr = targetSymbols.join(", ");

            // Read and parse ast_graph.json
            let allNodes: { name: string; filePath: string; calls: string[] }[] = [];
            let graphPath = path.resolve(this.allowedRoot, "data/ast_graph.json");
            try {
                await fsp.access(graphPath);
            } catch {
                graphPath = path.resolve(this.allowedRoot, "../data/ast_graph.json");
            }

            try {
                const graphData = await fsp.readFile(graphPath, "utf-8");
                const graph = JSON.parse(graphData);
                
                const traverseNode = (node: any, currentFilePath?: string) => {
                    const filePath = node.filePath || currentFilePath || "";
                    if (node.calls && node.calls.length > 0) {
                        allNodes.push({
                            name: node.name,
                            filePath: filePath,
                            calls: node.calls
                        });
                    }
                    if (node.children) {
                        for (const child of node.children) {
                            traverseNode(child, filePath);
                        }
                    }
                };
                traverseNode(graph);
            } catch (e) {
                logger.warn(`[ASTCodeSurgeon] Failed to read or parse ast_graph.json: ${e instanceof Error ? e.message : String(e)}`);
            }

            const directCallersSet = new Set<string>();
            const transitiveCallersSet = new Set<string>();
            const allCallersFiles = new Set<string>();

            // Find direct callers
            for (const node of allNodes) {
                const hasCall = node.calls.some(c => targetSymbols.includes(c));
                if (hasCall && !targetSymbols.includes(node.name)) {
                    directCallersSet.add(node.name);
                    allCallersFiles.add(node.filePath);
                }
            }

            // BFS for transitive callers
            const queue: string[] = Array.from(directCallersSet);
            const visited = new Set<string>(targetSymbols);
            for (const dc of directCallersSet) {
                visited.add(dc);
            }

            while (queue.length > 0) {
                const current = queue.shift()!;
                for (const node of allNodes) {
                    if (node.calls.includes(current)) {
                        if (!visited.has(node.name)) {
                            visited.add(node.name);
                            transitiveCallersSet.add(node.name);
                            allCallersFiles.add(node.filePath);
                            queue.push(node.name);
                        }
                    }
                }
            }

            const totalCallersCount = directCallersSet.size + transitiveCallersSet.size;
            let riskLevel = "LOW";
            if (totalCallersCount > 0) {
                if (totalCallersCount <= 3) {
                    riskLevel = "MEDIUM";
                } else {
                    riskLevel = "HIGH";
                }
            }

            // Check for core files
            let hasCore = false;
            for (const filePath of allCallersFiles) {
                const base = path.basename(filePath);
                if (base === "AgentLoop.ts" || base === "Gateway.ts" || base === "AgentLoop.js" || base === "Gateway.js") {
                    hasCore = true;
                    break;
                }
            }
            if (hasCore) {
                riskLevel = "CRITICAL";
            }

            return JSON.stringify({
                success: true,
                diff: diffPatch,
                blastRadius: {
                    target: targetStr,
                    directCallers: Array.from(directCallersSet),
                    transitiveCallers: Array.from(transitiveCallersSet),
                    riskLevel: riskLevel
                }
            });
        }

        // 6. Formatting & Atomic Write
        try {
            newCode = await prettier.format(newCode, { parser: "typescript" });
        } catch (e) {
            logger.warn(`[ASTCodeSurgeon] Prettier formatting failed, falling back to raw AST output.`);
        }

        const bakPath = `${resolvedPath}.bak`;
        const tmpPath = `${resolvedPath}.tmp`;

        try {
            await fsp.copyFile(resolvedPath, bakPath);
            await fsp.writeFile(tmpPath, newCode, "utf-8");
            await safeRename(tmpPath, resolvedPath);
            
            logger.info(`[ASTCodeSurgeon] Đã sửa file thành công: ${resolvedPath}`);
            return "SUCCESS";
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[ASTCodeSurgeon] Lỗi I/O: ${errMsg}`);
            await this.revert(targetFile).catch(() => {});
            throw e;
        }
    }

    public async revert(targetFile: string): Promise<boolean> {
        const resolvedPath = path.resolve(this.allowedRoot, targetFile);
        if (!resolvedPath.startsWith(this.allowedRoot)) {
            throw new SecurityViolationError(`Truy cập file bị chặn (Path Traversal): ${resolvedPath}`);
        }
        
        const bakPath = `${resolvedPath}.bak`;
        try {
            await safeRename(bakPath, resolvedPath);
            logger.info(`[ASTCodeSurgeon] Reverted file: ${resolvedPath}`);
            return true;
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[ASTCodeSurgeon] Revert failed: ${errMsg}`);
            return false;
        }
    }
}
