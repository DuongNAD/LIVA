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

function Show-LivaResourcePreflight {
    # Hai bo kiem, hai cau hoi khac nhau:
    #   - binary --preflight : moi truong CHAY (profile build, CUDA/GPU, espeak-ng,
    #     ffmpeg, vec0, khoa ma hoa) -> thu Node khong the biet.
    #   - npm run doctor     : FILE MODEL tren dia, kem lenh tai.
    # Ca hai chi bao cao. -CheckOnly khong duoc doi bat cu thu gi.

    # Doc RELEASE truoc: do la ban ship, va dong "vision" phu thuoc profile —
    # bao theo debug se noi vision khong dung duoc trong khi ban that thi dung duoc.
    $candidates = @(
        (Join-Path $ProjectRoot "target\release\liva-native-core.exe"),
        (Join-Path $ProjectRoot "target\debug\liva-native-core.exe")
    )
    $exe = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1

    if (-not $exe) {
        Write-Host "[Check] Chua build core -> bo qua bao cao moi truong chay." -ForegroundColor Yellow
        Write-Host "        Build: cargo build --release --features cuda (trong liva-native-core)" -ForegroundColor DarkGray
    } else {
        # Khong dat ten bien la $profile — do la bien tu dong cua PowerShell.
        $buildProfile = if ($exe -like "*\release\*") { "release" } else { "debug" }
        Write-Host "[Check] Moi truong chay - doc tu ban $buildProfile" -ForegroundColor Cyan
        & $exe --preflight
        if ($buildProfile -eq "debug") {
            Write-Host "        (Chi thay ban debug. Ban release co the khac o dong vision.)" -ForegroundColor DarkGray
        }
    }

    Write-Host "[Check] File model tren dia (npm run doctor)" -ForegroundColor Cyan
    Push-Location -Path $ProjectRoot
    try {
        & npm.cmd run doctor
        # doctor thoat 1 khi thieu file BAT BUOC. -CheckOnly la bo bao cao nen
        # khong throw — chi noi ro, con quyet dinh chan hay khong la cua nguoi goi.
        if ($LASTEXITCODE -ne 0) {
            Write-Host "[Check] doctor bao thieu model bat buoc (exit $LASTEXITCODE)." -ForegroundColor Yellow
        }
    } finally {
        Pop-Location
    }
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
    Write-Host ""
    Show-LivaResourcePreflight
    Write-Host ""
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
