# Tự động tải file pre-compiled llama.cpp dành cho Windows
$llamaDir = "$env:USERPROFILE\.unsloth\llama.cpp"
if (!(Test-Path -Path $llamaDir)) {
    New-Item -ItemType Directory -Path $llamaDir -Force
}

$url = "https://github.com/ggerganov/llama.cpp/releases/download/b4920/llama-b4920-bin-win-cuda-cu12.20-x64.zip"
$zipPath = "$llamaDir\llama-bin.zip"

Write-Host "Downloading Pre-compiled llama.cpp for Windows..."
Invoke-WebRequest -Uri $url -OutFile $zipPath

Write-Host "Extracting..."
Expand-Archive -Path $zipPath -DestinationPath $llamaDir -Force

Write-Host "Cleaning up..."
Remove-Item -Path $zipPath

Write-Host "Done. llama.cpp is ready for Unsloth."
