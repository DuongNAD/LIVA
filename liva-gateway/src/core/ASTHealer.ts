import { ASTWorkerBridge } from "./ASTWorkerBridge";
import { logger } from "../utils/logger";
import * as path from "node:path";

function isSafeSandboxPath(sandboxRoot: string): boolean {
    if (!sandboxRoot || typeof sandboxRoot !== "string" || sandboxRoot.trim() === "") {
        return false;
    }
    // Check for path traversal attempts
    const normalized = path.normalize(sandboxRoot);
    if (sandboxRoot.includes("..") || normalized.includes("..")) {
        return false;
    }
    return true;
}

/**
 * Lớp Tự Chữa Lành và Đánh Giá AST (Evaluator Healer on Host)
 * Giúp Evaluator lấy thông tin lỗi tinh sạch và fix tự động các thư viện Import.
 *
 * [AI_CONTEXT §4 CRITICAL_DIRECTIVE] ts-morph (full multi-file type-check + import
 * healing) is CPU-heavy (>10ms) and MUST NOT run on the Gateway event loop.
 * All heavy work is offloaded to ASTWorker via ASTWorkerBridge. This class is now
 * a thin async facade preserving the original public contract.
 */
export class ASTHealer {
    /**
     * Healer Tự Động Định Tuyến Import (trên toàn bộ file TS trong Sandbox).
     * Offloaded to a worker thread to protect the event loop.
     */
    public async autoHealImportsOnSandbox(sandboxRoot: string): Promise<{ success: boolean; logs: string }> {
        if (!isSafeSandboxPath(sandboxRoot)) {
            return { success: false, logs: "⚠️ Cảnh báo Healer: Đường dẫn sandbox không hợp lệ hoặc không an toàn." };
        }
        try {
            return await ASTWorkerBridge.healImports(sandboxRoot);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            return { success: false, logs: `⚠️ Cảnh báo Healer: Không thể tự vá import. Lỗi: ${errMsg}` };
        }
    }

    /**
     * Dịch lỗi Compiler thành ASI (Actionable Side Information) cho DarwinianEvolver.
     * Offloaded to a worker thread (getPreEmitDiagnostics = full type-check).
     */
    public async getASIFromPreEmitDiagnosticsOnSandbox(sandboxRoot: string): Promise<string> {
        if (!isSafeSandboxPath(sandboxRoot)) {
            return "[ASI Engine Fatal Error] Đường dẫn sandbox không hợp lệ hoặc không an toàn.";
        }
        try {
            return await ASTWorkerBridge.getDiagnostics(sandboxRoot);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[ASTHealer] Diagnostics worker failed: ${errMsg}`);
            return `[ASI Engine Fatal Error] Không thể trích xuất Diagnostics Sandbox: ${errMsg}`;
        }
    }
}
