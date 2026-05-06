import { spawn } from "child_process";
import { promises as fs } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";
import { ASTGraphBuilder } from "./ASTGraphBuilder";

/**
 * Resolves the gitnexus binary path.
 * Priority: local node_modules/.bin > global PATH fallback.
 * On Windows, uses .cmd shim.
 */
function resolveGitNexusBin(): string {
    const isWindows = process.platform === "win32";
    const binName = isWindows ? "gitnexus.cmd" : "gitnexus";

    // Try project-local (gateway) node_modules first
    const localBin = path.resolve(process.cwd(), "node_modules", ".bin", binName);
    // Try monorepo root node_modules
    const rootBin = path.resolve(process.cwd(), "..", "node_modules", ".bin", binName);

    // Sync check is acceptable here — called once at class construction, not in hot path
    const fsSyncModule = require("fs");
    if (fsSyncModule.existsSync(localBin)) return localBin;
    if (fsSyncModule.existsSync(rootBin)) return rootBin;

    // Fallback: assume globally installed and available in PATH
    return binName;
}

export interface GitNexusIndexOptions {
    /** Enable semantic embeddings generation (heavy, opt-in). Default: false */
    embeddings?: boolean;
}

export class GitNexusIndexer {
    private indexing: boolean = false;
    private debounceTimer: NodeJS.Timeout | null = null;

    public triggerIndex(delayMs: number = 5000, options: GitNexusIndexOptions = {}) {
        if (this.debounceTimer) {
            clearTimeout(this.debounceTimer);
        }
        this.debounceTimer = setTimeout(() => {
            void this.runIndex(options);
        }, delayMs);
    }

    private async runIndex(options: GitNexusIndexOptions = {}) {
        if (this.indexing) return;
        this.indexing = true;
        try {
            const binPath = resolveGitNexusBin();
            const args = ["analyze"];
            if (options.embeddings) {
                args.push("--embeddings");
            }

            logger.info(`[GitNexusIndexer] Đang chạy: ${binPath} ${args.join(" ")}`);

            const result = await new Promise<{ stdout: string; stderr: string }>((resolve, reject) => {
                const proc = spawn(binPath, args, {
                    cwd: process.cwd(),
                    shell: process.platform === "win32",
                    stdio: ["ignore", "pipe", "pipe"],
                });

                let stdout = "";
                let stderr = "";
                proc.stdout.on("data", (d: Buffer) => { stdout += d.toString(); });
                proc.stderr.on("data", (d: Buffer) => { stderr += d.toString(); });

                proc.on("close", (code) => {
                    if (code === 0) resolve({ stdout, stderr });
                    else reject(new Error(`gitnexus exited with code ${code}: ${stderr}`));
                });

                proc.on("error", (err) => reject(err));
            });

            logger.info(`[GitNexusIndexer] Hoàn tất CLI: ${result.stdout}`);
            if (result.stderr) logger.warn(`[GitNexusIndexer] Cảnh báo CLI: ${result.stderr}`);

            // NEW: AST Hierarchical Graph Building (Phase 2)
            logger.info("[GitNexusIndexer] Bắt đầu xây dựng AST Hierarchical Graph...");
            const projectRoot = path.resolve(process.cwd(), "..");
            const graphBuilder = new ASTGraphBuilder(projectRoot);
            
            // Lọc thư mục gateway hoặc thư mục gốc
            const astGraph = await graphBuilder.buildGraph(path.resolve(projectRoot, "openclaw-gateway", "src"));
            
            // Lưu ra file JSON
            const dataDir = path.resolve(projectRoot, "data");
            await fs.mkdir(dataDir, { recursive: true });
            const graphPath = path.join(dataDir, "ast_graph.json");
            
            await fs.writeFile(graphPath, JSON.stringify(astGraph, null, 2), "utf-8");
            logger.info(`[GitNexusIndexer] Đã lưu Hierarchical AST Graph tại: ${graphPath}`);

        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[GitNexusIndexer] Thất bại: ${errMsg}`);
        } finally {
            this.indexing = false;
        }
    }

    public dispose() {
        if (this.debounceTimer) {
            clearTimeout(this.debounceTimer);
            this.debounceTimer = null;
        }
    }
}
