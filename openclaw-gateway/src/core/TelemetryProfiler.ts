import { performance, PerformanceObserver } from 'node:perf_hooks';
import * as fs from "node:fs";
import { promises as fsp } from "node:fs";
import * as path from "node:path";

/**
 * @module TelemetryProfiler
 * Cảm biến nỗi đau của LIVA. Theo dõi và ghi lại các hàm/tiến trình gây nghẽn cổ chai (> 500ms).
 */
export class TelemetryProfiler {
    private static logPath = path.join(process.cwd(), 'data', 'agents', 'liva_core', 'bottleneck_logs.txt');
    private static isInitialized = false;

    public static initialize() {
        if (this.isInitialized) return;
        this.isInitialized = true;

        const dir = path.dirname(this.logPath);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }

        // Bật PerformanceObserver để bắt các node bị nghẽn
        const obs = new PerformanceObserver((items) => {
            const entries = items.getEntries();
            for (const entry of entries) {
                // Nếu vượt quá 500ms => Nghẽn cổ chai lớn
                if (entry.duration > 500) {
                    this.logBottleneck(entry.name, entry.duration);
                }
            }
        });
        obs.observe({ entryTypes: ['measure'], buffered: true });
    }

    /**
     * Bọc một hàm bất đồng bộ để đo lường.
     */
    public static async track<T>(taskName: string, fn: () => Promise<T>): Promise<T> {
        this.initialize();
        const startMark = `${taskName}_start`;
        const endMark = `${taskName}_end`;
        
        performance.mark(startMark);
        try {
            return await fn();
        } finally {
            performance.mark(endMark);
            performance.measure(taskName, startMark, endMark);
        }
    }

    private static pendingLogs: string[] = [];
    private static flushTimer: ReturnType<typeof setTimeout> | null = null;

    private static logBottleneck(task: string, duration: number) {
        const timestamp = new Date().toISOString();
        const logMsg = `[${timestamp}] BOTTLENECK DETECTED: Task '${task}' chạy cạn kiệt tài nguyên (${Math.round(duration)}ms)`;
        this.pendingLogs.push(logMsg);

        // Debounced flush — gộp nhiều bottleneck events thành 1 lần ghi duy nhất
        if (!this.flushTimer) {
            this.flushTimer = setTimeout(async () => {
                this.flushTimer = null;
                try {
                    let existing = '';
                    if (fs.existsSync(this.logPath)) {
                        existing = await fsp.readFile(this.logPath, 'utf-8');
                    }
                    const combined = existing + this.pendingLogs.join('\n') + '\n';
                    this.pendingLogs = [];
                    // Giữ file log không quá 5KB — cắt từ cuối (giữ log mới nhất)
                    const trimmed = combined.length > 5000 ? combined.slice(-5000) : combined;
                    
                    // Atomic write
                    const tmpPath = `${this.logPath}.tmp`;
                    await fsp.writeFile(tmpPath, trimmed, 'utf-8');
                    await fsp.rename(tmpPath, this.logPath);
                } catch {
                    // Không để lỗi log làm sập hệ thống
                }
            }, 2000); // Gộp tối đa 2s trước khi flush
        }
    }
}
