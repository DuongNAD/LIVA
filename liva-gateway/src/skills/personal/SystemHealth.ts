import { logger } from "../../utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import * as os from "node:os";

const execAsync = promisify(exec);

export const metadata = {
  name: "system_health",
  search_keywords: ["sức khỏe hệ thống", "CPU", "RAM", "disk", "bộ nhớ", "tình trạng máy", "system info"],
  description: "[AUTO_RUN] System health check: Measure CPU usage %, RAM usage, disk capacity, and current battery percentage.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {}, // Không yêu cầu tham số
  },
};

export const execute = async (): Promise<string> => {
    try {
        let cpu = 0;
        let battery = "N/A";
        let diskReport = "";
        let gpuReport = "- 🎮 GPU: Không phát hiện (hoặc không có NVIDIA GPU)";
        
        const totalRam = Math.round(os.totalmem() / 1024 / 1024 / 1024);
        const freeRam = Math.round(os.freemem() / 1024 / 1024 / 1024);
        const usedRam = totalRam - freeRam;

        if (process.platform === "win32") {
            const psScript = `
                $cpu = [math]::Round((Get-WmiObject win32_processor | Measure-Object -Property LoadPercentage -Average).Average)
                $os = Get-CimInstance Win32_OperatingSystem
                $totalRam = [math]::Round($os.TotalVisibleMemorySize / 1MB, 2)
                $freeRam = [math]::Round($os.FreePhysicalMemory / 1MB, 2)
                $usedRam = [math]::Round($totalRam - $freeRam, 2)
                $batteryObj = Get-WmiObject win32_battery
                $battery = if ($null -ne $batteryObj) { $batteryObj.EstimatedChargeRemaining } else { "N/A (PC Bàn)" }
                $disk = Get-Volume -DriveLetter C
                $diskFree = [math]::Round($disk.SizeRemaining / 1GB, 2)
                $diskTotal = [math]::Round($disk.Size / 1GB, 2)
                
                $data = [PSCustomObject]@{
                    CPU = $cpu
                    TotalRAM_GB = $totalRam
                    UsedRAM_GB = $usedRam
                    Battery_Percent = $battery
                    DiskC_Free_GB = $diskFree
                    DiskC_Total_GB = $diskTotal
                }
                $data | ConvertTo-Json
            `;

            const { stdout } = await execAsync(`powershell.exe -NoProfile -Command "${psScript}"`);
            const result = JSON.parse(stdout.trim());
            cpu = result.CPU;
            battery = result.Battery_Percent + "%";
            diskReport = `- 💾 Ổ C (Disk C): Còn trống ${result.DiskC_Free_GB} GB / Tổng ${result.DiskC_Total_GB} GB`;

            // GPU/VRAM metrics via nvidia-smi (graceful fallback if no NVIDIA GPU)
            try {
                const { stdout: gpuOut } = await execAsync(
                    `nvidia-smi --query-gpu=name,temperature.gpu,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits`,
                    { timeout: 5000 }
                );
                const gpuLines = gpuOut.trim().split("\n");
                if (gpuLines.length > 0 && gpuLines[0].includes(",")) {
                    const parts = gpuLines[0].split(",").map(s => s.trim());
                    const [gpuName, temp, gpuUtil, vramUsed, vramTotal] = parts;
                    gpuReport = `- 🎮 GPU: ${gpuName}\n- 🌡️ Nhiệt độ GPU: ${temp}°C\n- ⚡ GPU Load: ${gpuUtil}%\n- 🧠 VRAM: ${vramUsed} MB / ${vramTotal} MB (${Math.round(Number(vramUsed) / Number(vramTotal) * 100)}%)`;
                }
            } catch {
                // nvidia-smi not available — skip silently
            }
        } else {
            // macOS / Linux
            try {
                if (process.platform === "darwin") {
                    const { stdout } = await execAsync("ps -A -o %cpu | awk '{s+=$1} END {print s}'");
                    cpu = Math.round(parseFloat(stdout.trim()) / os.cpus().length);
                } else {
                    const { stdout } = await execAsync("top -bn1 | grep 'Cpu(s)' | sed \"s/.*, *\\([0-9.]*\\)%* id.*/\\1/\" | awk '{print 100 - $1}'");
                    cpu = Math.round(parseFloat(stdout.trim()));
                }
            } catch {
                cpu = 0;
            }

            try {
                if (process.platform === "darwin") {
                    const { stdout } = await execAsync("pmset -g batt");
                    const match = stdout.match(/(\d+)%/);
                    if (match) battery = `${match[1]}%`;
                    else battery = "N/A (Cắm sạc liên tiếp / Không có pin)";
                } else {
                    const { stdout } = await execAsync("cat /sys/class/power_supply/BAT0/capacity");
                    battery = `${stdout.trim()}%`;
                }
            } catch {
                battery = "N/A";
            }

            try {
                const { stdout } = await execAsync("df -g / | tail -1");
                const parts = stdout.trim().split(/\s+/);
                if (process.platform === "darwin" && parts.length >= 4) {
                    const total = parseInt(parts[1]);
                    const free = parseInt(parts[3]);
                    diskReport = `- 💾 Ổ đĩa hệ thống: Còn trống ${free} GB / Tổng ${total} GB`;
                } else {
                    const { stdout: dfH } = await execAsync("df -h / | tail -1");
                    const partsH = dfH.trim().split(/\s+/);
                    diskReport = `- 💾 Ổ đĩa hệ thống: Còn trống ${partsH[3]} / Tổng ${partsH[1]} (Capacity: ${partsH[4]})`;
                }
            } catch {
                diskReport = `- 💾 Ổ đĩa hệ thống: N/A`;
            }

            if (process.platform === "darwin") {
                const cpus = os.cpus();
                const cpuModel = cpus[0]?.model || "";
                if (cpuModel.includes("Apple") || process.arch === "arm64") {
                    gpuReport = `- 🎮 GPU: ${cpuModel.trim() || "Apple Silicon GPU"} (Unified Memory)\n- 🧠 VRAM: ~${Math.round(totalRam * 0.75)} GB tối đa (Unified Memory Sharing)`;
                }
            }
        }

        logger.info(`[SystemHealth] Đã thu thập báo cáo sức khoẻ OS + GPU.`);
        
        return `[SYSTEM HEALTH REPORT]
- 🧠 CPU Usage: ${cpu}%
- 🐏 RAM Usage: ${usedRam} GB / ${totalRam} GB
- 🔋 Battery: ${battery}
${diskReport}
${gpuReport}`;

    } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[SystemHealth] Lỗi: ${errMsg}`);
        return `[HEALTH ERROR] Không thể đo lường sức khoẻ hệ thống: ${errMsg}`;
    }
};
