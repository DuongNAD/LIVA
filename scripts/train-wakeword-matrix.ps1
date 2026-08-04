param(
    [ValidateSet("Doctor", "Install", "Fetch", "Prepare", "Augment", "Fit", "Train", "Benchmark", "Select", "Resume", "All")]
    [string]$Action = "Doctor",
    [string]$Variant = "all",
    [ValidateRange(0.01, 0.99)]
    [double]$Threshold = 0.58
)

$ErrorActionPreference = "Stop"
$env:PYTHONUTF8 = "1"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$venvDir = Join-Path $repoRoot "tools\wakeword\venv"
$pythonPath = Join-Path $venvDir "Scripts\python.exe"
$cliPath = Join-Path $venvDir "Scripts\livekit-wakeword.exe"
$matrixPath = Join-Path $repoRoot "tools\wakeword\variants.json"
$datasetManifestPath = Join-Path $repoRoot "tools\wakeword\public-datasets.json"
$configTemplatePath = Join-Path $repoRoot "tools\wakeword\hey_liva_prod.yaml"
$publicCorpusDir = Join-Path $repoRoot "tools\wakeword\work\public-corpus"
$variantOutputDir = Join-Path $repoRoot "tools\wakeword\work\variants"
$reportsDir = Join-Path $variantOutputDir "reports"
$selectionPath = Join-Path $reportsDir "selection.json"
$baseModelDir = Join-Path $repoRoot "tools\wakeword\work\output\wake_liva_en_v2"
$ownerPositiveDir = Join-Path $repoRoot "data\wake-enrollment\positive-clean"
$ownerPositivePool = Join-Path $repoRoot "data\wake-enrollment\positive"
$ownerNegativeDir = Join-Path $repoRoot "data\wake-enrollment\negative"
$matrix = Get-Content -Raw -LiteralPath $matrixPath | ConvertFrom-Json

function Invoke-Checked {
    param([scriptblock]$Command)
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code $LASTEXITCODE"
    }
}

function Assert-Toolchain {
    if (-not (Test-Path -LiteralPath $pythonPath)) {
        throw "Wake-word Python environment is missing; run scripts/train-wakeword.ps1 -Action Install"
    }
    if (-not (Test-Path -LiteralPath $cliPath)) {
        throw "livekit-wakeword CLI is missing; run scripts/train-wakeword.ps1 -Action Install"
    }
    $probe = "import torch; assert torch.cuda.is_available(); print(torch.__version__, torch.version.cuda, torch.cuda.get_device_name(0))"
    Invoke-Checked { & $pythonPath -c $probe }
}

function Get-SelectedVariants {
    $selected = @($matrix.variants)
    if ($Variant -ne "all") {
        $selected = @($selected | Where-Object { $_.id -eq $Variant })
        if ($selected.Count -ne 1) {
            throw "Unknown variant '$Variant'"
        }
    }
    return $selected
}

function Invoke-Doctor {
    Assert-Toolchain
    Push-Location $repoRoot
    try {
        Invoke-Checked { node --test scripts/wake-public-corpus.test.mjs }
        Invoke-Checked {
            & $pythonPath scripts/fetch-wake-public-corpus.py `
                --manifest $datasetManifestPath `
                --output $publicCorpusDir `
                --dry-run
        }
    } finally {
        Pop-Location
    }
}

function Install-DatasetDependencies {
    Assert-Toolchain
    Invoke-Checked { & $pythonPath -m pip install "datasets==4.8.5" }
}

function Fetch-PublicCorpus {
    Assert-Toolchain
    Push-Location $repoRoot
    try {
        Invoke-Checked {
            & $pythonPath scripts/fetch-wake-public-corpus.py `
                --manifest $datasetManifestPath `
                --output $publicCorpusDir
        }
    } finally {
        Pop-Location
    }
}

function Prepare-Variants {
    $arguments = @(
        "scripts/prepare-wake-variants.mjs",
        "--matrix", $matrixPath,
        "--base-model-dir", $baseModelDir,
        "--public-corpus-dir", $publicCorpusDir,
        "--owner-positive-dir", $ownerPositiveDir,
        "--output-dir", $variantOutputDir,
        "--config-template", $configTemplatePath
    )
    if (Test-Path -LiteralPath $ownerNegativeDir) {
        $arguments += @("--owner-negative-dir", $ownerNegativeDir)
    }
    Push-Location $repoRoot
    try {
        Invoke-Checked { node @arguments }
    } finally {
        Pop-Location
    }
}

function Get-VariantConfigPath {
    param($Item)
    $configPath = Join-Path $variantOutputDir "configs\$($Item.id).yaml"
    if (-not (Test-Path -LiteralPath $configPath)) {
        throw "$configPath is missing; run -Action Prepare first"
    }
    return $configPath
}

function Augment-Variants {
    Assert-Toolchain
    foreach ($item in Get-SelectedVariants) {
        $configPath = Get-VariantConfigPath $item
        Write-Host "Augmenting wake variant $($item.id)"
        Invoke-Checked { & $cliPath augment $configPath }
    }
}

function Fit-Variants {
    Assert-Toolchain
    foreach ($item in Get-SelectedVariants) {
        $configPath = Get-VariantConfigPath $item
        Write-Host "Fitting wake variant $($item.id)"
        Invoke-Checked { & $cliPath train $configPath }
        Invoke-Checked { & $cliPath export $configPath }
        $modelPath = Join-Path $variantOutputDir "$($item.model_name)\$($item.model_name).onnx"
        Invoke-Checked { & $cliPath eval $configPath -m $modelPath }
    }
}

function Train-Variants {
    Augment-Variants
    Fit-Variants
}

function Test-VariantCompleted {
    param($Item)
    $modelDir = Join-Path $variantOutputDir $Item.model_name
    $modelPath = Join-Path $modelDir "$($Item.model_name).onnx"
    $evalPath = Join-Path $modelDir "$($Item.model_name)_eval.json"
    return (Test-Path -LiteralPath $modelPath) -and (Test-Path -LiteralPath $evalPath)
}

function Resume-Experiment {
    if ($Variant -ne "all") {
        throw "Resume operates on the complete matrix; do not pass -Variant"
    }
    Assert-Toolchain
    foreach ($item in @($matrix.variants)) {
        if (Test-VariantCompleted $item) {
            Write-Host "Skipping completed wake variant $($item.id)"
            continue
        }
        Write-Host "Resuming wake variant $($item.id)"
        Invoke-Checked {
            & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath `
                -Action Train -Variant $item.id -Threshold $Threshold
        }
    }
    Benchmark-Variants
    Select-Candidate
}

function Get-OwnerPositiveHoldout {
    if (-not (Test-Path -LiteralPath $ownerPositiveDir) -or -not (Test-Path -LiteralPath $ownerPositivePool)) {
        throw "Owner positive corpus is missing"
    }
    $trainingHashes = @{}
    foreach ($file in Get-ChildItem -LiteralPath $ownerPositiveDir -File -Filter "*.wav") {
        $trainingHashes[(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash] = $true
    }
    $holdout = @()
    foreach ($file in Get-ChildItem -LiteralPath $ownerPositivePool -File -Filter "*.wav") {
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        if (-not $trainingHashes.ContainsKey($hash)) {
            $holdout += $file.FullName
        }
    }
    if ($holdout.Count -eq 0) {
        throw "No independent owner-positive holdout remains outside positive-clean"
    }
    return $holdout
}

function Benchmark-Variants {
    New-Item -ItemType Directory -Force -Path $reportsDir | Out-Null
    Push-Location $repoRoot
    try {
        Invoke-Checked { cargo build -p liva-native-core --bin wakeword_benchmark }
        $benchmarkPath = Join-Path $repoRoot "target\debug\wakeword_benchmark.exe"
        $positiveFiles = @(Get-OwnerPositiveHoldout)
        $negativeDirectories = @(
            Get-ChildItem -LiteralPath $publicCorpusDir -Directory |
                ForEach-Object { Join-Path $_.FullName "test" } |
                Where-Object { Test-Path -LiteralPath $_ }
        )
        if ($negativeDirectories.Count -eq 0) {
            throw "Public test holdout is missing; run -Action Fetch first"
        }

        foreach ($item in Get-SelectedVariants) {
            $modelDir = Join-Path $variantOutputDir $item.model_name
            $modelPath = Join-Path $modelDir "$($item.model_name).onnx"
            if (-not (Test-Path -LiteralPath $modelPath)) {
                throw "$modelPath is missing; train the variant first"
            }
            $reportPath = Join-Path $reportsDir "$($item.id).json"
            $benchmarkArgs = @("--model", $modelPath, "--threshold", [string]$Threshold)
            foreach ($path in $positiveFiles) { $benchmarkArgs += @("--positive", $path) }
            foreach ($path in $negativeDirectories) { $benchmarkArgs += @("--negative", $path) }
            $benchmarkArgs += @(
                "--min-recall", "0",
                "--max-fpph", "999999",
                "--min-negative-hours", "0",
                "--report", $reportPath
            )
            Invoke-Checked { & $benchmarkPath @benchmarkArgs }
            Copy-Item -LiteralPath (Join-Path $modelDir "variant-manifest.json") `
                -Destination (Join-Path $reportsDir "$($item.id).variant.json") -Force
        }
    } finally {
        Pop-Location
    }
}

function Select-Candidate {
    Push-Location $repoRoot
    try {
        node scripts/select-wake-candidate.mjs `
            --reports-dir $reportsDir `
            --matrix $matrixPath `
            --output $selectionPath
        if ($LASTEXITCODE -eq 2) {
            Write-Warning "Experimental winner selected, but production promotion is blocked. See $selectionPath"
        } elseif ($LASTEXITCODE -ne 0) {
            throw "Candidate selection failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

Push-Location $repoRoot
try {
    switch ($Action) {
        "Doctor" { Invoke-Doctor }
        "Install" { Install-DatasetDependencies }
        "Fetch" { Fetch-PublicCorpus }
        "Prepare" { Prepare-Variants }
        "Augment" { Augment-Variants }
        "Fit" { Fit-Variants }
        "Train" { Train-Variants }
        "Benchmark" { Benchmark-Variants }
        "Select" { Select-Candidate }
        "Resume" { Resume-Experiment }
        "All" {
            Install-DatasetDependencies
            Fetch-PublicCorpus
            Prepare-Variants
            Train-Variants
            Benchmark-Variants
            Select-Candidate
        }
    }
} finally {
    Pop-Location
}
