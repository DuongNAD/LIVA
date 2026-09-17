# RAM Guard Watchdog — Circuit Breaker for system memory
param (
    [double]$MinFreeGB = 4.0,
    [double]$MaxLoadPct = 80.0
)

$os = Get-CimInstance Win32_OperatingSystem
$totalGB = [math]::Round($os.TotalVisibleMemorySize / 1MB, 2)
$freeGB = [math]::Round($os.FreePhysicalMemory / 1MB, 2)
$usedGB = [math]::Round(($os.TotalVisibleMemorySize - $os.FreePhysicalMemory) / 1MB, 2)
$loadPct = [math]::Round(($usedGB / $totalGB) * 100, 1)

Write-Host "[RAM-GUARD] System Memory: Used ${usedGB}GB / Total ${totalGB}GB (${loadPct}% load), Free ${freeGB}GB" -ForegroundColor Cyan

if ($freeGB -lt $MinFreeGB) {
    Write-Host "[RAM-GUARD] CRITICAL: Free RAM (${freeGB}GB) is below safety threshold (${MinFreeGB}GB). Aborting task to protect OS stability." -ForegroundColor Red
    exit 1
}

if ($loadPct -gt $MaxLoadPct) {
    Write-Host "[RAM-GUARD] CRITICAL: Memory Load (${loadPct}%) exceeds safety threshold (${MaxLoadPct}%). Aborting task to prevent freeze." -ForegroundColor Red
    exit 1
}

Write-Host "[RAM-GUARD] Memory status HEALTHY. Proceeding." -ForegroundColor Green
exit 0
