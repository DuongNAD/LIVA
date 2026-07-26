# LIVA System - Start All Services (PowerShell)
# Run: .\scripts\start_all.ps1

param(
    [switch]$CheckOnly
)

# UTF-8 Encoding Fix for Vietnamese
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
chcp 65001 > $null

$ErrorActionPreference = "Stop"
# Dynamic project root calculation based on script directory
$ProjectRoot = (Resolve-Path "$PSScriptRoot\..").Path

function Test-LivaOwnedProcess {
    param(
        [Parameter(Mandatory = $true)]
        [int]$ProcessId
    )

    $processInfo = Get-CimInstance Win32_Process -Filter "ProcessId = $ProcessId" -ErrorAction SilentlyContinue
    if (-not $processInfo) {
        return $false
    }

    $rootPrefix = $ProjectRoot.TrimEnd('\') + '\'
    $executablePath = [string]$processInfo.ExecutablePath
    $commandLine = [string]$processInfo.CommandLine
    if ($executablePath.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $true
    }

    $isNode = [string]$processInfo.Name -ieq "node.exe"
    $referencesCheckout = $commandLine.IndexOf(
        $rootPrefix,
        [System.StringComparison]::OrdinalIgnoreCase
    ) -ge 0
    $isLivaDevServer = $commandLine -match "(?i)(vite|liva-ui)"
    return $isNode -and $referencesCheckout -and $isLivaDevServer
}

function Clear-LivaPort {
    param(
        [Parameter(Mandatory = $true)]
        [int]$Port
    )

    $connections = @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
    foreach ($connection in $connections) {
        $process = Get-Process -Id $connection.OwningProcess -ErrorAction SilentlyContinue
        if (-not $process) {
            continue
        }
        if (-not (Test-LivaOwnedProcess -ProcessId $connection.OwningProcess)) {
            throw "Port $Port is used by foreign process '$($process.ProcessName)' (PID $($connection.OwningProcess)). Stop it explicitly or configure another port."
        }

        if ($CheckOnly) {
            Write-Host "[Check] Port $Port is held by an existing LIVA process (PID $($connection.OwningProcess))." -ForegroundColor Yellow
            continue
        }

        Write-Host "[Guard] Stopping stale LIVA process on port ${Port}: $($process.ProcessName) (PID $($connection.OwningProcess))" -ForegroundColor Yellow
        Stop-Process -Id $connection.OwningProcess -Force
        Wait-Process -Id $connection.OwningProcess -Timeout 5 -ErrorAction SilentlyContinue
    }
}

function Wait-LocalPort {
    param(
        [Parameter(Mandatory = $true)]
        [int]$Port,
        [Parameter(Mandatory = $true)]
        [System.Diagnostics.Process]$Process,
        [int]$TimeoutSeconds = 15
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $Process.Refresh()
        if ($Process.HasExited) {
            throw "Process '$($Process.ProcessName)' exited before port $Port became ready (exit code $($Process.ExitCode))."
        }
        if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) {
            return
        }
        Start-Sleep -Milliseconds 200
    }
    throw "Timed out waiting for local port $Port after $TimeoutSeconds seconds."
}

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "     HE DIEU HANH NHAN THUC LIVA - BOOTSTRAP V25" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

# ============================================================
# Port Guard: Kill processes on required ports
# ============================================================

Write-Host "[Guard] Kiem tra va giai phong cac cong mang..." -ForegroundColor Yellow

foreach ($port in @(5173, 8002)) {
    Clear-LivaPort -Port $port
}

# Kill legacy Tauri desktop shell processes
$procs = Get-Process -Name "liva-desktop" -ErrorAction SilentlyContinue
if ($procs) {
    foreach ($process in $procs) {
        if (Test-LivaOwnedProcess -ProcessId $process.Id) {
            if ($CheckOnly) {
                Write-Host "[Check] Found stale LIVA desktop process (PID $($process.Id))." -ForegroundColor Yellow
            } else {
                Write-Host "[Guard] Tat tien trinh cu: liva-desktop (PID $($process.Id))" -ForegroundColor Yellow
                Stop-Process -Id $process.Id -Force
            }
        }
    }
}

if ($CheckOnly) {
    Write-Host "[OK] Startup preflight completed without changing any process." -ForegroundColor Green
    return
}

Start-Sleep -Seconds 1
Write-Host "[Guard] Cac cong da duoc giai phong." -ForegroundColor Green
Write-Host ""

# ============================================================
# Start Services (Background Jobs)
# ============================================================

$UiPath = Join-Path $ProjectRoot "liva-ui"
$existingLlamaPids = @(
    Get-Process -Name "llama-server" -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty Id
)

# Service 1: UI Dev Server
Write-Host "[1/2] Dang khoi dong UI Dev Server (Port 5173)..." -ForegroundColor Cyan
$uiProc = Start-Process -FilePath "npm.cmd" -ArgumentList "run dev" -WorkingDirectory $UiPath -WindowStyle Hidden -PassThru

try {
    Wait-LocalPort -Port 5173 -Process $uiProc
} catch {
    Stop-Process -Id $uiProc.Id -Force -ErrorAction SilentlyContinue
    throw
}

# Service 2: LIVA Tauri Desktop Shell (with embedded Rust core)
Write-Host "[2/2] Dang kich hoat LIVA Desktop Shell..." -ForegroundColor Green
$TauriPath = Join-Path $ProjectRoot "liva-desktop"
Push-Location -Path $TauriPath

try {
    & npx.cmd tauri dev --no-dev-server
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri dev exited with code $LASTEXITCODE."
    }
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

    # Stop only llama-server instances created during this LIVA session.
    $llamaProcs = Get-Process -Name "llama-server" -ErrorAction SilentlyContinue
    foreach ($lp in $llamaProcs) {
        if ($lp.Id -notin $existingLlamaPids) {
            Stop-Process -Id $lp.Id -Force -ErrorAction SilentlyContinue
        }
    }

    Write-Host "[OK] He thong da tat sach se. Hen gap lai Sep!" -ForegroundColor Green
}
