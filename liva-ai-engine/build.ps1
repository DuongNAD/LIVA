$ErrorActionPreference = "Continue"
$env:FORCE_CMAKE = "1"
$env:CMAKE_ARGS = "-DGGML_CUDA=on -DCMAKE_GENERATOR_TOOLSET=`"cuda=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.8`""

Write-Host "Started LlamaCPP Compilation pipeline..."
& "E:\Project\LIVA\liva-ai-engine\venv\Scripts\python.exe" -m pip install llama-cpp-python --no-cache-dir --force-reinstall --upgrade -v > compile_cuda.log 2>&1
Write-Host "Done! Check compile_cuda.log"
