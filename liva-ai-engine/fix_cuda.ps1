$ErrorActionPreference = "Stop"

Write-Host "===========================" -ForegroundColor Cyan
Write-Host " FIXING CUDA INTEGRATION" -ForegroundColor Cyan
Write-Host "===========================" -ForegroundColor Cyan

$cudaPath = $env:CUDA_PATH
if (-not $cudaPath) {
    $cudaPath = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.8"
}

$cudaExtDir = Join-Path $cudaPath "extras\visual_studio_integration\MSBuildExtensions"
if (-not (Test-Path $cudaExtDir)) {
    Write-Host "Error: Cannot find CUDA MSBuildExtensions at $cudaExtDir" -ForegroundColor Red
    Exit
}

$vsPath = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools"
$vsExtDir = Join-Path $vsPath "MSBuild\Microsoft\VC\v170\BuildCustomizations"

if (-not (Test-Path $vsExtDir)) {
    $vsPath = "C:\BuildTools"
    $vsExtDir = Join-Path $vsPath "MSBuild\Microsoft\VC\v170\BuildCustomizations"
    if (-not (Test-Path $vsExtDir)) {
       Write-Host "Error: Cannot find MSBuild Extensions Directory." -ForegroundColor Red
       Exit
    }
}

Write-Host "Found CUDA Extensions: $cudaExtDir" -ForegroundColor Green
Write-Host "Found VS Extensions  : $vsExtDir" -ForegroundColor Green

$files = @("CUDA 12.8.props", "CUDA 12.8.targets", "CUDA 12.8.xml", "Nvda.Build.CudaTasks.v12.8.dll")
$copied = 0

foreach ($file in $files) {
    $src = Join-Path $cudaExtDir $file
    $dst = Join-Path $vsExtDir $file
    if (Test-Path $src) {
        Copy-Item -Path $src -Destination $dst -Force
        Write-Host "Copied $file" -ForegroundColor Green
        $copied++
    } else {
        Write-Host "Warning: Source file not found: $src" -ForegroundColor Yellow
    }
}

Write-Host "`nSuccessfully copied $copied files." -ForegroundColor Cyan
