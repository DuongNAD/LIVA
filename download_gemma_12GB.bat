@echo off
echo ==================================================
echo   DANG TAI MODEL GEMMA-4-26B BAN NEN Q4_K_M (12GB)
echo ==================================================
echo.
echo File: gemma-4-26B-A4B-it-UD-Q4_K_M.gguf
echo Vi tri luu: E:\AI_Models\
echo.
echo Vui long xai wifi manh, qua trinh nay co the mat 5-15 phut.
echo Thu nho cua so nay lai va lam viec khac.
echo.

curl.exe -L -# -o "E:\AI_Models\gemma-4-26B-A4B-it-UD-Q4_K_M.gguf" "https://huggingface.co/unsloth/gemma-4-26B-A4B-it-GGUF/resolve/main/gemma-4-26B-A4B-it-UD-Q3_K_M.gguf"

echo.
echo ==================================================
echo [HOAN TAT] Da tai xong Model!
echo Bay gio ban hay sua file openclaw-gateway\.env:
echo EXPERT_MODEL_NAME=gemma-4-26B-A4B-it-UD-Q4_K_M.gguf
echo ==================================================
pause
