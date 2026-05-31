# adb-reverse.ps1
# Automates Android emulator port tunneling for local Vite (5173) and Express API (3001).

# --- Resolve adb ---
$Adb = Get-Command adb -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if (-not $Adb) {
    $Candidates = @(
        "$env:ANDROID_HOME\platform-tools\adb.exe",
        "$env:ANDROID_SDK_ROOT\platform-tools\adb.exe",
        "C:\Android\Sdk\platform-tools\adb.exe",
        "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe"
    )
    $Adb = $Candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}

if (-not $Adb) {
    Write-Host "[!!] adb.exe not found. Please add platform-tools to PATH or configure ANDROID_HOME." -ForegroundColor Red
    exit 1
}

Write-Host "📱 Found adb: $Adb" -ForegroundColor Gray

# Check if a device/emulator is connected
$Devices = & $Adb devices 2>&1 | Select-String "emulator|device" | Where-Object { $_ -notmatch "List of" }

if (-not $Devices) {
    Write-Host "[!!] No active Android emulator or device was detected." -ForegroundColor Yellow
    Write-Host "     Please launch the emulator first and then re-run this script." -ForegroundColor Gray
    exit 0
}

Write-Host "🚀 Setting up local port tunnels (emulator ➔ host)..." -ForegroundColor Cyan

# Reverse Vite Dev Port
& $Adb reverse tcp:5173 tcp:5173
Write-Host "     [Vite] tcp:5173 ➔ http://localhost:5173" -ForegroundColor Green

# Reverse Backend API Port
& $Adb reverse tcp:3001 tcp:3001
Write-Host "     [API]  tcp:3001 ➔ http://localhost:3001" -ForegroundColor Green

Write-Host "✅ adb reverse tunnels active. Live reload is ready." -ForegroundColor Green
