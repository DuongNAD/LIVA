import * as ts from "typescript";
import * as fs from "node:fs";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { logger } from "./logger";

const targetDir = path.join(process.cwd(), "src");
const mapFile = path.join(process.cwd(), "brain_map.txt");
const cacheFile = path.join(process.cwd(), "brain_map.cache.json");

// Define cache structure
interface FileCache {
    [filePath: string]: {
        mtimeMs: number;
        skeletonText: string;
    }
}

let cache: FileCache = {};

try {
    await fsp.access(cacheFile);
    {
        cache = JSON.parse(await fsp.readFile(cacheFile, "utf-8"));
    }
} catch {
    logger.warn("[AST Cache] Cache corrupted. Rebuilding from scratch.");
}

let resultStr = "";
let processedFiles = 0;
let cacheHits = 0;

export async function extractSkeleton(filePath: string) {
    if (!filePath.endsWith(".ts")) return "";
    
    const stat = await fsp.stat(filePath);
    const mtimeMs = stat.mtimeMs;

    // Incremental Caching Check
    if (cache[filePath] && cache[filePath].mtimeMs === mtimeMs) {
        cacheHits++;
        resultStr += cache[filePath].skeletonText + "\n";
        processedFiles++;
        return cache[filePath].skeletonText;
    }

    const content = await fsp.readFile(filePath, "utf-8");
    const totalLines = content.split(/\r\n|\r|\n/).length;
    const sourceFile = ts.createSourceFile(filePath, content, ts.ScriptTarget.Latest, true);

    let fileBody = "";
    
    function extractJSDocAndComments(node: ts.Node) {
        const fullText = sourceFile.getFullText();
        const ranges = ts.getLeadingCommentRanges(fullText, node.pos);
        if (ranges) {
            for (const range of ranges) {
                const comment = fullText.substring(range.pos, range.end);
                // Vắt cạn JSDoc: Giữ Summary, ném rác @param đi
                if (comment.startsWith("/**")) {
                    const cleaned = comment.split('\n').filter(l => !l.includes("* @")).join('\n');
                    fileBody += cleaned + "\n";
                }
            }
        }
    }

    function visit(node: ts.Node) {
        // Dependency Trace
        if (ts.isImportDeclaration(node)) {
            const importText = node.getText(sourceFile);
            // Phép Khử Băng Kép: Chỉ giữ Import chéo nội bộ
            if (importText.includes(" from './") || importText.includes(" from '../")) {
                fileBody += importText + "\n";
            }
        } 
        else if (ts.isClassDeclaration(node) && node.name) {
            extractJSDocAndComments(node);
            let classSig = `export class ${node.name.text} {\n`;
            node.members.forEach(m => {
                extractJSDocAndComments(m);
                if (ts.isMethodDeclaration(m) || ts.isPropertyDeclaration(m)) {
                    const text = m.getText(sourceFile);
                    const bodyPos = text.indexOf('{');
                    if (bodyPos !== -1) {
                        classSig += "  " + text.substring(0, bodyPos).trim() + ";\n";
                    } else {
                        classSig += "  " + text + "\n";
                    }
                }
            });
            classSig += "}\n";
            fileBody += classSig;
        } 
        else if (ts.isFunctionDeclaration(node) && node.name) {
            extractJSDocAndComments(node);
            const text = node.getText(sourceFile);
            const bodyPos = text.indexOf('{');
            if (bodyPos !== -1) {
                fileBody += "export " + text.substring(0, bodyPos).trim() + ";\n";
            } else {
                fileBody += text + "\n";
            }
        } 
        else if (ts.isVariableStatement(node) && node.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword)) {
            extractJSDocAndComments(node);
            node.declarationList.declarations.forEach(d => {
               if (ts.isVariableDeclaration(d) && d.initializer && ts.isArrowFunction(d.initializer)) {
                    const text = d.initializer.getText(sourceFile);
                    const bodyPos = text.indexOf('{');
                    const sig = bodyPos !== -1 ? text.substring(0, bodyPos).trim() : text;
                    fileBody += `export const ${d.name.getText(sourceFile)} = ${sig} /*...*/;\n`;
               } else {
                    fileBody += `export const ${d.name.getText(sourceFile)};\n`;
               }
            });
        } 
        else if (ts.isInterfaceDeclaration(node) || ts.isTypeAliasDeclaration(node)) {
            extractJSDocAndComments(node);
            fileBody += node.getText(sourceFile) + "\n";
        }

        ts.forEachChild(node, visit);
    }
    visit(sourceFile);
    
    let skeletonText = "";
    if (fileBody.trim().length > 0) {
        // Build the metadata header inside the skeleton text
        const relativePath = path.relative(process.cwd(), filePath).replaceAll('\\', '/');
        // Ép Trọng Cú Pháp (Syntax Minification)
        const minifiedBody = fileBody.replaceAll(/^\s+/gm, '').replaceAll(/\n{2,}/g, '\n');
        skeletonText = `\n--- [File: ${relativePath} (Total Lines: ${totalLines})] ---\n${minifiedBody}`;
        resultStr += skeletonText + "\n";
    }

    // Save to Cache Map
    cache[filePath] = {
        mtimeMs,
        skeletonText
    };

    processedFiles++;
    return skeletonText;
}

async function scan(dir: string) {
    const files = await fsp.readdir(dir);
    for (const f of files) {
        const full = path.join(dir, f);
        if ((await fsp.stat(full)).isDirectory()) {
            if (f !== "node_modules" && f !== "dist") await scan(full);
        } else {
            // Lưới tàng hình (Bỏ qua file Rác/Test)
            if (f.startsWith("test_") || f.endsWith(".d.ts") || f.endsWith(".json")) continue;
            await extractSkeleton(full);
        }
    }
}

logger.info("[AST Skeleton] Bắt đầu gọt dũa hệ thống...");
const startTime = performance.now();

await scan(targetDir);

// Save generated map and cache
await fsp.writeFile(mapFile, resultStr, "utf-8");
await fsp.writeFile(cacheFile, JSON.stringify(cache, null, 2), "utf-8");

const endTime = performance.now();
logger.info(`[AST Skeleton] Hoành thành! Tổng cộng: ${processedFiles} files. (Cache hits: ${cacheHits})`);
logger.info(`[AST Skeleton] Kích cỡ MAP: ${Math.round(resultStr.length / 1024)}KB (~${Math.floor(resultStr.length / 3.8)} Tokens).`);
logger.info(`[AST Skeleton] Tốc độ xử lý: ${(endTime - startTime).toFixed(2)}ms`);
