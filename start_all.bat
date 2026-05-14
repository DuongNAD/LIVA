@echo off
chcp 65001 >NUL
echo ==================================================
echo       KHOI DONG HE THONG LIVA
echo ==================================================
echo.

REM ═══════════════════════════════════════════════════
REM  Port Guard: Tat tien trinh dang chiem port cu
REM ═══════════════════════════════════════════════════

echo [Guard] Kiem tra va giai phong cac cong mang...

REM Kill process on port 8101 (Whisper STT) - NEW
for /f "tokens=5" %%a in ('netstat -ano ^| findstr :8101 ^| findstr LISTENING 2^>NUL') do (
    echo [Guard] Port 8101 dang bi chiem boi PID %%a, dang tat...
    taskkill /T /F /PID %%a >NUL 2>&1
)

REM Kill process on port 8100 (Native gRPC Engine)
for /f "tokens=5" %%a in ('netstat -ano ^| findstr :8100 ^| findstr LISTENING 2^>NUL') do (
    echo [Guard] Port 8100 dang bi chiem boi PID %%a, dang tat...
    taskkill /T /F /PID %%a >NUL 2>&1
)

REM Kill process on port 8002 (Voice Engine Python)
for /f "tokens=5" %%a in ('netstat -ano ^| findstr :8002 ^| findstr LISTENING 2^>NUL') do (
    echo [Guard] Port 8002 dang bi chiem boi PID %%a, dang tat...
    taskkill /T /F /PID %%a >NUL 2>&1
)

REM Kill process on port 8082 (Gateway WebSocket)
for /f "tokens=5" %%a in ('netstat -ano ^| findstr :8082 ^| findstr LISTENING 2^>NUL') do (
    echo [Guard] Port 8082 dang bi chiem boi PID %%a, dang tat...
    taskkill /T /F /PID %%a >NUL 2>&1
)

REM Kill process on port 5173 (Vite Dev Server)
for /f "tokens=5" %%a in ('netstat -ano ^| findstr :5173 ^| findstr LISTENING 2^>NUL') do (
    echo [Guard] Port 5173 dang bi chiem boi PID %%a, dang tat...
    taskkill /T /F /PID %%a >NUL 2>&1
)

REM Kill old Tauri instances
taskkill /F /IM liva-ui.exe >NUL 2>&1

REM Kill old Electron instances (prevents duplicate windows)
taskkill /F /IM electron.exe >NUL 2>&1

REM Kill old Python processes that might be stuck
taskkill /F /IM python.exe >NUL 2>&1

timeout /t 2 /nobreak >NUL
echo [Guard] Cac cong da duoc giai phong.
echo.

REM ═══════════════════════════════════════════════════
REM  Khoi dong Python Dependencies Check
REM ═══════════════════════════════════════════════════

echo [Setup] Kiem tra Python dependencies...
cd liva-ai-engine

REM Check if virtualenv exists
if not exist "venv\Scripts\python.exe" (
    echo [Setup] Tao virtualenv moi...
    python -m venv venv
)

REM Upgrade pip (silent, ignore errors)
call venv\Scripts\pip.exe install --upgrade pip --quiet --disable-pip-version-check 2>NUL

REM Install dependencies (continue even if some fail)
call venv\Scripts\pip.exe install -r requirements.txt --quiet --disable-pip-version-check 2>NUL

REM Generate gRPC proto files if needed
echo [Setup] Kiem tra gRPC proto files...
if not exist "liva_engine_pb2.py" (
    echo [Setup] Generate gRPC files...
    call venv\Scripts\python.exe -m grpc_tools.protoc --python_out=. --grpc_python_out=. --proto_path=..\openclaw-gateway\src\proto ..\openclaw-gateway\src\proto\liva_engine.proto
)

cd ..

timeout /t 1 /nobreak >NUL
echo.

REM ═══════════════════════════════════════════════════
REM  Khoi dong tung thanh phan (START ORDER MATTERS!)
REM ═══════════════════════════════════════════════════

echo [1/6] Dang khoi dong Whisper STT Server (Port 8101)...
start "LIVA Whisper STT" cmd /k "chcp 65001 >NUL && cd liva-ai-engine && call venv\Scripts\activate.bat && python whisper_stt_server.py"

timeout /t 3 /nobreak >NUL

echo [2/6] Dang khoi dong Native AI Engine (gRPC port 8100)...
start "LIVA Native Engine" cmd /k "chcp 65001 >NUL && cd liva-ai-engine && call venv\Scripts\activate.bat && python liva_native_engine.py"

timeout /t 5 /nobreak >NUL

echo [3/6] Dang khoi dong Voice Engine (Python edge-tts port 8002)...
start "LIVA Voice Engine" cmd /k "chcp 65001 >NUL && cd liva-ai-engine && call venv\Scripts\activate.bat && python voice_engine.py"

timeout /t 2 /nobreak >NUL

echo [4/6] Dang khoi dong openclaw-gateway (Node.js port 8082)...
start "OpenClaw Gateway" cmd /k "chcp 65001 >NUL && cd openclaw-gateway && npm run dev"

timeout /t 5 /nobreak >NUL

echo [5/6] Dang khoi dong liva-ui (Vite Dev Server port 5173)...
start "LIVA UI Dev Server" cmd /k "chcp 65001 >NUL && cd liva-ui && npm run dev"

timeout /t 3 /nobreak >NUL

echo [6/6] Dang khoi dong liva-ui (Tauri Desktop)...
start "LIVA UI Desktop" cmd /k "chcp 65001 >NUL && cd liva-ui && npm run desktop"

echo.
echo ==================================================
echo HE THONG LIVA DA KHOI DONG!
echo.
echo Cac cong dang su dung:
echo   Port 8101 - Whisper STT Server
echo   Port 8100 - Native AI Engine (gRPC)
echo   Port 8002 - Voice Engine (TTS)
echo   Port 8082 - Gateway WebSocket
echo   Port 5173 - Vite Dev Server
echo.
echo Kiem tra trang thai: cd liva-ai-engine ^&^& venv\Scripts\python.exe test_services.py
echo ==================================================
pause
