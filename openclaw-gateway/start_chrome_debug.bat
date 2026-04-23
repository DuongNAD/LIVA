@echo off
echo ========================================================
echo LIVA CHROME HIJACKER LAUNCHER
echo ========================================================
echo.
echo Canh bao: Lenh nay se dong toan bo cac cua so Chrome hien tai!
echo.

echo.
echo [1/3] Dang dong toan bo tien trinh Chrome...
taskkill /F /IM chrome.exe /T >nul 2>&1
timeout /t 2 /nobreak >nul

echo [2/3] Dang khoi dong lai Chrome voi cong Remote Debugging (9222)...
REM Thu duong dan Chrome 64-bit
if exist "C:\Program Files\Google\Chrome\Application\chrome.exe" (
    start "" "C:\Program Files\Google\Chrome\Application\chrome.exe" --remote-debugging-port=9222 --user-data-dir="C:\liva-chrome-profile"
) else (
    REM Thu duong dan Chrome 32-bit
    start "" "C:\Program Files (x86)\Google\Chrome\Application\chrome.exe" --remote-debugging-port=9222 --user-data-dir="C:\liva-chrome-profile"
)

echo [3/3] Thanh cong! Chrome da duoc mo.
echo Sếp hay dam bao Chrome da dang nhap Google truoc khi tiep tuc.
echo.
echo Bay gio sếp co the mo terminal va chay lenh:
echo npm run test:gemini
echo.
pause
