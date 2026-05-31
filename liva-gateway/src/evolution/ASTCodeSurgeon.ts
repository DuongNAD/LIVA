import { safeRename } from '../utils/FileUtils';
import { ASTWorkerBridge } from "../core/ASTWorkerBridge";
import * as fsp from "fs/promises";
import * as path from "path";
import * as prettier from "prettier";
import { logger } from "../utils/logger";
import { jsonrepair } from "jsonrepair";

export class SecurityViolationError extends Error {
    constructor(msg: string) { super(msg); this.name = "SecurityViolationError"; }
}

export class ASTCodeSurgeon {
    private allowedRoot: string;

    constructor() {
        this.allowedRoot = process.cwd();
    }

    public async applyAstSurgery(targetFile: string, jsonInstructions: string): Promise<string> {
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
