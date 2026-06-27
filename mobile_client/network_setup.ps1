# network_setup.ps1 - Configure adb port reversing for LIVA Mobile Client
Write-Host "Configuring adb port reversing for LIVA Mobile Client..." -ForegroundColor Cyan

# Check if adb is available on the path
if (-not (Get-Command "adb" -ErrorAction SilentlyContinue)) {
    Write-Warning "adb command was not found on your PATH. Make sure Android SDK Platform Tools are installed and on your PATH if deploying to a physical device."
} else {
    # Reverse tcp ports
    Write-Host "Executing adb reverse tcp:5173 tcp:5173 (Vite HMR)..."
    adb reverse tcp:5173 tcp:5173

    Write-Host "Executing adb reverse tcp:3001 tcp:3001 (Gateway API)..."
    adb reverse tcp:3001 tcp:3001

    Write-Host "Executing adb reverse tcp:8002 tcp:8002 (Native Core WebSocket)..."
    adb reverse tcp:8002 tcp:8002

    Write-Host "Current active adb reverse tunnels:" -ForegroundColor Green
    adb reverse --list
}

Write-Host "ADB port reversing setup routine completed!" -ForegroundColor Green
