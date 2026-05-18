import { parse, Lang } from "@ast-grep/napi";
import { promises as fs } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";

export interface ASTNodeData {
    type: "repository" | "file" | "class" | "function" | "method";
    name: string;
    filePath?: string;
    codeSnippet?: string;
    startLine?: number;
    endLine?: number;
    children: ASTNodeData[];
    calls?: string[]; // Names of functions this node calls
}

export class ASTGraphBuilder {
    private basePath: string;

    constructor(basePath: string) {
        this.basePath = basePath;
    }

    /**
     * Quét toàn bộ thư mục và xây dựng Hierarchical Graph
     */
    public async buildGraph(targetDir?: string): Promise<ASTNodeData> {
        const rootDir = targetDir || this.basePath;
        logger.info(`[GraphRAG] Đang xây dựng AST Graph cho: ${rootDir}`);

        const repoNode: ASTNodeData = {
            type: "repository",
            name: path.basename(rootDir),
            children: []
        };

        const files = await this.getAllFiles(rootDir);
        for (const file of files) {
            if (file.endsWith(".ts") || file.endsWith(".js") || file.endsWith(".tsx") || file.endsWith(".jsx")) {
                try {
                    const fileNode = await this.parseFile(file);
                    if (fileNode) {
                        repoNode.children.push(fileNode);
                    }
                } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e);
                    logger.warn(`[GraphRAG] Không thể parse file ${file}: ${errMsg}`);
                }
            }
        }

        logger.info(`[GraphRAG] AST Graph hoàn tất: ${repoNode.children.length} files parsed.`);
        return repoNode;
    }

    /**
     * Đệ quy tìm tất cả file
     */
    private async getAllFiles(dir: string, fileList: string[] = []): Promise<string[]> {
        const files = await fs.readdir(dir, { withFileTypes: true });
        for (const file of files) {
            const res = path.resolve(dir, file.name);
            // Bỏ qua node_modules, dist, artifacts
            if (file.isDirectory()) {
                if (file.name !== "node_modules" && file.name !== "dist" && file.name !== "artifacts" && file.name !== ".git") {
                    await this.getAllFiles(res, fileList);
                }
            } else {
                fileList.push(res);
            }
        }
        return fileList;
    }

    /**
     * Dùng ast-grep/napi để bóc tách Function và Class
     */
    private async parseFile(filePath: string): Promise<ASTNodeData | null> {
        const sourceCode = await fs.readFile(filePath, "utf-8");
        const relativePath = path.relative(this.basePath, filePath);
        
        // Chọn ngôn ngữ parse
        const language = (filePath.endsWith(".ts") || filePath.endsWith(".tsx")) ? Lang.TypeScript : Lang.JavaScript;
        const ast = parse(language, sourceCode);
        const root = ast.root();

        const fileNode: ASTNodeData = {
            type: "file",
            name: relativePath,
            filePath: relativePath,
            children: []
        };

        // Tìm các function declarations
        const functions = root.findAll({
            rule: { kind: "function_declaration" }
        });

        for (const func of functions) {
            const nameNode = func.find({ rule: { kind: "identifier" } });
            const name = nameNode ? nameNode.text() : "anonymous_func";
            const range = func.range();

            // Tìm các lệnh gọi hàm (Call Graph) bên trong hàm này
            const calls = func.findAll({ rule: { kind: "call_expression" } })
                            .map(c => c.find({ rule: { kind: "identifier" } })?.text())
                            .filter(Boolean) as string[];

            fileNode.children.push({
                type: "function",
                name: name,
                filePath: relativePath,
                codeSnippet: func.text(),
                startLine: range.start.line + 1,
                endLine: range.end.line + 1,
                calls: [...new Set(calls)], // Deduplicate
                children: []
            });
        }

        // Tìm các class declarations
        const classes = root.findAll({
            rule: { kind: "class_declaration" }
        });

        for (const cls of classes) {
            const nameNode = cls.find({ rule: { kind: "identifier" } });
            const name = nameNode ? nameNode.text() : "anonymous_class";
            const range = cls.range();

            const classNode: ASTNodeData = {
                type: "class",
                name: name,
                filePath: relativePath,
                codeSnippet: cls.text(),
                startLine: range.start.line + 1,
                endLine: range.end.line + 1,
                children: []
            };

            // Tìm các methods trong class
            const methods = cls.findAll({ rule: { kind: "method_definition" } });
            for (const method of methods) {
                const methodNameNode = method.find({ rule: { kind: "property_identifier" } });
                const methodName = methodNameNode ? methodNameNode.text() : "anonymous_method";
                const mRange = method.range();
                
                const calls = method.findAll({ rule: { kind: "call_expression" } })
                            .map(c => c.find({ rule: { kind: "property_identifier" } })?.text() || c.find({ rule: { kind: "identifier" } })?.text())
                            .filter(Boolean) as string[];

                classNode.children.push({
                    type: "method",
                    name: methodName,
                    filePath: relativePath,
                    codeSnippet: method.text(),
                    startLine: mRange.start.line + 1,
                    endLine: mRange.end.line + 1,
                    calls: [...new Set(calls)],
                    children: []
                });
            }

            fileNode.children.push(classNode);
        }

        // Nếu file trống, không có hàm/class nào thì bỏ qua để tiết kiệm node
        if (fileNode.children.length === 0) {
            return null;
        }

        return fileNode;
    }
}
