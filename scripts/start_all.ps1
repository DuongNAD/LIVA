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
# Python Environment Check & Setup
# ============================================================

Write-Host "[Setup] Kiem tra Python dependencies..." -ForegroundColor Yellow
$VenvPath = Join-Path $ProjectRoot "liva-ai-engine\venv"

if (-not (Test-Path "$VenvPath\Scripts\python.exe")) {
    Write-Host "[Setup] Tao virtualenv moi..." -ForegroundColor Yellow
    New-Item -ItemType Directory -Path $VenvPath -Force | Out-Null
    & python -m venv $VenvPath
}

$VenvPython = "$VenvPath\Scripts\python.exe"
$VenvPip = "$VenvPath\Scripts\pip.exe"

# Upgrade pip
Write-Host "[Setup] Nang cap pip..." -ForegroundColor Yellow
& $VenvPip install --upgrade pip --quiet 2>$null

# Install dependencies
Write-Host "[Setup] Cai dat dependencies tu requirements.txt..." -ForegroundColor Yellow
$ReqFile = Join-Path $ProjectRoot "liva-ai-engine\requirements.txt"
if (Test-Path $ReqFile) {
    & $VenvPip install -r $ReqFile --quiet 2>$null
}

# Generate gRPC files
$GrpcOut = Join-Path $ProjectRoot "liva-ai-engine\liva_engine_pb2.py"
if (-not (Test-Path $GrpcOut)) {
    Write-Host "[Setup] Generate gRPC files tu liva_engine.proto..." -ForegroundColor Yellow
    $ProtoFile = Join-Path $ProjectRoot "liva-gateway\src\proto\liva_engine.proto"
    & $VenvPython -m grpc_tools.protoc --python_out=. --grpc_python_out=. --proto_path=..\liva-gateway\src\proto $ProtoFile
}

Write-Host ""

# ============================================================
# Start Services (Background Jobs)
# ============================================================

$AiEnginePath = Join-Path $ProjectRoot "liva-ai-engine"
$GatewayPath = Join-Path $ProjectRoot "liva-gateway"
$UiPath = Join-Path $ProjectRoot "liva-ui"

# Service 1: Whisper STT
Write-Host "[1/6] Dang khoi dong Whisper STT (Port 8101)..." -ForegroundColor Cyan
$sttProc = Start-Process -FilePath $VenvPython -ArgumentList "whisper_stt_server.py" -WorkingDirectory $AiEnginePath -WindowStyle Hidden -PassThru

Start-Sleep -Seconds 2

# Service 2: Native AI Engine (gRPC)
Write-Host "[2/6] Dang khoi dong Native AI Engine (Port 8100)..." -ForegroundColor Cyan
$engineProc = Start-Process -FilePath $VenvPython -ArgumentList "liva_native_engine.py" -WorkingDirectory $AiEnginePath -WindowStyle Hidden -PassThru

Start-Sleep -Seconds 3

# Service 3: Voice Engine
Write-Host "[3/6] Dang khoi dong Voice Engine (Port 8002)..." -ForegroundColor Cyan
$voiceProc = Start-Process -FilePath $VenvPython -ArgumentList "voice_engine.py" -WorkingDirectory $AiEnginePath -WindowStyle Hidden -PassThru

Start-Sleep -Seconds 1

# Service 4: Gateway (Node.js)
Write-Host "[4/6] Dang khoi dong LIVA Gateway (Port 8082)..." -ForegroundColor Cyan
$gatewayProc = Start-Process -FilePath "npm.cmd" -ArgumentList "run dev" -WorkingDirectory $GatewayPath -WindowStyle Hidden -PassThru

Start-Sleep -Seconds 3

# Service 5: UI Dev Server
Write-Host "[5/6] Dang khoi dong UI Dev Server (Port 5173)..." -ForegroundColor Cyan
$uiProc = Start-Process -FilePath "npm.cmd" -ArgumentList "run dev" -WorkingDirectory $UiPath -WindowStyle Hidden -PassThru

Start-Sleep -Seconds 2

# Service 6: LIVA Tauri Desktop Shell
Write-Host "[Start] Dang kich hoat LIVA Desktop Shell..." -ForegroundColor Green
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

    $daemonProcs = @($sttProc, $engineProc, $voiceProc, $gatewayProc, $uiProc)
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
