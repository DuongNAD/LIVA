import { z } from "zod";
import { logger } from "@utils/logger";
import * as chokidar from "chokidar";
import * as fs from "node:fs";
import * as path from "node:path";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

// ── Zod Schema ──────────────────────────────────────────────────────────────
const LiveErrorWardenSchema = z.object({
    action: z.enum(["watch", "unwatch", "list"]).describe("Hành động: theo dõi, ngừng theo dõi, hoặc liệt kê"),
    filePath: z.string().optional().describe("Đường dẫn tuyệt đối tới file log cần theo dõi"),
    patterns: z.array(z.string()).optional().describe("Danh sách regex pattern lỗi tuỳ chỉnh"),
});

// ── Metadata ────────────────────────────────────────────────────────────────
export const metadata = {
    name: "live_error_warden",
    description: "[AUTO_RUN] Real-time log file monitor (tail -f). Watches log files for Exception, OOM, FATAL errors and alerts immediately with analysis and suggested fixes.",
    kit: "DEVOPS_KIT",
    search_keywords: ["log", "monitor", "watch", "error", "exception", "crash", "tail", "theo dõi", "lỗi"],
    parameters: {
        type: "object",
        properties: {
            action: { type: "string", enum: ["watch", "unwatch", "list"] },
            filePath: { type: "string", description: "Absolute path to log file" },
            patterns: {
                type: "array",
                items: { type: "string" },
                description: "Custom error regex patterns",
            },
        },
        required: ["action"],
    },
};

// ── Default error patterns ──────────────────────────────────────────────────
const DEFAULT_PATTERNS: RegExp[] = [
    /Exception/i,
    /Error:/i,
    /FATAL/i,
    /\bOOM\b/i,
    /OutOfMemory/i,
    /ECONNREFUSED/i,
    /SIGKILL/i,
    /Segmentation fault/i,
    /Stack trace/i,
    /Unhandled/i,
    /ENOSPC/i,
    /EPERM/i,
    /ENOMEM/i,
];

// ── Fix suggestion map ─────────────────────────────────────────────────────
const FIX_SUGGESTIONS: Array<{ pattern: RegExp; suggestion: string }> = [
    { pattern: /ECONNREFUSED/i, suggestion: "Kiểm tra service đích có đang chạy không. Xác nhận host:port đúng." },
    { pattern: /\bOOM\b|OutOfMemory/i, suggestion: "Tăng heap size (--max-old-space-size) hoặc kiểm tra memory leak." },
    { pattern: /ENOSPC/i, suggestion: "Ổ đĩa đầy. Giải phóng dung lượng hoặc tăng disk." },
    { pattern: /EPERM/i, suggestion: "Thiếu quyền truy cập. Kiểm tra permission file/folder." },
    { pattern: /ENOMEM/i, suggestion: "Hệ thống hết RAM. Tắt bớt ứng dụng hoặc tăng RAM/swap." },
    { pattern: /Segmentation fault/i, suggestion: "Lỗi bộ nhớ native. Kiểm tra native addon hoặc C/C++ binding." },
    { pattern: /SIGKILL/i, suggestion: "Process bị kill bởi OS (thường do OOM killer). Kiểm tra memory usage." },
    { pattern: /Unhandled.*rejection|UnhandledPromiseRejection/i, suggestion: "Promise không có .catch(). Thêm error handler cho async code." },
    { pattern: /Stack trace|TypeError|ReferenceError/i, suggestion: "Bug code. Đọc stack trace để xác định file và dòng lỗi." },
    { pattern: /FATAL/i, suggestion: "Lỗi nghiêm trọng. Khởi động lại service và kiểm tra config." },
    { pattern: /Exception/i, suggestion: "Exception xảy ra. Kiểm tra chi tiết stack trace bên dưới." },
    { pattern: /Error:/i, suggestion: "Kiểm tra log context xung quanh để xác định nguyên nhân gốc." },
];

// ── Watcher info type ───────────────────────────────────────────────────────
interface WatcherInfo {
    watcher: chokidar.FSWatcher;
    patterns: RegExp[];
    errorsDetected: number;
    lastError: string | null;
    lastErrorTime: number | null;
}

// ── LogWatcherRegistry Singleton ────────────────────────────────────────────
class LogWatcherRegistry {
    #watchers: Map<string, WatcherInfo> = new Map();
    #lastPositions: Map<string, number> = new Map();

    /** Bắt đầu theo dõi file log */
    async watch(filePath: string, customPatterns?: string[]): Promise<string> {
        const resolved = path.resolve(filePath);

        // Kiểm tra đã watch chưa
        if (this.#watchers.has(resolved)) {
            return `[WARDEN INFO] File "${path.basename(resolved)}" đã đang được theo dõi.`;
        }

        // Kiểm tra file tồn tại
        try {
            fs.accessSync(resolved, fs.constants.R_OK);
        } catch {
            return `[WARDEN ERROR] Không tìm thấy hoặc không đọc được file: ${resolved}`;
        }

        // Lấy file size hiện tại làm vị trí bắt đầu (chỉ đọc nội dung mới)
        const stat = fs.statSync(resolved);
        this.#lastPositions.set(resolved, stat.size);

        // Build pattern list
        const patterns = [...DEFAULT_PATTERNS];
        if (customPatterns && customPatterns.length > 0) {
            for (const p of customPatterns) {
                try {
                    patterns.push(new RegExp(p, "i"));
                } catch {
                    logger.warn(`[LiveErrorWarden] Regex pattern không hợp lệ: "${p}" — bỏ qua.`);
                }
            }
        }

        // Tạo chokidar watcher
        const watcher = chokidar.watch(resolved, {
            persistent: true,
            usePolling: false,
            awaitWriteFinish: { stabilityThreshold: 200, pollInterval: 100 },
        });

        const watcherInfo: WatcherInfo = {
            watcher,
            patterns,
            errorsDetected: 0,
            lastError: null,
            lastErrorTime: null,
        };

        watcher.on("change", () => {
            this.#onFileChange(resolved, watcherInfo);
        });

        watcher.on("error", (err: unknown) => {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.error(`[LiveErrorWarden] Lỗi watcher cho ${path.basename(resolved)}: ${errMsg}`);
        });

        this.#watchers.set(resolved, watcherInfo);

        logger.info(`[LiveErrorWarden] ✅ Bắt đầu theo dõi: ${resolved} (${patterns.length} patterns)`);
        return `[WARDEN SUCCESS] Đang theo dõi file: ${resolved}\n- Patterns: ${patterns.length} quy tắc\n- Vị trí bắt đầu: byte ${stat.size}`;
    }

    /** Xử lý khi file thay đổi */
    #onFileChange(filePath: string, info: WatcherInfo): void {
        const lastPos = this.#lastPositions.get(filePath) ?? 0;

        let currentSize: number;
        try {
            const stat = fs.statSync(filePath);
            currentSize = stat.size;
        } catch {
            return; // File bị xoá hoặc không đọc được
        }

        // File bị truncate (log rotation)
        if (currentSize < lastPos) {
            this.#lastPositions.set(filePath, 0);
            return;
        }

        // Không có dữ liệu mới
        if (currentSize === lastPos) return;

        // Đọc dữ liệu mới
        const stream = fs.createReadStream(filePath, {
            start: lastPos,
            end: currentSize - 1,
            encoding: "utf-8",
        });

        let newData = "";
        stream.on("data", (chunk: string) => {
            newData += chunk;
        });

        stream.on("end", () => {
            this.#lastPositions.set(filePath, currentSize);
            this.#analyzeNewLines(filePath, newData, info);
        });

        stream.on("error", (err) => {
            logger.error(`[LiveErrorWarden] Lỗi đọc dữ liệu mới từ ${path.basename(filePath)}: ${err.message}`);
        });
    }

    /** Phân tích dòng mới tìm lỗi */
    #analyzeNewLines(filePath: string, data: string, info: WatcherInfo): void {
        const lines = data.split(/\r?\n/);
        const fileName = path.basename(filePath);

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];
            if (!line.trim()) continue;

            for (const pattern of info.patterns) {
                if (pattern.test(line)) {
                    info.errorsDetected++;
                    info.lastError = line.substring(0, 200);
                    info.lastErrorTime = Date.now();

                    // Trích xuất context (2 dòng trước + 2 dòng sau)
                    const contextStart = Math.max(0, i - 2);
                    const contextEnd = Math.min(lines.length - 1, i + 2);
                    const contextLines = lines.slice(contextStart, contextEnd + 1);
                    const context = contextLines.join("\n");

                    // Tìm gợi ý sửa lỗi
                    const suggestion = this.#getSuggestion(line);

                    // Copy lỗi + gợi ý vào clipboard
                    const clipboardContent = `[${fileName}] Error:\n${context}\n\nSuggestion: ${suggestion}`;
                    this.#copyToClipboard(clipboardContent);

                    // Push notification qua IPC
                    const notification = JSON.stringify({
                        event: "SHOW_TOAST",
                        payload: {
                            title: `🚨 ${fileName} phát hiện lỗi`,
                            message: `${line.substring(0, 100)}...\n💡 ${suggestion}`,
                            type: "error",
                            duration: 10000,
                        },
                    });
                    process.stdout.write(notification + "\n");

                    // Log chi tiết
                    logger.warn(
                        `[LiveErrorWarden] 🚨 ${fileName} lỗi dòng ~${i}: ${line.substring(0, 150)}\n💡 Gợi ý: ${suggestion}`,
                    );

                    // Chỉ alert lỗi đầu tiên trong mỗi batch để tránh spam
                    return;
                }
            }
        }
    }

    /** Tìm gợi ý sửa lỗi dựa trên pattern matching */
    #getSuggestion(line: string): string {
        for (const { pattern, suggestion } of FIX_SUGGESTIONS) {
            if (pattern.test(line)) {
                return suggestion;
            }
        }
        return "Kiểm tra log context xung quanh để xác định nguyên nhân. Copy đã sẵn trong clipboard.";
    }

    /** Copy nội dung vào clipboard qua PowerShell */
    #copyToClipboard(text: string): void {
        // Escape đặc biệt cho PowerShell
        const safe = text.replace(/'/g, "''").replace(/\r?\n/g, "`n");
        execAsync(`powershell.exe -NoProfile -Command "Set-Clipboard -Value '${safe}'"`, { timeout: 5000 })
            .catch((err) => {
                logger.warn(`[LiveErrorWarden] Không copy được vào clipboard: ${err.message}`);
            });
    }

    /** Ngừng theo dõi file */
    async unwatch(filePath: string): Promise<string> {
        const resolved = path.resolve(filePath);
        const info = this.#watchers.get(resolved);

        if (!info) {
            return `[WARDEN ERROR] File "${resolved}" không đang được theo dõi.`;
        }

        await info.watcher.close();
        this.#watchers.delete(resolved);
        this.#lastPositions.delete(resolved);

        logger.info(`[LiveErrorWarden] Đã ngừng theo dõi: ${resolved}`);
        return `[WARDEN SUCCESS] Đã ngừng theo dõi: ${resolved}\n- Tổng lỗi phát hiện: ${info.errorsDetected}`;
    }

    /** Liệt kê tất cả file đang theo dõi */
    list(): string {
        if (this.#watchers.size === 0) {
            return "[WARDEN STATUS] Không có file nào đang được theo dõi.";
        }

        let output = `[WARDEN STATUS] Đang theo dõi ${this.#watchers.size} file:\n\n`;

        for (const [filePath, info] of this.#watchers) {
            const lastErr = info.lastError
                ? `"${info.lastError.substring(0, 80)}..." (${info.lastErrorTime ? new Date(info.lastErrorTime).toLocaleTimeString("vi-VN") : "N/A"})`
                : "Chưa có lỗi";

            output += `📄 ${path.basename(filePath)}\n`;
            output += `   Path: ${filePath}\n`;
            output += `   Patterns: ${info.patterns.length}\n`;
            output += `   Lỗi phát hiện: ${info.errorsDetected}\n`;
            output += `   Lỗi gần nhất: ${lastErr}\n\n`;
        }

        return output;
    }

    /** Cleanup tất cả watcher */
    async dispose(): Promise<void> {
        for (const [filePath, info] of this.#watchers) {
            await info.watcher.close();
            logger.info(`[LiveErrorWarden] Đã đóng watcher: ${filePath}`);
        }
        this.#watchers.clear();
        this.#lastPositions.clear();
    }
}

// ── Singleton instance ──────────────────────────────────────────────────────
export const logWatcherRegistry = new LogWatcherRegistry();

// ── Execute function ────────────────────────────────────────────────────────
export const execute = async (argsObj: unknown): Promise<string> => {
    try {
        const parsed = LiveErrorWardenSchema.parse(argsObj);

        switch (parsed.action) {
            case "watch": {
                if (!parsed.filePath) {
                    return "[WARDEN ERROR] Cần cung cấp 'filePath' để bắt đầu theo dõi.";
                }
                return await logWatcherRegistry.watch(parsed.filePath, parsed.patterns);
            }
            case "unwatch": {
                if (!parsed.filePath) {
                    return "[WARDEN ERROR] Cần cung cấp 'filePath' để ngừng theo dõi.";
                }
                return await logWatcherRegistry.unwatch(parsed.filePath);
            }
            case "list":
                return logWatcherRegistry.list();
            default:
                return "[WARDEN ERROR] Hành động không hợp lệ.";
        }
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[LiveErrorWarden] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[WARDEN ERROR] Sai định dạng: ${error.issues.map((e) => e.message).join(", ")}`;
        }
        return `[WARDEN ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
