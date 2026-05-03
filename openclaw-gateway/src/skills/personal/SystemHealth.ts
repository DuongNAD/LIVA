import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

export const metadata = {
  name: "system_health",
  description: "Khám sức khỏe máy tính: Đo lường % CPU đang sử dụng, dung lượng RAM, dung lượng ổ đĩa C và phần trăm Pin (Battery) hiện tại.",
  kit: "PERSONAL_KIT",
  parameters: {
    type: "object",
    properties: {}, // Không yêu cầu tham số
  },
};

export const execute = async (): Promise<string> => {
    try {
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

        // GPU/VRAM metrics via nvidia-smi (graceful fallback if no NVIDIA GPU)
        let gpuReport = "- 🎮 GPU: Không phát hiện (hoặc không có NVIDIA GPU)";
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

        logger.info(`[SystemHealth] Đã thu thập báo cáo sức khoẻ OS + GPU.`);
        
        return `[SYSTEM HEALTH REPORT]
- 🧠 CPU Usage: ${result.CPU}%
- 🐏 RAM Usage: ${result.UsedRAM_GB} GB / ${result.TotalRAM_GB} GB
- 🔋 Battery: ${result.Battery_Percent}%
- 💾 Ổ C (Disk C): Còn trống ${result.DiskC_Free_GB} GB / Tổng ${result.DiskC_Total_GB} GB
${gpuReport}`;

    } catch (error: any) {
        logger.error(`[SystemHealth] Lỗi: ${error.message}`);
        return `[HEALTH ERROR] Không thể đo lường sức khoẻ hệ thống: ${error.message}`;
    }
};
