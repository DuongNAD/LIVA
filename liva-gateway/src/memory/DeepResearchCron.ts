import { promises as fsp } from "node:fs";
import path from "node:path";
import { logger } from "../utils/logger";
import { ObsidianVaultManager } from "./ObsidianVaultManager";

export class DeepResearchCron {
    readonly #vaultRoot: string;
    
    constructor(private vaultManager: ObsidianVaultManager, vaultRootPath: string) {
        this.#vaultRoot = path.resolve(vaultRootPath);
    }

    /**
     * Thuật toán quét Đồ thị tìm Nút Mồ Côi và tính Gravity
     */
    public async findTopOrphanNodes(limit: number = 3): Promise<string[]> {
        const linkCounts: Record<string, number> = {};
        const existingFiles = new Set<string>();

        // Quét tất cả các file .md trong Vault (Boilerplate đếm số lượng Wikilink)
        // LIVA sẽ đếm tần suất xuất hiện của [[Wikilink]]
        const files = await this.#getAllMarkdownFiles(this.#vaultRoot);
        
        for (const file of files) {
            existingFiles.add(path.basename(file, ".md"));
            
            const content = await fsp.readFile(file, "utf-8");
            
            // Regex lấy tất cả [[Tên Bài]] hoặc [[Bài gốc|Alias]]
            const matches = content.matchAll(/\[\[(.*?)\]\]/g);
            for (const match of matches) {
                const linkContent = match[1];
                // Lấy phần đầu nếu có Alias: [[Tên Bài|Alias]]
                const actualLink = linkContent.split('|')[0];
                linkCounts[actualLink] = (linkCounts[actualLink] || 0) + 1;
            }
        }

        // Lọc ra các Nút Mồ Côi (Chưa có file .md tương ứng)
        const orphanNodes = Object.keys(linkCounts)
            .filter(link => !existingFiles.has(link))
            .sort((a, b) => linkCounts[b] - linkCounts[a]); // Sắp xếp theo Gravity (Số lượng backlink giảm dần)

        return orphanNodes.slice(0, limit);
    }

    /**
     * Tự động tạo bản nháp sau khi Research
     */
    public async draftResearch(nodeName: string, researchContent: string): Promise<void> {
        const draftDir = path.join(this.#vaultRoot, "LIVA_Drafts");
        await fsp.mkdir(draftDir, { recursive: true });
        
        const filePath = path.join(draftDir, `${nodeName}.md`);
        
        // Append nội dung research vào Draft (HITLGuard sẽ review sau)
        await fsp.writeFile(filePath, researchContent, "utf-8");
        logger.info({ node: nodeName }, "DeepResearch: Draft created successfully.");
    }

    async #getAllMarkdownFiles(dir: string): Promise<string[]> {
        let results: string[] = [];
        const list = await fsp.readdir(dir, { withFileTypes: true });
        for (const file of list) {
            const res = path.resolve(dir, file.name);
            if (file.isDirectory()) {
                results = results.concat(await this.#getAllMarkdownFiles(res));
            } else if (file.isFile() && file.name.endsWith(".md")) {
                results.push(res);
            }
        }
        return results;
    }
}
