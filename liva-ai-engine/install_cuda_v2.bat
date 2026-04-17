@echo off
echo =======================================================
echo LIVA AI - COMPILE CUDA FORCE (Visual Studio Community)
echo =======================================================

echo [1] Dang xoa thu vien loi/cu...
call .\venv\Scripts\python.exe -m pip uninstall -y llama-cpp-python

echo [2] Thiet lap co (Flag) ep buoc dung CUDA...
set CMAKE_ARGS=-DGGML_CUDA=on
set FORCE_CMAKE=1

echo [3] Khoi chay Bo bien dich C++ cua Phien ban Community...
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"

echo [4] Dang tien hanh Forge (Ren) llama-cpp-python 0.3.20 (Ho tro Gemma 4)...
call .\venv\Scripts\python.exe -m pip install llama-cpp-python==0.3.20 --no-cache-dir --force-reinstall --no-deps

echo.
echo HOAN TAT! Tat cua so nay va chay start_jarvis.ps1 nhe Sep!
pause
