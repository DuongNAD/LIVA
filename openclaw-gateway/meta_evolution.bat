@echo off
setlocal

echo ========================================================
echo [META-EVOLUTION WATCHDOG] DANG SAO LUU HE THONG LOI...
echo ========================================================
xcopy /I /E /Y "src" "src_backup"

:loop
echo.
echo ========================================================
echo   [META-EVOLUTION WATCHDOG] KICH HOAT SINGULARITY DAEMON
echo ========================================================
call npm run evolve

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo 🔴 [META-EVOLUTION WATCHDOG] Daemon bi CRASH! Phat hien Meta-Brick!
    echo 🔴 [META-EVOLUTION WATCHDOG] Dang kich hoat Phao Cuu Sinh (Rollback)...
    xcopy /I /E /Y "src_backup" "src"
    echo 🟢 [META-EVOLUTION WATCHDOG] Rollback hoan tat. Thu hoi su song thanh cong!
    echo 🕒 Dang thu gian 10s cho he thong ha nhiet truoc khi khoi dong lai...
    timeout /t 10
    goto loop
)

echo [META-EVOLUTION WATCHDOG] Vong lap ket thuc an toan (Cach ly loi thu cong).
endlocal
