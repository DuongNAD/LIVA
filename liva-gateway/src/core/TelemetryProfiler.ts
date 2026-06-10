import { safeRename } from '../utils/FileUtils';
import { performance, PerformanceObserver } from 'node:perf_hooks';
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import * as os from 'node:os';
import { logger } from "../utils/logger";
import { VramCostEstimator, TaskType } from './VramCostEstimator';

export type AgentLoopState = 'IDLE' | 'THINKING' | 'SWAPPING' | 'CONSOLIDATION' | 'RENDERING';

export interface MemorySnapshot {
    timestamp: number;
    state: AgentLoopState;
    ram: {
        rss: number;          // Resident Set Size (Bộ nhớ thực tế Node.js chiếm dụng)
        heapTotal: number;    // Tổng dung lượng Heap được cấp phát
        heapUsed: number;     // Dung lượng Heap thực tế đang sử dụng
        external: number;     // Bộ nhớ C++ nằm ngoài Heap quản lý bởi V8
        systemTotal: number;  // Tổng RAM vật lý của hệ thống
        systemFree: number;   // RAM hệ thống còn trống
    };
    vram: {
        estimatedTotal: number; // Ước lượng tổng lượng VRAM ứng dụng chiếm dụng (MB)
        routerFootprint: number;// VRAM tĩnh ước tính cho mô hình Router (MB)
        expertFootprint: number;// VRAM tĩnh ước tính cho mô hình Expert (MB)
        avatarFootprint: number;// VRAM tĩnh ước tính cho WebGL Avatar (MB)
        limit: number;          // Giới hạn an toàn VRAM được cấu hình (MB)
    };
}

export interface ProfilerConfig {
    vramLimitMB: number;
    ramWarningThresholdBytes: number;
    logDir: string;
    sampleIntervalMS: number;
    maxHistoryLength: number;
}

/**
 * @module TelemetryProfiler
 * Cảm biến nỗi đau của LIVA. Theo dõi và ghi lại các hàm/tiến trình gây nghẽn cổ chai (> 500ms).
 */
export class TelemetryProfiler {
    // === STATIC API (Preserved for compatibility) ===
    private static logPath = path.join(process.cwd(), 'data', 'agents', 'liva_core', 'bottleneck_logs.txt');
    private static isInitialized = false;
    private static pendingLogs: string[] = [];
    private static flushTimer: ReturnType<typeof setTimeout> | null = null;

    public static initialize() {
        if (this.isInitialized) return;
        this.isInitialized = true;

        const dir = path.dirname(this.logPath);
        // Fire-and-forget async mkdir — this runs once at boot, non-blocking
        fsp.mkdir(dir, { recursive: true }).catch(() => { /* dir may already exist */ });

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
        const logMsg = `[${timestamp}] BOTTLENECK DETECTED: Task '${task}' chạy cạn kiệt tài nguyên (${Math.round(duration)}ms)`;
        this.pendingLogs.push(logMsg);

        // Debounced flush — gộp nhiều bottleneck events thành 1 lần ghi duy nhất
        if (!this.flushTimer) {
            this.flushTimer = setTimeout(async () => {
                this.flushTimer = null;
                try {
                    let existing = '';
                    try {
                        existing = await fsp.readFile(this.logPath, 'utf-8');
                    } catch {
                        // File doesn't exist yet — start fresh
                    }
                    const combined = existing + this.pendingLogs.join('\n') + '\n';
                    this.pendingLogs = [];
                    // Giữ file log không quá 5KB — cắt từ cuối (giữ log mới nhất)
                    const trimmed = combined.length > 5000 ? combined.slice(-5000) : combined;

                    // Atomic write
                    const tmpPath = `${this.logPath}.tmp`;
                    await fsp.writeFile(tmpPath, trimmed, 'utf-8');
                    await safeRename(tmpPath, this.logPath);
                } catch {
                    // Không để lỗi log làm sập hệ thống
                }
            }, 2000); // Gộp tối đa 2s trước khi flush
        }
    }

    // === INSTANTIABLE API (Milestone 4 implementation) ===
    public config: ProfilerConfig;
    public logFilePath: string;
    public activeState: AgentLoopState = 'IDLE';
    public intervalId: NodeJS.Timeout | null = null;
    public memoryHistory: MemorySnapshot[] = [];
    public isExpertLoaded: boolean = false;
    public isRouterLoaded: boolean = false;

    // Các hằng số footprint tĩnh từ VramCostEstimator
    public get ROUTER_VRAM_MB(): number {
        return VramCostEstimator.get(TaskType.LLM_ROUTER);
    }
    public get EXPERT_VRAM_MB(): number {
        return VramCostEstimator.get(TaskType.LLM_EXPERT);
    }
    public readonly AVATAR_VRAM_MB = 800;

    constructor(config: Partial<ProfilerConfig> = {}) {
        this.config = {
            vramLimitMB: config.vramLimitMB ?? 12288, // 12GB mặc định (phù hợp với cấu hình Tier 2)
            ramWarningThresholdBytes: config.ramWarningThresholdBytes ?? (os.totalmem() * 0.85), // 85% tổng RAM
            logDir: config.logDir ?? path.join(process.cwd(), 'logs', 'telemetry'),
            sampleIntervalMS: config.sampleIntervalMS ?? 5000, // 5 giây quét một lần
            maxHistoryLength: config.maxHistoryLength ?? 1000
        };
        this.logFilePath = path.join(this.config.logDir, `memory_profile_${Date.now()}.jsonl`);
    }

    /**
     * Cập nhật trạng thái hiện tại của Agent Loop
     */
    public setAgentState(state: AgentLoopState): void {
        this.activeState = state;
    }

    /**
     * Báo hiệu trạng thái nạp mô hình vào VRAM từ ModelOrchestrator
     */
    public setModelLoadingStatus(model: 'router' | 'expert', loaded: boolean): void {
        if (model === 'router') {
            this.isRouterLoaded = loaded;
        } else if (model === 'expert') {
            this.isExpertLoaded = loaded;
        }
    }

    /**
     * Khởi động bộ theo dõi ngầm
     */
    public async start(): Promise<void> {
        await fsp.mkdir(this.config.logDir, { recursive: true });
        if (this.intervalId) return;

        this.intervalId = setInterval(async () => {
            try {
                const snapshot = await this.takeSnapshot();
                this.memoryHistory.push(snapshot);
                if (this.memoryHistory.length > this.config.maxHistoryLength) {
                    this.memoryHistory.shift();
                }
                await this.writeSnapshotToDisk(snapshot);
                this.evaluateThresholds(snapshot);
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.error(`[TelemetryProfiler] Ghi nhận snapshot thất bại: ${msg}`);
            }
        }, this.config.sampleIntervalMS);

        if (this.intervalId.unref) {
            this.intervalId.unref(); // Tránh nghẽn tiến trình khi tắt ứng dụng
        }
    }

    /**
     * Dừng bộ theo dõi
     */
    public stop(): void {
        if (this.intervalId) {
            clearInterval(this.intervalId);
            this.intervalId = null;
        }
    }

    /**
     * Trích xuất thông tin bộ nhớ tại thời điểm gọi
     */
    public async takeSnapshot(): Promise<MemorySnapshot> {
        const memoryUsage = process.memoryUsage();
        
        let estimatedVram = 0;
        let routerFootprint = 0;
        let expertFootprint = 0;
        let avatarFootprint = 0;

        if (this.isRouterLoaded) {
            routerFootprint = this.ROUTER_VRAM_MB;
            estimatedVram += this.ROUTER_VRAM_MB;
        }
        if (this.isExpertLoaded) {
            expertFootprint = this.EXPERT_VRAM_MB;
            estimatedVram += this.EXPERT_VRAM_MB;
        }
        if (this.activeState === 'RENDERING') {
            avatarFootprint = this.AVATAR_VRAM_MB;
            estimatedVram += this.AVATAR_VRAM_MB;
        }

        return {
            timestamp: Date.now(),
            state: this.activeState,
            ram: {
                rss: memoryUsage.rss,
                heapTotal: memoryUsage.heapTotal,
                heapUsed: memoryUsage.heapUsed,
                external: memoryUsage.external,
                systemTotal: os.totalmem(),
                systemFree: os.freemem()
            },
            vram: {
                estimatedTotal: estimatedVram,
                routerFootprint,
                expertFootprint,
                avatarFootprint,
                limit: this.config.vramLimitMB
            }
        };
    }

    /**
     * Đánh giá các ngưỡng cảnh báo tài nguyên
     */
    public evaluateThresholds(snapshot: MemorySnapshot): void {
        // Cảnh báo vượt ngưỡng RAM Node.js
        if (snapshot.ram.rss > this.config.ramWarningThresholdBytes) {
            const usagePercent = ((snapshot.ram.rss / snapshot.ram.systemTotal) * 100).toFixed(1);
            logger.warn(
                `[TelemetryProfiler] ⚠️ CẢNH BÁO BỘ NHỚ RAM: Bộ nhớ RSS Node.js (${(snapshot.ram.rss / 1024 / 1024).toFixed(1)} MB) ` +
                `vượt ngưỡng an toàn. Chiếm dụng hệ thống: ${usagePercent}%. Trạng thái Agent: ${snapshot.state}`
            );
        }

        // Cảnh báo quá tải VRAM ước tính
        if (snapshot.vram.estimatedTotal > snapshot.vram.limit) {
            logger.warn(
                `[TelemetryProfiler] ⚠️ CẢNH BÁO VRAM: Lượng VRAM ước tính (${snapshot.vram.estimatedTotal} MB) ` +
                `vượt quá giới hạn cấu hình (${snapshot.vram.limit} MB). Nguy cơ gây lỗi OOM GPU.`
            );
        }
    }

    /**
     * Ghi thông tin snapshot xuống ổ đĩa định dạng JSON Lines (.jsonl)
     */
    public async writeSnapshotToDisk(snapshot: MemorySnapshot): Promise<void> {
        const line = JSON.stringify(snapshot) + '\n';
        await fsp.appendFile(this.logFilePath, line, 'utf-8');
    }

    /**
     * Xuất báo cáo tổng kết lịch sử đo đạc
     */
    public getHistoryReport(): { totalSamples: number; stateStats: Record<string, number>; maxRamBytes: number; maxVramMB: number } {
        let maxRam = 0;
        let maxVram = 0;
        const stateCounts: Record<string, number> = {};

        for (const snap of this.memoryHistory) {
            if (snap.ram.rss > maxRam) maxRam = snap.ram.rss;
            if (snap.vram.estimatedTotal > maxVram) maxVram = snap.vram.estimatedTotal;
            stateCounts[snap.state] = (stateCounts[snap.state] || 0) + 1;
        }

        return {
            totalSamples: this.memoryHistory.length,
            stateStats: stateCounts,
            maxRamBytes: maxRam,
            maxVramMB: maxVram
        };
    }
}
