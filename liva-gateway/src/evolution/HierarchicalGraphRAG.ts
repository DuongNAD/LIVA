import { promises as fs } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";
import { ASTNodeData } from "./ASTGraphBuilder";

/**
 * Triển khai Hierarchical Graph Routing (Mnemis & AgentForge inspired)
 * Cung cấp 2 bước truy vấn:
 * - System 1: Quét Node tổng quan (Repository/File level)
 * - System 2: Đi sâu vào chi tiết Hàm (Function/Method level) thông qua Call Graph
 */
export class HierarchicalGraphRAG {
    private graphPath: string;
    private cachedGraph: ASTNodeData | null = null;

    constructor() {
        const projectRoot = path.resolve(process.cwd(), "..");
        this.graphPath = path.join(projectRoot, "data", "ast_graph.json");
    }

    private async loadGraph(): Promise<ASTNodeData> {
        if (this.cachedGraph) return this.cachedGraph;
        try {
            const data = await fs.readFile(this.graphPath, "utf-8");
            this.cachedGraph = JSON.parse(data) as ASTNodeData;
            return this.cachedGraph;
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[GraphRAG] Lỗi đọc đồ thị: ${errMsg}`);
            throw new Error("AST Graph chưa được xây dựng. Vui lòng chờ GitNexusIndexer.");
        }
    }

    /**
     * System 1 (Global Selection): Quét tổng quan để tìm các File có chứa từ khóa
     * Hoạt động cực kỳ nhẹ và nhanh (không load code snippet)
     */
    public async system1Search(keyword: string): Promise<{ type: string; name: string; filePath?: string }[]> {
        const graph = await this.loadGraph();
        const results: { type: string; name: string; filePath?: string }[] = [];
        const lowerKeyword = keyword.toLowerCase();

        // BFS duyệt cây
        const queue: ASTNodeData[] = [graph];

        while (queue.length > 0) {
            const node = queue.shift()!;
            
            // Tìm kiếm trên tên file/class/function
            if (node.name.toLowerCase().includes(lowerKeyword)) {
                results.push({
                    type: node.type,
                    name: node.name,
                    filePath: node.filePath
                });
            }

            if (node.children) {
                queue.push(...node.children);
            }
        }

        return results;
    }

    /**
     * System 2 (Detailed Traversal): Lấy chi tiết một Function/Method và 
     * tự động móc nối (traverse) các hàm mà nó gọi (Call Graph)
     */
    public async system2DeepDive(functionName: string, depth: number = 1): Promise<any[]> {
        const graph = await this.loadGraph();
        const results: any[] = [];
        
        // 1. Tìm hàm gốc
        const rootNodes = this.findNodesByName(graph, functionName);
        if (rootNodes.length === 0) return [];

        // 2. Lấy thông tin chi tiết và đệ quy tìm các hàm được gọi
        for (const root of rootNodes) {
            const details = await this.traverseCallGraph(root, graph, depth);
            results.push(details);
        }

        return results;
    }

    private findNodesByName(root: ASTNodeData, name: string): ASTNodeData[] {
        const results: ASTNodeData[] = [];
        const queue: ASTNodeData[] = [root];

        while (queue.length > 0) {
            const node = queue.shift()!;
            if (node.name === name) {
                results.push(node);
            }
            if (node.children) {
                queue.push(...node.children);
            }
        }
        return results;
    }

    private async traverseCallGraph(node: ASTNodeData, globalGraph: ASTNodeData, depth: number): Promise<any> {
        const result: any = {
            name: node.name,
            type: node.type,
            file: node.filePath,
            code: node.codeSnippet,
            calls: []
        };

        if (depth > 0 && node.calls && node.calls.length > 0) {
            for (const callName of node.calls) {
                // Ignore obvious built-ins or very short calls
                if (callName.length < 3) continue;
                
                const calledNodes = this.findNodesByName(globalGraph, callName);
                for (const cNode of calledNodes) {
                    // Tránh vòng lặp vô hạn đệ quy
                    if (cNode.name !== node.name) {
                        const callDetails = await this.traverseCallGraph(cNode, globalGraph, depth - 1);
                        result.calls.push(callDetails);
                    }
                }
            }
        }

        return result;
    }
}
