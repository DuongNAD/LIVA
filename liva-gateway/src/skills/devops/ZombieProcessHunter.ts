import { z } from "zod";
import { logger } from "@utils/logger";
import { HITLGuard } from "@security/HITLGuard";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

// ── Zod Schema ──────────────────────────────────────────────────────────────
const ZombieProcessSchema = z.object({
    action: z.enum(["scan", "auto_scan_start", "auto_scan_stop", "status"])
        .describe("Hành động: quét một lần, bắt đầu/dừng quét tự động, hoặc xem trạng thái"),
    memoryThresholdMB: z.number().min(512).optional().default(5120)
        .describe("Ngưỡng RAM (MB) để coi là zombie, mặc định 5120 (5GB)"),
    idleThresholdMinutes: z.number().min(5).optional().default(30)
        .describe("Thời gian idle tối thiểu (phút) để coi là zombie, mặc định 30"),
});

// ── Metadata ────────────────────────────────────────────────────────────────
export const metadata = {
    name: "zombie_process_hunter",
    description: "[ASK_FIRST] Monitors RAM/VRAM usage. Detects idle processes consuming >5GB RAM for 30+ minutes and proposes cleanup with HITL approval.",
    kit: "DEVOPS_KIT",
    requires_hitl: true,
    search_keywords: ["zombie", "process", "ram", "memory", "leak", "idle", "cleanup", "bộ nhớ", "tiến trình"],
    parameters: {
        type: "object",
        properties: {
            action: { type: "string", enum: ["scan", "auto_scan_start", "auto_scan_stop", "status"] },
            memoryThresholdMB: { type: "number", description: "RAM threshold in MB (default 5120 = 5GB)" },
            idleThresholdMinutes: { type: "number", description: "Idle time threshold in minutes (default 30)" },
        },
        required: ["action"],
    },
};

// ── Process snapshot type ───────────────────────────────────────────────────
interface ProcessSnapshot {
    firstSeen: number;
    lastCpu: number;
    cpuSamples: number[];
}

interface ZombieCandidate {
    pid: number;
    name: string;
    memMB: number;
    cpuSec: number;
    idleMinutes: number;
}

// ── Auto-scan interval constant ─────────────────────────────────────────────
const AUTO_SCAN_INTERVAL_MS = 5 * 60 * 1000; // 5 phút

// ── macOS helpers ───────────────────────────────────────────────────────────
/** Parse BSD `ps` cumulative CPU time ([DD-]HH:MM:SS / MM:SS.ss) into seconds. */
function parsePsTime(t: string): number {
    let days = 0;
    let rest = t;
    if (rest.includes("-")) {
        const [d, r] = rest.split("-");
        days = Number(d) || 0;
        rest = r;
    }
    const segs = rest.split(":").map(Number);
    let sec = 0;
    if (segs.length === 3) sec = segs[0] * 3600 + segs[1] * 60 + segs[2];
    else if (segs.length === 2) sec = segs[0] * 60 + segs[1];
    else sec = segs[0] || 0;
    return Math.round((days * 86400 + sec) * 100) / 100;
}

/** macOS: high-memory processes via `ps` (rss KB → MB), shaped like the Windows JSON. */
async function getMacHighMemProcesses(
    thresholdMB: number,
): Promise<Array<{ Id: number; ProcessName: string; MemMB: number; CPU_Sec: number }>> {
    const { stdout } = await execAsync("ps -axo pid=,rss=,time=,comm=", { timeout: 15000 });
    const out: Array<{ Id: number; ProcessName: string; MemMB: number; CPU_Sec: number }> = [];
    for (const line of stdout.split("\n")) {
        const t = line.trim();
        if (!t) continue;
        const parts = t.split(/\s+/);
        if (parts.length < 4) continue;
        const pid = Number(parts[0]);
        const rssKb = Number(parts[1]);
        if (!Number.isFinite(pid) || !Number.isFinite(rssKb)) continue;
        const memMB = Math.round(rssKb / 1024);
        if (memMB <= thresholdMB) continue;
        const comm = parts.slice(3).join(" ");
        out.push({ Id: pid, ProcessName: comm.split("/").pop() || comm, MemMB: memMB, CPU_Sec: parsePsTime(parts[2]) });
    }
    return out;
}

// ── ZombieScanner Singleton ─────────────────────────────────────────────────
class ZombieScanner {
    #autoScanInterval: NodeJS.Timeout | null = null;
    #memoryThresholdMB = 5120;
    #idleThresholdMs = 30 * 60 * 1000;
    #processHistory: Map<number, ProcessSnapshot> = new Map();
    #lastScanResults: ZombieCandidate[] = [];
    #lastScanTime: number | null = null;
    #isAutoScanning = false;

    /** Quét tiến trình tốn RAM */
    async scan(thresholdMB: number, idleThresholdMinutes: number): Promise<string> {
        this.#memoryThresholdMB = thresholdMB;
        this.#idleThresholdMs = idleThresholdMinutes * 60 * 1000;

        const thresholdBytes = thresholdMB * 1024 * 1024;

        let processes: Array<{ Id: number; ProcessName: string; MemMB: number; CPU_Sec: number }>;
        try {
            if (process.platform === "darwin") {
                processes = await getMacHighMemProcesses(thresholdMB);
            } else {
                // Windows: high-memory processes as JSON via PowerShell.
                const psScript = `
                    Get-Process | Where-Object { $_.WorkingSet64 -gt ${thresholdBytes} } |
                    Select-Object Id, ProcessName,
                        @{N='MemMB';E={[math]::Round($_.WorkingSet64/1MB)}},
                        @{N='CPU_Sec';E={[math]::Round($_.CPU, 2)}} |
                    ConvertTo-Json -Compress
                `.trim().replace(/\n\s*/g, " ");
                const { stdout } = await execAsync(
                    `powershell.exe -NoProfile -Command "${psScript}"`,
                    { timeout: 15000 },
                );
                const trimmed = stdout.trim();
                if (!trimmed) {
                    processes = [];
                } else {
                    const parsed = JSON.parse(trimmed);
                    processes = Array.isArray(parsed) ? parsed : [parsed];
                }
            }
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            logger.error(`[ZombieHunter] Lỗi quét tiến trình: ${msg}`);
            return `[ZOMBIE ERROR] Không thể quét tiến trình: ${msg}`;
        }

        if (processes.length === 0) {
            this.#lastScanResults = [];
            this.#lastScanTime = Date.now();
            return `[ZOMBIE SCAN] Không tìm thấy tiến trình nào sử dụng >${thresholdMB} MB RAM. Hệ thống sạch.`;
        }

        const now = Date.now();
        const zombies: ZombieCandidate[] = [];

        for (const proc of processes) {
            const pid = proc.Id;
            const cpuSec = proc.CPU_Sec ?? 0;
            const existing = this.#processHistory.get(pid);

            if (!existing) {
                // Lần đầu thấy → ghi nhận
                this.#processHistory.set(pid, {
                    firstSeen: now,
                    lastCpu: cpuSec,
                    cpuSamples: [cpuSec],
                });
                continue;
            }

            // Cập nhật CPU sample
            existing.cpuSamples.push(cpuSec);
            if (existing.cpuSamples.length > 20) {
                existing.cpuSamples.shift(); // Giữ tối đa 20 mẫu
            }

            // Tính CPU delta — nếu CPU gần như không tăng → idle
            const cpuDelta = cpuSec - existing.lastCpu;
            existing.lastCpu = cpuSec;

            const timeSinceFirstSeen = now - existing.firstSeen;
            const isIdle = cpuDelta < 1.0; // Dưới 1 giây CPU trong khoảng quét → idle

            if (timeSinceFirstSeen >= this.#idleThresholdMs && isIdle) {
                zombies.push({
                    pid,
                    name: proc.ProcessName,
                    memMB: proc.MemMB,
                    cpuSec,
                    idleMinutes: Math.round(timeSinceFirstSeen / 60000),
                });
            }
        }

        // Dọn dẹp process history — xóa PID không còn trong danh sách
        const activePids = new Set(processes.map((p) => p.Id));
        for (const pid of this.#processHistory.keys()) {
            if (!activePids.has(pid)) {
                this.#processHistory.delete(pid);
            }
        }

        this.#lastScanResults = zombies;
        this.#lastScanTime = now;

        if (zombies.length === 0) {
            // Vẫn liệt kê high-memory processes
            let output = `[ZOMBIE SCAN] Không phát hiện zombie. ${processes.length} tiến trình >${thresholdMB}MB:\n\n`;
            output += `| # | Tên | PID | RAM (MB) | CPU (s) |\n`;
            output += `|---|-----|-----|----------|---------|\n`;
            for (let i = 0; i < Math.min(processes.length, 15); i++) {
                const p = processes[i];
                output += `| ${i + 1} | ${p.ProcessName} | ${p.Id} | ${p.MemMB} | ${p.CPU_Sec ?? 0} |\n`;
            }
            return output;
        }

        // Có zombie!
        let output = `[ZOMBIE SCAN] 🧟 Phát hiện ${zombies.length} tiến trình zombie:\n\n`;
        output += `| # | Tên | PID | RAM (MB) | Idle (phút) | CPU (s) |\n`;
        output += `|---|-----|-----|----------|-------------|---------|\n`;
        for (let i = 0; i < zombies.length; i++) {
            const z = zombies[i];
            output += `| ${i + 1} | ${z.name} | ${z.pid} | ${z.memMB} | ${z.idleMinutes} | ${z.cpuSec} |\n`;
        }
        output += `\n💡 Dùng process_manager hoặc yêu cầu LIVA kill PID cụ thể.`;

        logger.info(`[ZombieHunter] Phát hiện ${zombies.length} zombie.`);
        return output;
    }

    /** Bắt đầu quét tự động */
    async autoScanStart(thresholdMB: number, idleThresholdMinutes: number): Promise<string> {
        if (this.#autoScanInterval) {
            return "[ZOMBIE INFO] Auto-scan đã đang chạy. Dùng 'auto_scan_stop' để dừng trước khi bật lại.";
        }

        this.#memoryThresholdMB = thresholdMB;
        this.#idleThresholdMs = idleThresholdMinutes * 60 * 1000;
        this.#isAutoScanning = true;

        // Quét lần đầu ngay
        await this.scan(thresholdMB, idleThresholdMinutes);

        this.#autoScanInterval = setInterval(async () => {
            try {
                const result = await this.scan(thresholdMB, idleThresholdMinutes);

                // Nếu phát hiện zombie → thông báo + HITL
                if (this.#lastScanResults.length > 0) {
                    for (const zombie of this.#lastScanResults) {
                        // Push notification
                        const notification = JSON.stringify({
                            event: "SHOW_TOAST",
                            payload: {
                                title: `🧟 Zombie: ${zombie.name}`,
                                message: `PID ${zombie.pid} ngốn ${zombie.memMB}MB RAM và idle ${zombie.idleMinutes} phút. Kill?`,
                                type: "warning",
                                duration: 15000,
                            },
                        });
                        process.stdout.write(notification + "\n");

                        // Xin phép HITL để kill
                        try {
                            await HITLGuard.requestApproval({
                                toolName: "zombie_process_hunter",
                                args: { action: "kill", pid: zombie.pid, name: zombie.name, memMB: zombie.memMB },
                                reason: `${zombie.name} (PID ${zombie.pid}) ngốn ${zombie.memMB}MB RAM và idle ${zombie.idleMinutes} phút. Kill để giải phóng RAM?`,
                            });

                            // User approved → kill
                            await execAsync(
                                process.platform === "darwin"
                                    ? `kill -9 ${zombie.pid}`
                                    : `powershell.exe -NoProfile -Command "Stop-Process -Id ${zombie.pid} -Force -ErrorAction Stop"`,
                                { timeout: 10000 },
                            );
                            logger.info(`[ZombieHunter] ✅ Đã kill zombie ${zombie.name} (PID ${zombie.pid}), giải phóng ~${zombie.memMB}MB.`);

                            const killToast = JSON.stringify({
                                event: "SHOW_TOAST",
                                payload: {
                                    title: `✅ Đã kill ${zombie.name}`,
                                    message: `Giải phóng ~${zombie.memMB}MB RAM.`,
                                    type: "success",
                                    duration: 5000,
                                },
                            });
                            process.stdout.write(killToast + "\n");

                            // Xoá khỏi history
                            this.#processHistory.delete(zombie.pid);
                        } catch (err: unknown) {
                            const msg = err instanceof Error ? err.message : String(err);
                            logger.info(`[ZombieHunter] Người dùng từ chối kill ${zombie.name}: ${msg}`);
                            // Từ chối → bỏ qua, không hỏi lại cho PID này trong lần quét tới
                            // Reset firstSeen để không alert lại ngay
                            const snapshot = this.#processHistory.get(zombie.pid);
                            if (snapshot) {
                                snapshot.firstSeen = Date.now();
                            }
                        }
                    }
                }
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.error(`[ZombieHunter] Lỗi auto-scan: ${msg}`);
            }
        }, AUTO_SCAN_INTERVAL_MS);
        this.#autoScanInterval.unref();

        logger.info(`[ZombieHunter] ✅ Auto-scan bắt đầu: mỗi 5 phút, ngưỡng ${thresholdMB}MB, idle ${idleThresholdMinutes} phút.`);
        return `[ZOMBIE SUCCESS] Auto-scan đã bật.\n- Quét mỗi: 5 phút\n- Ngưỡng RAM: ${thresholdMB}MB\n- Ngưỡng idle: ${idleThresholdMinutes} phút\n- HITL: Sẽ hỏi trước khi kill`;
    }

    /** Dừng quét tự động */
    autoScanStop(): string {
        if (!this.#autoScanInterval) {
            return "[ZOMBIE INFO] Auto-scan chưa được bật.";
        }

        clearInterval(this.#autoScanInterval);
        this.#autoScanInterval = null;
        this.#isAutoScanning = false;

        logger.info("[ZombieHunter] Auto-scan đã dừng.");
        return "[ZOMBIE SUCCESS] Auto-scan đã tắt.";
    }

    /** Trạng thái hiện tại */
    status(): string {
        const autoStatus = this.#isAutoScanning ? "🟢 Đang chạy" : "🔴 Tắt";
        const lastScan = this.#lastScanTime
            ? new Date(this.#lastScanTime).toLocaleTimeString("vi-VN")
            : "Chưa quét";
        const trackedCount = this.#processHistory.size;
        const zombieCount = this.#lastScanResults.length;

        let output = `[ZOMBIE STATUS]\n`;
        output += `- Auto-scan: ${autoStatus}\n`;
        output += `- Ngưỡng RAM: ${this.#memoryThresholdMB}MB\n`;
        output += `- Ngưỡng idle: ${Math.round(this.#idleThresholdMs / 60000)} phút\n`;
        output += `- Lần quét cuối: ${lastScan}\n`;
        output += `- Tiến trình đang track: ${trackedCount}\n`;
        output += `- Zombie phát hiện gần nhất: ${zombieCount}\n`;

        if (this.#lastScanResults.length > 0) {
            output += `\n🧟 Zombies gần nhất:\n`;
            for (const z of this.#lastScanResults) {
                output += `  - ${z.name} (PID ${z.pid}): ${z.memMB}MB, idle ${z.idleMinutes} phút\n`;
            }
        }

        return output;
    }
}

// ── Singleton instance ──────────────────────────────────────────────────────
export const zombieScanner = new ZombieScanner();

// ── Execute function ────────────────────────────────────────────────────────
export const execute = async (argsObj: unknown): Promise<string> => {
    try {
        const parsed = ZombieProcessSchema.parse(argsObj);

        switch (parsed.action) {
            case "scan":
                return await zombieScanner.scan(parsed.memoryThresholdMB, parsed.idleThresholdMinutes);
            case "auto_scan_start":
                return await zombieScanner.autoScanStart(parsed.memoryThresholdMB, parsed.idleThresholdMinutes);
            case "auto_scan_stop":
                return zombieScanner.autoScanStop();
            case "status":
                return zombieScanner.status();
            default:
                return "[ZOMBIE ERROR] Hành động không hợp lệ.";
        }
    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[ZombieHunter] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[ZOMBIE ERROR] Sai định dạng: ${error.issues.map((e) => e.message).join(", ")}`;
        }
        return `[ZOMBIE ERROR] Lỗi hệ thống: ${errMsg}`;
    }
};
