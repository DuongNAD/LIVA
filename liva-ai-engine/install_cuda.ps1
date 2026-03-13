$ErrorActionPreference = "Stop"

Write-Host "===========================" -ForegroundColor Cyan
Write-Host "LIVA AI - CUDA SETUP" -ForegroundColor Cyan
Write-Host "===========================" -ForegroundColor Cyan

if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    Write-Host "Downloading VS Build Tools..." -ForegroundColor Yellow
    $exePath = "$env:TEMP\vs_buildtools.exe"
    Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vs_buildtools.exe" -OutFile $exePath
    
    Write-Host "Installing silently (5-10 mins)..." -ForegroundColor Yellow
    $process = Start-Process -FilePath $exePath -ArgumentList "--quiet --wait --norestart --nocache --installPath C:\BuildTools --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" -Wait -PassThru
    
    if ($process.ExitCode -ne 0 -and $process.ExitCode -ne 3010) {
        Write-Host "Install Failed: $($process.ExitCode)" -ForegroundColor Red
        exit
    }
    Write-Host "Install OK!" -ForegroundColor Green
}

Write-Host "Cleaning up old package..." -ForegroundColor Yellow
Set-Location "E:\Project\LIVA\liva-ai-engine"
& .\venv\Scripts\python.exe -m pip uninstall -y llama-cpp-python

Write-Host "Compiling Llama-cpp with CUDA 12..." -ForegroundColor Yellow
$env:CMAKE_ARGS="-DGGML_CUDA=on"
$env:FORCE_CMAKE="1"

# Bat file execution to setup env vars is tricky in powershell, so we will use cmd.exe directly to do the build
$cmdScript = @"
call `"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat`" || call `"C:\BuildTools\VC\Auxiliary\Build\vcvars64.bat`"
cd /d "E:\Project\LIVA\liva-ai-engine"
.\venv\Scripts\python.exe -m pip install llama-cpp-python --no-cache-dir --upgrade --force-reinstall
"@

$batPath = "$env:TEMP\build_llama.bat"
$cmdScript | Out-File -FilePath $batPath -Encoding ASCII
& cmd.exe /c $batPath

Write-Host "DONE!" -ForegroundColor Green
