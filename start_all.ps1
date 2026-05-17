# LIVA System - Start All Services (PowerShell)
# Run: .\start_all.ps1

# UTF-8 Encoding Fix for Vietnamese
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
chcp 65001 > $null

$ErrorActionPreference = "SilentlyContinue"
$ProjectRoot = "E:\Project\LIVA"

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "       KHOI DONG HE THONG LIVA" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

# ============================================================
# Port Guard: Kill processes on required ports
# ============================================================

Write-Host "[Guard] Kiem tra va giai phong cac cong mang..." -ForegroundColor Yellow

$ports = @(8101, 8100, 8002, 8082, 5173)

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

# Kill legacy processes
$processes = @("liva-tauri-poc", "python", "node")
foreach ($p in $processes) {
    $procs = Get-Process -Name $p -ErrorAction SilentlyContinue
    if ($procs) {
        Write-Host "[Guard] Tat tien trinh: $p" -ForegroundColor Yellow
        Stop-Process -Name $p -Force -ErrorAction SilentlyContinue
    }
}

Start-Sleep -Seconds 2
Write-Host "[Guard] Cac cong da duoc giai phong." -ForegroundColor Green
Write-Host ""

# ============================================================
# Python Environment Check
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
Write-Host "[Setup] Upgrade pip..." -ForegroundColor Yellow
& $VenvPip install --upgrade pip --quiet 2>$null

# Install dependencies
Write-Host "[Setup] Install dependencies..." -ForegroundColor Yellow
$ReqFile = Join-Path $ProjectRoot "liva-ai-engine\requirements.txt"
if (Test-Path $ReqFile) {
    & $VenvPip install -r $ReqFile --quiet 2>$null
}

# Generate gRPC files
$GrpcOut = Join-Path $ProjectRoot "liva-ai-engine\liva_engine_pb2.py"
if (-not (Test-Path $GrpcOut)) {
    Write-Host "[Setup] Generate gRPC files..." -ForegroundColor Yellow
    $ProtoFile = Join-Path $ProjectRoot "openclaw-gateway\src\proto\liva_engine.proto"
    & $VenvPython -m grpc_tools.protoc --python_out=. --grpc_python_out=. --proto_path=..\openclaw-gateway\src\proto $ProtoFile
}

Write-Host ""

# ============================================================
# Start Services (Background Jobs)
# ============================================================

$AiEnginePath = Join-Path $ProjectRoot "liva-ai-engine"
$GatewayPath = Join-Path $ProjectRoot "openclaw-gateway"
$UiPath = Join-Path $ProjectRoot "liva-ui"

# Service 1: Whisper STT
Write-Host "[1/6] Dang khoi dong Whisper STT (Port 8101)..." -ForegroundColor Cyan
Start-Process -FilePath "cmd.exe" -ArgumentList "/c `"chcp 65001>nul && `"$VenvPython`" whisper_stt_server.py`"" -WorkingDirectory $AiEnginePath -WindowStyle Normal -PassThru

Start-Sleep -Seconds 3

# Service 2: Native AI Engine (gRPC)
Write-Host "[2/6] Dang khoi dong Native AI Engine (Port 8100)..." -ForegroundColor Cyan
Start-Process -FilePath "cmd.exe" -ArgumentList "/c `"chcp 65001>nul && `"$VenvPython`" liva_native_engine.py`"" -WorkingDirectory $AiEnginePath -WindowStyle Normal -PassThru

Start-Sleep -Seconds 5

# Service 3: Voice Engine
Write-Host "[3/6] Dang khoi dong Voice Engine (Port 8002)..." -ForegroundColor Cyan
Start-Process -FilePath "cmd.exe" -ArgumentList "/c `"chcp 65001>nul && `"$VenvPython`" voice_engine.py`"" -WorkingDirectory $AiEnginePath -WindowStyle Normal -PassThru

Start-Sleep -Seconds 2

# Service 4: Gateway (Node.js)
Write-Host "[4/6] Dang khoi dong Gateway (Port 8082)..." -ForegroundColor Cyan
Start-Process -FilePath "cmd.exe" -ArgumentList "/c `"chcp 65001>nul && npm run dev`"" -WorkingDirectory $GatewayPath -WindowStyle Normal -PassThru

Start-Sleep -Seconds 5

# Service 5: UI Dev Server
Write-Host "[5/6] Dang khoi dong UI Dev Server (Port 5173)..." -ForegroundColor Cyan
Start-Process -FilePath "cmd.exe" -ArgumentList "/c `"chcp 65001>nul && npm run dev`"" -WorkingDirectory $UiPath -WindowStyle Normal -PassThru

Start-Sleep -Seconds 3

# Service 6: LIVA Tauri Desktop (Vite already running on step 5, skip beforeDevCommand)
Write-Host "[6/6] Dang khoi dong LIVA Tauri Desktop..." -ForegroundColor Cyan
$TauriPath = Join-Path $ProjectRoot "liva-tauri-poc"
Start-Process -FilePath "cmd.exe" -ArgumentList "/c `"chcp 65001>nul && cd /d $TauriPath && cargo tauri dev --no-dev-server`"" -WorkingDirectory $TauriPath -WindowStyle Normal -PassThru

Write-Host ""
Write-Host "==================================================" -ForegroundColor Green
Write-Host " HE THONG LIVA DA KHOI DONG!" -ForegroundColor Green
Write-Host "==================================================" -ForegroundColor Green
Write-Host ""
Write-Host "Cac cong dang su dung:" -ForegroundColor White
Write-Host "  Port 8101 - Whisper STT Server" -ForegroundColor Gray
Write-Host "  Port 8100 - Native AI Engine (gRPC)" -ForegroundColor Gray
Write-Host "  Port 8002 - Voice Engine (TTS)" -ForegroundColor Gray
Write-Host "  Port 8082 - Gateway WebSocket" -ForegroundColor Gray
Write-Host "  Port 5173 - Vite Dev Server" -ForegroundColor Gray
Write-Host ""
Write-Host "Kiem tra trang thai: cd liva-ai-engine; .\venv\Scripts\python.exe test_services.py" -ForegroundColor Yellow
