# ==============================================================================
# J.A.R.V.I.S ONE-CLICK ORCHESTRATOR - (CHẾ ĐỘ TRỰC BAN ZALO E4B)
# ==============================================================================
Write-Host "=============================================" -ForegroundColor Cyan
Write-Host " Khởi Động Đòn Bẩy Zalo J.A.R.V.I.S (E4B)" -ForegroundColor Cyan
Write-Host "=============================================" -ForegroundColor Cyan

$LIVA_ROOT = $PSScriptRoot
$ENGINE_DIR = Join-Path $LIVA_ROOT "liva-ai-engine"
$GATEWAY_DIR = Join-Path $LIVA_ROOT "openclaw-gateway"

# 0. Kiểm tra Python Environment
Write-Host "0. Kiểm tra môi trường Python..." -ForegroundColor Yellow
$PYTHON = Join-Path $ENGINE_DIR "venv\Scripts\python.exe"
if (-not (Test-Path $PYTHON)) {
    Write-Host "❌ Không tìm thấy Python virtualenv. Chạy liva_first_run_build.ps1 trước!" -ForegroundColor Red
    exit 1
}

# 1. Kích hoạt Não Trí Tuệ (Whisper STT - Cổng 8101)
Write-Host "1. Kích hoạt Não Thính giác Whisper (Cổng 8101)..." -ForegroundColor Yellow
$whisperProcess = Start-Process -FilePath cmd -ArgumentList "/k cd /d ""$ENGINE_DIR"" && .\venv\Scripts\python.exe whisper_stt_server.py" -PassThru -WindowStyle Normal
Start-Sleep -Seconds 3

# 2. Kích hoạt Não Định Hướng (Router 8000 - E4B)
Write-Host "2. Kích hoạt Não Trực Ban E4B (Cổng 8000)..." -ForegroundColor Yellow
$routerProcess = Start-Process -FilePath cmd -ArgumentList "/k cd /d ""$ENGINE_DIR"" && .\venv\Scripts\python.exe engine.py" -PassThru -WindowStyle Normal

Start-Sleep -Seconds 2

# 3. Kích hoạt Não Điều Phối Gateway
Write-Host "3. Kích hoạt Gateway Trái Tim Hệ Thống..." -ForegroundColor Green
$gatewayProcess = Start-Process -FilePath cmd -ArgumentList "/k cd /d ""$GATEWAY_DIR"" && npm run dev" -PassThru -WindowStyle Normal

Write-Host "`n[J.A.R.V.I.S MÔI TRƯỜNG E4B ONLINE] Mọi hệ thống đã khởi chạy và sẵn sàng chờ lệnh!" -ForegroundColor Green
Write-Host "Để kích hoạt Singularity tự mổ xẻ mã nguồn, hãy dùng 'npm run evolve'." -ForegroundColor Magenta
Write-Host "Lệnh này sẽ tự động giết E4B, gọi Não 26B, làm xong tự tắt 26B và gọi lại E4B cho Sếp!" -ForegroundColor Magenta
