# LIVA System - Start All Services (PowerShell)
# Run: .\scripts\start_all.ps1

# UTF-8 Encoding Fix for Vietnamese
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
chcp 65001 > $null

$ErrorActionPreference = "SilentlyContinue"
# Dynamic project root calculation based on script directory
$ProjectRoot = (Resolve-Path "$PSScriptRoot\..").Path

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "     HE DIEU HANH NHAN THUC LIVA - BOOTSTRAP V25" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

# ============================================================
# Port Guard: Kill processes on required ports
# ============================================================

Write-Host "[Guard] Kiem tra va giai phong cac cong mang..." -ForegroundColor Yellow

$ports = @(8101, 8100, 8002, 8082, 5173, 8000)

foreach ($port in $ports) {
    $connections = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
    foreach ($conn in $connections) {
        $proc = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue
        if ($proc) {
            Write-Host "[Guard] Port $port bi chiem boi $($proc.ProcessName) (PID $($conn.OwningProcess))" -ForegroundColor Yellow
            Stop-Process -Id $conn.OwningProcess -Force
        }
    }
}

# Kill legacy Tauri desktop shell processes
$procs = Get-Process -Name "liva-desktop" -ErrorAction SilentlyContinue
if ($procs) {
    Write-Host "[Guard] Tat tien trinh cu: liva-desktop" -ForegroundColor Yellow
    Stop-Process -Name "liva-desktop" -Force -ErrorAction SilentlyContinue
}

Start-Sleep -Seconds 1
Write-Host "[Guard] Cac cong da duoc giai phong." -ForegroundColor Green
Write-Host ""

# ============================================================
# Start Services (Background Jobs)
# ============================================================

$UiPath = Join-Path $ProjectRoot "liva-ui"

# Service 1: UI Dev Server
Write-Host "[1/2] Dang khoi dong UI Dev Server (Port 5173)..." -ForegroundColor Cyan
$uiProc = Start-Process -FilePath "npm.cmd" -ArgumentList "run dev" -WorkingDirectory $UiPath -WindowStyle Hidden -PassThru

Start-Sleep -Seconds 2

# Service 2: LIVA Tauri Desktop Shell (with embedded Rust core)
Write-Host "[2/2] Dang kich hoat LIVA Desktop Shell..." -ForegroundColor Green
$TauriPath = Join-Path $ProjectRoot "liva-desktop"
Push-Location -Path $TauriPath

try {
    & npx.cmd tauri dev --no-dev-server
} finally {
    Pop-Location
    
    # ============================================================
    # Cleanup on Desktop Exit
    # ============================================================
    Write-Host "==================================================" -ForegroundColor Yellow
    Write-Host "[Wait] Dang tat LIVA... Vui long cho xa tai nguyen..." -ForegroundColor Yellow
    Write-Host "==================================================" -ForegroundColor Yellow

    $daemonProcs = @($uiProc)
    foreach ($proc in $daemonProcs) {
        if ($proc) {
            Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        }
    }

    # Ensure llama-server is killed to release GPU VRAM
    $llamaProcs = Get-Process -Name "llama-server" -ErrorAction SilentlyContinue
    foreach ($lp in $llamaProcs) {
        Stop-Process -Id $lp.Id -Force -ErrorAction SilentlyContinue
    }

    Write-Host "[OK] He thong da tat sach se. Hen gap lai Sep!" -ForegroundColor Green
}
