import * as fs from "node:fs/promises";
import * as path from "node:path";
import { logger } from "../utils/logger";

export class FileExplorer {
    readonly #basePath: string;

    constructor(basePath?: string) {
        // Default to project root (up 2 levels from src/services if we're in openclaw-gateway)
        // e.g. e:\project\openclaw_remake
        this.#basePath = basePath || path.resolve(process.cwd(), "..");
    }

    /**
     * Resolves a target path and ensures it does not escape the basePath (chroot jail)
     */
    #resolveAndJail(targetPath: string): string {
        // Resolve absolute path
        const absolutePath = path.resolve(this.#basePath, targetPath.replace(/^[\/\\]/, "")); // Remove leading slash if any
        
        // Check jailbreak
        if (!absolutePath.startsWith(this.#basePath)) {
            logger.warn(`[FileExplorer] 🚨 Security Block: Path traversal attempt prevented: ${targetPath}`);
            throw new Error("Access Denied: Path is outside of allowed workspace.");
        }
        
        return absolutePath;
    }

    /**
     * List directory contents safely
     */
    public async listDirectory(dirPath: string = ""): Promise<{ name: string, isDirectory: boolean, size: number }[]> {
        const safePath = this.#resolveAndJail(dirPath);
        
        try {
            const stat = await fs.stat(safePath);
            if (!stat.isDirectory()) {
                throw new Error("Not a directory");
            }

            const files = await fs.readdir(safePath, { withFileTypes: true });
            
            const results = [];
            for (const file of files) {
                // Ignore hidden files / node_modules to keep UI clean? No, let user see them, maybe just dotfiles?
                try {
                    const filePath = path.join(safePath, file.name);
                    const fileStat = await fs.stat(filePath);
                    results.push({
                        name: file.name,
                        isDirectory: file.isDirectory(),
                        size: fileStat.size
                    });
                } catch (e) {
                    // Ignore files that can't be stat'd (permissions)
                }
            }

            // Sort directories first
            return results.sort((a, b) => {
                if (a.isDirectory && !b.isDirectory) return -1;
                if (!a.isDirectory && b.isDirectory) return 1;
                return a.name.localeCompare(b.name);
            });

        } catch (e: any) {
            logger.error(`[FileExplorer] Lỗi đọc thư mục ${dirPath}: ${e.message}`);
            throw e;
        }
    }

    /**
     * Read a text file safely
     */
    public async readFile(filePath: string): Promise<string> {
        const safePath = this.#resolveAndJail(filePath);
        
        try {
            const stat = await fs.stat(safePath);
            if (stat.isDirectory()) {
                throw new Error("Cannot read a directory as file");
            }
            if (stat.size > 5 * 1024 * 1024) { // 5MB limit
                throw new Error("File too large (limit 5MB)");
            }

            return await fs.readFile(safePath, "utf-8");
        } catch (e: any) {
            logger.error(`[FileExplorer] Lỗi đọc tệp ${filePath}: ${e.message}`);
            throw e;
        }
    }
}
