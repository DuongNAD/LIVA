import { performance, PerformanceObserver } from 'perf_hooks';
import * as fs from 'fs';
import * as path from 'path';

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

    private static logBottleneck(task: string, duration: number) {
        const timestamp = new Date().toISOString();
        const logMsg = `[${timestamp}] BOTTLENECK DETECTED: Task '${task}' chạy cạn kiệt tài nguyên (${Math.round(duration)}ms)\n`;
        fs.appendFileSync(this.logPath, logMsg, 'utf-8');
        // Giữ file log không quá dài (cắt bớt để tránh phình to)
        const content = fs.readFileSync(this.logPath, 'utf-8');
        if (content.length > 5000) {
            fs.writeFileSync(this.logPath, content.slice(-5000), 'utf-8');
        }
    }
}
