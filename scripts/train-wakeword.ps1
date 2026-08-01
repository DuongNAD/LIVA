param(
    [ValidateSet("Doctor", "Install", "Setup", "Personalize", "Train", "Eval", "All")]
    [string]$Action = "Doctor",
    [string]$ModelPath = "",
    [string]$EnrollmentDir = "",
    [ValidateRange(2, 1000)]
    [int]$EnrollmentMinimum = 20,
    [ValidateRange(1, 50000)]
    [int]$EnrollmentTrainCopies = 10000,
    [ValidateRange(1, 50000)]
    [int]$EnrollmentTestCopies = 1000
)

$ErrorActionPreference = "Stop"
$env:PYTHONUTF8 = "1"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$configPath = Join-Path $repoRoot "tools\wakeword\hey_liva_prod.yaml"
$toolchainPath = Join-Path $repoRoot "tools\wakeword\toolchain.json"
$venvDir = Join-Path $repoRoot "tools\wakeword\venv"
$pythonPath = Join-Path $venvDir "Scripts\python.exe"
$cliPath = Join-Path $venvDir "Scripts\livekit-wakeword.exe"
$modelDir = Join-Path $repoRoot "tools\wakeword\work\output\wake_liva_en_v2"
$toolchain = Get-Content -Raw -LiteralPath $toolchainPath | ConvertFrom-Json
$torchVersion = [string]$toolchain.torch.version
$torchIndexUrl = [string]$toolchain.torch.index_url

function Invoke-Checked {
    param([scriptblock]$Command)
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Lệnh thất bại với exit code $LASTEXITCODE"
    }
}

function Invoke-Doctor {
    Push-Location $repoRoot
    try {
        Invoke-Checked { node scripts/wake-training-check.mjs }
        Invoke-Checked { ffmpeg -version }
        Invoke-Checked { nvidia-smi --query-gpu=name,memory.total --format=csv,noheader }
        if (Test-Path -LiteralPath $cliPath) {
            Assert-CudaTorch
            Invoke-Checked { & $cliPath --help }
        } else {
            Write-Host "Training venv chưa được cài: chạy -Action Install"
        }
    } finally {
        Pop-Location
    }
}

function Assert-CudaTorch {
    # Không dùng string literal trong đoạn `-c`: Windows PowerShell 5.1 có thể
    # bỏ dấu quote khi chuyển native argv. Assertion vẫn fail-closed trên CPU.
    $probe = "import torch; assert torch.cuda.is_available(); print(torch.__version__, torch.version.cuda, torch.cuda.get_device_name(0))"
    Invoke-Checked { & $pythonPath -c $probe }
}

function Install-Toolchain {
    if (-not (Test-Path -LiteralPath $pythonPath)) {
        Invoke-Checked { py -3.11 -m venv $venvDir }
    }
    Invoke-Checked { & $pythonPath -m pip install --upgrade pip }
    $requirement = "livekit-wakeword[train,eval,export] @ git+$($toolchain.repository)@$($toolchain.commit)"
    Invoke-Checked { & $pythonPath -m pip install $requirement }
    Invoke-Checked {
        & $pythonPath -m pip install --force-reinstall `
            "torch==$torchVersion" "torchaudio==$torchVersion" `
            --index-url $torchIndexUrl
    }
    Assert-CudaTorch
    Invoke-Checked { & $cliPath --help }
}

function Setup-TrainingData {
    Invoke-Checked { & $cliPath setup --config $configPath }
}

function Prepare-Enrollment {
    $sourceDir = $EnrollmentDir
    if ([string]::IsNullOrWhiteSpace($sourceDir)) {
        $sourceDir = Join-Path $repoRoot "data\wake-enrollment\positive"
    }
    Invoke-Checked {
        node scripts/prepare-wake-enrollment.mjs `
            --source $sourceDir `
            --model-dir $modelDir `
            --minimum $EnrollmentMinimum `
            --train-copies $EnrollmentTrainCopies `
            --test-copies $EnrollmentTestCopies
    }
}

function Start-Training {
    Assert-CudaTorch
    # Generate the full synthetic corpus first. Real enrollment is injected only
    # after generation so it cannot reduce the configured synthetic sample count.
    Invoke-Checked { & $cliPath generate $configPath }
    Prepare-Enrollment
    Invoke-Checked { & $cliPath augment $configPath }
    Invoke-Checked { & $cliPath train $configPath }
    Invoke-Checked { & $cliPath export $configPath }
    $trainedModel = Join-Path $modelDir "wake_liva_en_v2.onnx"
    Invoke-Checked { & $cliPath eval $configPath -m $trainedModel }
}

function Start-Evaluation {
    if ([string]::IsNullOrWhiteSpace($ModelPath)) {
        throw "-ModelPath là bắt buộc với -Action Eval"
    }
    $resolvedModel = (Resolve-Path -LiteralPath $ModelPath).Path
    Assert-CudaTorch
    Invoke-Checked { & $cliPath eval $configPath -m $resolvedModel }
}

Push-Location $repoRoot
try {
    switch ($Action) {
        "Doctor" { Invoke-Doctor }
        "Install" { Install-Toolchain }
        "Setup" { Setup-TrainingData }
        "Personalize" { Start-Training }
        "Train" { Start-Training }
        "Eval" { Start-Evaluation }
        "All" {
            Install-Toolchain
            Setup-TrainingData
            Start-Training
        }
    }
} finally {
    Pop-Location
}
