import { exec } from "child_process";
import { promisify } from "util";
import { promises as fs } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";
import { ASTGraphBuilder } from "./ASTGraphBuilder";

const execAsync = promisify(exec);

export class GitNexusIndexer {
    private indexing: boolean = false;
    private debounceTimer: NodeJS.Timeout | null = null;

    public triggerIndex(delayMs: number = 5000) {
        if (this.debounceTimer) {
            clearTimeout(this.debounceTimer);
        }
        this.debounceTimer = setTimeout(() => {
            void this.runIndex();
        }, delayMs);
    }

    private async runIndex() {
        if (this.indexing) return;
        this.indexing = true;
        try {
            logger.info("[GitNexusIndexer] Đang chạy phân tích GitNexus nền...");
            const { stdout, stderr } = await execAsync("npx gitnexus analyze --embeddings");
            logger.info(`[GitNexusIndexer] Hoàn tất CLI: ${stdout}`);
            if (stderr) logger.warn(`[GitNexusIndexer] Cảnh báo CLI: ${stderr}`);

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

        } catch (e: any) {
            logger.error(`[GitNexusIndexer] Thất bại: ${e.message}`);
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
