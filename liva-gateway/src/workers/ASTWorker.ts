/**
 * ASTWorker — ts-morph heavy AST operations in an isolated Worker Thread
 * ======================================================================
 * [AI_CONTEXT §4 CRITICAL_DIRECTIVE — Event Loop Protection]
 *
 * `getPreEmitDiagnostics()` runs a FULL multi-file TypeScript type-check
 * synchronously (seconds of CPU). `fixMissingImports/organizeImports/formatText`
 * are likewise heavy. Running them on the main thread freezes the Gateway
 * (Voice Full-Duplex, WS handshake, TTS).
 *
 * This worker is spawned ONE-SHOT: it processes a single { op, sandboxRoot }
 * message, posts the result, and is terminated by the parent. No persistent
 * state, no timers → nothing to register in CoreKernel.shutdown().
 */
import { parentPort } from "node:worker_threads";
import * as path from "node:path";
import { Project, ScriptTarget, ModuleKind } from "ts-morph";
import { CSHS_WEIGHTS } from "../evolution/harness-types";

export type AstWorkerOp = "heal" | "diagnostics" | "cshsAnalyze" | "surgery";

export interface AstWorkerRequest {
    op: AstWorkerOp;
    sandboxRoot?: string;
    // For cshsAnalyze
    astDiff?: string;
    jobId?: string;
    threshold?: number;
    // For surgery
    targetFile?: string;
    instructions?: any;
}

export interface AstWorkerResponse {
    ok: boolean;
    /** For "heal" */
    success?: boolean;
    logs?: string;
    /** For "diagnostics" */
    asi?: string;
    /** For "cshsAnalyze" */
    cshsResult?: {
        score: number;
        anchors: string[];
        diagnosticCount: number;
        pass: boolean;
    };
    /** For "surgery" */
    newCode?: string;
    /** Error path */
    error?: string;
}

/**
 * Auto-route imports + clean code across the whole sandbox.
 * Mirrors the original ASTHealer.autoHealImportsOnSandbox() logic.
 */
async function healImports(sandboxRoot: string): Promise<AstWorkerResponse> {
    try {
        const project = new Project({
            tsConfigFilePath: path.join(sandboxRoot, "tsconfig.json"),
            skipAddingFilesFromTsConfig: false,
            compilerOptions: { allowJs: true },
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
        return { ok: true, success: true, logs: "✅ Đã tự động vá Imports và làm sạch Code toàn Sandbox thành công." };
    } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        return { ok: true, success: false, logs: `⚠️ Cảnh báo Healer: Không thể tự vá import. Lỗi: ${errMsg}` };
    }
}

/**
 * Collect pre-emit diagnostics → ASI report.
 * Mirrors the original ASTHealer.getASIFromPreEmitDiagnosticsOnSandbox() logic.
 */
function getDiagnostics(sandboxRoot: string): AstWorkerResponse {
    try {
        const project = new Project({
            tsConfigFilePath: path.join(sandboxRoot, "tsconfig.json"),
            skipAddingFilesFromTsConfig: false,
            compilerOptions: { allowJs: true, noEmit: true },
        });

        const diagnostics = project.getPreEmitDiagnostics();
        if (diagnostics.length === 0) return { ok: true, asi: "" };

        let asiReport = "<actionable_side_information>\\n[CẢNH BÁO TỪ TRÌNH BIÊN DỊCH AST MULTI-FILE]\\nKiến trúc đột biến bị vỡ quy tắc Typing/Syntax:\\n";

        const relevantDiagnostics = diagnostics.filter(d => {
            const f = d.getSourceFile();
            return f ? !f.getFilePath().includes("node_modules") : true;
        });

        if (relevantDiagnostics.length === 0) return { ok: true, asi: "" };

        for (const d of relevantDiagnostics.slice(0, 10)) {
            const message = d.getMessageText();
            const msgStr = typeof message === "string" ? message : message.getMessageText();
            const line = d.getLineNumber() || "Unknown";
            const file = d.getSourceFile()?.getBaseName() || "UnknownFile";
            asiReport += `- [File: ${file}] Dòng [${line}]: ${msgStr}\\n`;
        }

        asiReport += "\\nHướng dẫn ASI: Hệ thống đã clone riêng Workspace ảo. Lỗi TS ở trên là nguyên bản (True Error). Cần check lại File/Import.\\n</actionable_side_information>";
        return { ok: true, asi: asiReport };
    } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        return { ok: true, asi: `[ASI Engine Fatal Error] Không thể trích xuất Diagnostics Sandbox: ${errMsg}` };
    }
}

/**
 * CSHS Analyze (Contextual Syntax & Hallucination Score)
 */
function cshsAnalyze(astDiff: string, jobId: string, threshold: number): AstWorkerResponse {
    const project = new Project({
        useInMemoryFileSystem: true,
        compilerOptions: {
            target: ScriptTarget.ESNext,
            module: ModuleKind.ESNext,
            strict: true,
            noEmit: true,
            skipLibCheck: true,
            noUnusedLocals: true,
            allowJs: false,
        },
    });

    const fileName = `/eval_${jobId}.ts`;
    const sourceFile = project.createSourceFile(fileName, astDiff, { overwrite: true });
    
    try {
        let totalPenalty = 0;
        const anchors: string[] = [];
        const PLACEHOLDER_TOKENS = ["YOUR_CODE_HERE", "TODO:", "FIXME:", "PLACEHOLDER"];

        for (const token of PLACEHOLDER_TOKENS) {
            let count = 0;
            let index = astDiff.indexOf(token);
            while (index !== -1) {
                count += 1;
                index = astDiff.indexOf(token, index + token.length);
            }
            if (count === 0) continue;
            totalPenalty += CSHS_WEIGHTS.PLACEHOLDER_TOKEN * count;
            anchors.push(`Placeholder "${token}" detected`);
        }

        const diagnostics = sourceFile.getPreEmitDiagnostics().filter((diag) => {
            const diagSource = diag.getSourceFile();
            return !diagSource || diagSource.getFilePath() === sourceFile.getFilePath();
        });

        for (const diag of diagnostics) {
            const code = diag.getCode();
            switch (code) {
                case 2304:
                    totalPenalty += CSHS_WEIGHTS.TS2304_UNDEFINED_NAME;
                    anchors.push(`TS2304: ${diag.getMessageText()}`);
                    break;
                case 2307:
                    totalPenalty += CSHS_WEIGHTS.TS2307_UNRESOLVED_IMPORT;
                    anchors.push(`TS2307: ${diag.getMessageText()}`);
                    break;
                case 7027:
                    totalPenalty += CSHS_WEIGHTS.TS7027_UNREACHABLE_CODE;
                    anchors.push("TS7027: Unreachable code");
                    break;
                case 6133:
                    totalPenalty += CSHS_WEIGHTS.TS6133_UNUSED_VARIABLE;
                    anchors.push(`TS6133: ${diag.getMessageText()}`);
                    break;
                default:
                    totalPenalty += code < 2000 ? 0.3 : 0.05;
                    break;
            }
        }

        const score = Number(Math.min(totalPenalty, 1).toFixed(3));
        return {
            ok: true,
            cshsResult: {
                score,
                anchors: anchors.slice(0, 20),
                diagnosticCount: diagnostics.length,
                pass: score <= threshold,
            }
        };
    } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        return { ok: false, error: errMsg };
    } finally {
        project.removeSourceFile(sourceFile);
    }
}

/**
 * Apply AST Surgery
 */
function applySurgery(targetFile: string, instructions: any): AstWorkerResponse {
    try {
        const project = new Project({
            compilerOptions: { target: ScriptTarget.ESNext }
        });

        let sourceFile;
        try {
            sourceFile = project.addSourceFileAtPath(targetFile);
        } catch (e) {
            return { ok: false, error: `File không tồn tại: ${targetFile}` };
        }

        if (instructions.replaceFunctionBody && instructions.functionName) {
            const func = sourceFile.getFunction(instructions.functionName);
            if (func) {
                func.setBodyText(instructions.replaceFunctionBody);
            }
        }

        const diagnostics = project.getPreEmitDiagnostics();
        if (diagnostics.length > 0) {
            const errors = project.formatDiagnosticsWithColorAndContext(diagnostics);
            return { ok: false, error: `Lỗi cú pháp/Type script sau khi sửa:\n${errors}` };
        }

        return { ok: true, newCode: sourceFile.getFullText() };
    } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        return { ok: false, error: errMsg };
    }
}

parentPort?.on("message", async (req: AstWorkerRequest) => {
    let res: AstWorkerResponse;
    try {
        if (req.op === "heal") {
            res = await healImports(req.sandboxRoot!);
        } else if (req.op === "diagnostics") {
            res = getDiagnostics(req.sandboxRoot!);
        } else if (req.op === "cshsAnalyze") {
            res = cshsAnalyze(req.astDiff!, req.jobId!, req.threshold!);
        } else if (req.op === "surgery") {
            res = applySurgery(req.targetFile!, req.instructions);
        } else {
            res = { ok: false, error: `Unknown AST worker op: ${String((req as AstWorkerRequest).op)}` };
        }
    } catch (e: unknown) {
        res = { ok: false, error: e instanceof Error ? e.message : String(e) };
    }
    parentPort?.postMessage(res);
});
