@echo off
echo ==================================================
echo       KHOI DONG HE THONG LIVA
echo ==================================================
echo.

echo [Don dep] Dang tat cac tien trinh cu (neu bi treo)...
taskkill /F /IM node.exe >NUL 2>&1
taskkill /F /IM python.exe >NUL 2>&1
taskkill /F /IM electron.exe >NUL 2>&1
timeout /t 2 /nobreak >NUL
echo.

echo [1/3] Dang khoi dong liva-ai-engine (Python)...
start "LIVA AI Engine" cmd /k "cd liva-ai-engine && call venv\Scripts\activate.bat && python engine.py"

echo [2/4] Dang khoi dong Voice Engine (Python edge-tts)...
start "LIVA Voice Engine" cmd /k "cd liva-ai-engine && call venv\Scripts\activate.bat && python voice_engine.py"

echo [3/4] Dang khoi dong openclaw-gateway (Node.js)...
start "OpenClaw Gateway" cmd /k "cd openclaw-gateway && npx tsx src/Gateway.ts"

echo [4/4] Dang khoi dong liva-ui (Electron)...
start "LIVA UI Dev Server" cmd /k "cd liva-ui && npm run dev"
timeout /t 5 /nobreak >NUL
start "LIVA UI Desktop" cmd /k "cd liva-ui && npm run desktop"

echo.
echo ==================================================
echo Hoan tat mo cac terminal va khoi chay he thong!
echo ==================================================
pause
