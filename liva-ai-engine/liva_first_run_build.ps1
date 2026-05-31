# ==============================================================================
# LIVA First-Run Native Inference Engine Compilation Pipeline
# ==============================================================================
# Target: Windows Bare-Metal / NVIDIA RTX Hardware / Native C++ from Source
#
# This script:
#   1. Verifies the complete build toolchain (Git, CMake, NVCC, MSVC)
#   2. Interrogates the GPU hardware to extract exact Compute Capability
#   3. Clones or updates the llama.cpp source repository
#   4. Configures CMake with strict hardware-targeted optimization flags
#   5. Compiles from source using all available CPU cores
#   6. Stages outputs (llama.dll + llama-server.exe) into native_lib/
#
# MUST be executed with Administrator privileges.
# ==============================================================================

# Note: Administrator privileges are recommended but not strictly required.
# The script will warn if not elevated but will attempt to proceed.
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "[WARNING] Not running as Administrator. Build may fail if system paths need modification." -ForegroundColor Yellow
    Write-Host "          Proceeding anyway - compilation itself does not require elevation." -ForegroundColor Yellow
}
$ErrorActionPreference = "Stop"

$ScriptRoot = $PSScriptRoot
$RepoDir    = Join-Path $ScriptRoot "llama_cpp_src"
$OutputDir  = Join-Path $ScriptRoot "native_lib"
$BuildDir   = Join-Path $RepoDir "build"
$LogFile    = Join-Path $ScriptRoot "native_build.log"

# ==============================================================================
# UTILITY: Timestamped logging
# ==============================================================================
function Write-Log {
    param([string]$Message, [string]$Color = "White")
    $ts = Get-Date -Format "HH:mm:ss"
    Write-Host "[$ts] $Message" -ForegroundColor $Color
    "[$ts] $Message" | Out-File -Append -FilePath $LogFile -Encoding UTF8
}

# ==============================================================================
# PHASE 0: Pre-flight - Check if native_lib already exists (skip rebuild)
# ==============================================================================
$DllTarget    = Join-Path $OutputDir "llama.dll"
$ServerTarget = Join-Path $OutputDir "llama-server.exe"

if ((Test-Path $DllTarget) -and (Test-Path $ServerTarget)) {
    Write-Log "Native library already exists at: $OutputDir" "Green"
    Write-Log "llama.dll and llama-server.exe detected. Skipping rebuild." "Green"
    Write-Log "To force a rebuild, delete the native_lib/ directory and re-run this script." "Yellow"
    exit 0
}

Write-Log "================================================================" "Cyan"
Write-Log " LIVA Native Zero-Overhead Compilation Pipeline" "Cyan"
Write-Log "================================================================" "Cyan"
"" | Out-File -FilePath $LogFile -Encoding UTF8  # Clear log

# ==============================================================================
# PHASE 1: Toolchain Dependency Verification
# ==============================================================================
Write-Log "[Phase 1/6] Verifying build toolchain..." "Cyan"

# 1a. Git
if (-not (Get-Command "git" -ErrorAction SilentlyContinue)) {
    Write-Log "FATAL: Git is not installed or not in PATH." "Red"
    exit 1
}
Write-Log "  [OK] Git: $(git --version)" "Green"

# 1b. CMake
if (-not (Get-Command "cmake" -ErrorAction SilentlyContinue)) {
    Write-Log "FATAL: CMake is not installed or not in PATH." "Red"
    exit 1
}
$cmakeVer = (cmake --version | Select-Object -First 1)
Write-Log "  [OK] CMake: $cmakeVer" "Green"

# 1c. NVIDIA CUDA Compiler (nvcc)
if (-not (Get-Command "nvcc" -ErrorAction SilentlyContinue)) {
    Write-Log "FATAL: NVIDIA CUDA Toolkit (nvcc) not found." "Red"
    Write-Log "  Install CUDA Toolkit 12.x from: https://developer.nvidia.com/cuda-downloads" "Yellow"
    exit 1
}
$nvccVer = (nvcc --version | Select-String "release" | ForEach-Object { $_.Line.Trim() })
Write-Log "  [OK] NVCC: $nvccVer" "Green"

# 1d. Visual Studio MSVC (C++ Build Tools)
$vsWherePaths = @(
    "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe",
    "${env:ProgramFiles}\Microsoft Visual Studio\Installer\vswhere.exe"
)
$vsWherePath = $vsWherePaths | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $vsWherePath) {
    Write-Log "FATAL: vswhere.exe not found. Visual Studio is not installed." "Red"
    Write-Log "  Download Visual Studio Community 2022: https://visualstudio.microsoft.com/" "Yellow"
    exit 1
}

# Check if C++ tools are installed
$vsPath = & $vsWherePath -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
if (-not $vsPath) {
    # C++ tools missing - try to auto-install them
    $vsAnyPath = & $vsWherePath -latest -property installationPath 2>$null
    if ($vsAnyPath) {
        Write-Log "  Visual Studio found at: $vsAnyPath" "Yellow"
        Write-Log "  BUT: C++ Build Tools workload is NOT installed." "Yellow"
        Write-Log "  Attempting automatic installation of C++ workload..." "Cyan"
        Write-Log "  This will download ~2 GB and take 5-10 minutes. Please wait..." "Yellow"
        
        $vsInstallerPath = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vs_installer.exe"
        if (-not (Test-Path $vsInstallerPath)) {
            # Try setup.exe for modifications
            $vsInstallerPath = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\setup.exe"
        }
        
        if (Test-Path $vsInstallerPath) {
            $installArgs = @(
                "modify",
                "--installPath", $vsAnyPath,
                "--add", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                "--add", "Microsoft.VisualStudio.Component.Windows11SDK.26100",
                "--passive",
                "--norestart"
            )
            Write-Log "  Running: $vsInstallerPath $($installArgs -join ' ')" "Yellow"
            $installProcess = Start-Process -FilePath $vsInstallerPath -ArgumentList $installArgs -Wait -PassThru
            
            if ($installProcess.ExitCode -eq 0 -or $installProcess.ExitCode -eq 3010) {
                Write-Log "  C++ Build Tools installed successfully!" "Green"
                # Re-check after install
                $vsPath = & $vsWherePath -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
            } else {
                Write-Log "  Auto-install returned exit code: $($installProcess.ExitCode)" "Yellow"
            }
        }
    }
}

# Final check
if (-not $vsPath) {
    # Last resort: check if CMake can find any compiler on its own
    $vsPath = & $vsWherePath -latest -property installationPath 2>$null
    if ($vsPath) {
        Write-Log "  [WARN] C++ tools not fully confirmed, but VS exists. CMake may still find a compiler." "Yellow"
    } else {
        Write-Log "FATAL: No suitable C++ compiler found." "Red"
        Write-Log "  Open Visual Studio Installer and add 'Desktop development with C++' workload." "Yellow"
        exit 1
    }
}
Write-Log "  [OK] MSVC: $vsPath" "Green"

Write-Log "[Phase 1/6] Toolchain verification complete." "Green"

# ==============================================================================
# PHASE 2: Dynamic Hardware Interrogation
# ==============================================================================
Write-Log "[Phase 2/6] Interrogating NVIDIA GPU hardware..." "Cyan"

try {
    $gpuName = (nvidia-smi --query-gpu=name --format=csv,noheader | Select-Object -First 1).Trim()
    $computeCapRaw = (nvidia-smi --query-gpu=compute_cap --format=csv,noheader | Select-Object -First 1).Trim()
    $vramMB = (nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits | Select-Object -First 1).Trim()

    if ([string]::IsNullOrWhiteSpace($computeCapRaw)) {
        throw "Empty compute capability response from nvidia-smi"
    }

    # Transform "12.0" to "120" for CMake -DCMAKE_CUDA_ARCHITECTURES
    $computeCap = $computeCapRaw.Replace(".", "")

    Write-Log "  GPU Name       : $gpuName" "Green"
    Write-Log "  Compute Cap    : SM $computeCapRaw (CMake target: $computeCap)" "Green"
    Write-Log "  VRAM           : $vramMB MiB" "Green"
} catch {
    Write-Log "FATAL: GPU interrogation failed: $_" "Red"
    Write-Log "  Ensure NVIDIA Display Drivers are active and nvidia-smi is accessible." "Yellow"
    exit 1
}

Write-Log "[Phase 2/6] Hardware profile locked: SM_$computeCap ($gpuName)" "Green"

# ==============================================================================
# PHASE 3: Source Repository Acquisition
# ==============================================================================
Write-Log "[Phase 3/6] Acquiring llama.cpp source code..." "Cyan"

# Git writes progress to stderr which PowerShell misinterprets as errors.
# Temporarily relax error handling for git operations.
$prevEAP = $ErrorActionPreference
$ErrorActionPreference = "Continue"

if (-not (Test-Path $RepoDir)) {
    Write-Log "  Cloning llama.cpp repository (this may take 1-2 minutes)..." "Yellow"
    git clone --depth 1 https://github.com/ggml-org/llama.cpp.git $RepoDir 2>&1 | Out-File -Append $LogFile
    if ($LASTEXITCODE -ne 0) {
        $ErrorActionPreference = $prevEAP
        Write-Log "FATAL: git clone failed. Check network and log: $LogFile" "Red"
        exit 1
    }
} else {
    Write-Log "  Local repository found. Pulling latest master..." "Yellow"
    Push-Location $RepoDir
    git fetch origin 2>&1 | Out-File -Append $LogFile
    git reset --hard origin/master 2>&1 | Out-File -Append $LogFile
    Pop-Location
}

$ErrorActionPreference = $prevEAP
Write-Log "[Phase 3/6] Source code ready at: $RepoDir" "Green"

# ==============================================================================
# PHASE 4: CMake Configuration - Hardware-Tailored Build
# ==============================================================================
Write-Log "[Phase 4/6] Configuring CMake with strict hardware adherence..." "Cyan"

# Clean previous build artifacts
if (Test-Path $BuildDir) {
    Write-Log "  Cleaning previous build directory..." "Yellow"
    Remove-Item -Recurse -Force $BuildDir
}

Push-Location $RepoDir

# CMake Argument Matrix:
#   -DGGML_CUDA=ON                          : Enforce CUDA runtime backend
#   -DCMAKE_CUDA_ARCHITECTURES=$computeCap  : Generate PTX ONLY for detected silicon
#   -DGGML_CUDA_GRAPHS=ON                   : Kernel fusion, eliminates CPU dispatch overhead (5-18% speedup)
#   -DBUILD_SHARED_LIBS=ON                  : Produce .dll for Python CFFI integration
#   -DCMAKE_BUILD_TYPE=Release              : Full compiler optimizations (-O2, LTO)
#   -DLLAMA_BUILD_TESTS=OFF                 : Skip test binaries to reduce build time
#   -DLLAMA_BUILD_EXAMPLES=ON               : Build llama-server.exe (lives in examples/)

$cmakeArgs = @(
    "-B", "build",
    "-DGGML_CUDA=ON",
    "-DCMAKE_CUDA_ARCHITECTURES=$computeCap",
    "-DGGML_CUDA_GRAPHS=ON",
    "-DBUILD_SHARED_LIBS=ON",
    "-DCMAKE_BUILD_TYPE=Release",
    "-DLLAMA_BUILD_TESTS=OFF",
    "-DLLAMA_BUILD_EXAMPLES=ON"
)

Write-Log "  CMake flags: $($cmakeArgs -join ' ')" "Yellow"

$prevEAP = $ErrorActionPreference
$ErrorActionPreference = "Continue"

& cmake @cmakeArgs 2>&1 | Out-File -Append $LogFile
$cmakeExitCode = $LASTEXITCODE

$ErrorActionPreference = $prevEAP

if ($cmakeExitCode -ne 0) {
    Write-Log "FATAL: CMake configuration failed. Check log: $LogFile" "Red"
    Pop-Location
    exit 1
}

Pop-Location
Write-Log "[Phase 4/6] CMake configuration complete." "Green"

# ==============================================================================
# PHASE 5: Parallel Source Compilation
# ==============================================================================
$cpuCores = $env:NUMBER_OF_PROCESSORS
if (-not $cpuCores) { $cpuCores = 4 }

Write-Log "[Phase 5/6] Compiling from source with $cpuCores parallel threads..." "Cyan"
Write-Log "  This will take 5-15 minutes depending on your CPU. Please wait..." "Yellow"

$buildStart = Get-Date

Push-Location $RepoDir
$prevEAP = $ErrorActionPreference
$ErrorActionPreference = "Continue"
& cmake --build build --config Release -j $cpuCores 2>&1 | Out-File -Append $LogFile
$buildExitCode = $LASTEXITCODE
$ErrorActionPreference = $prevEAP
Pop-Location

$buildDuration = (Get-Date) - $buildStart

if ($buildExitCode -ne 0) {
    Write-Log "FATAL: Compilation failed after $($buildDuration.TotalMinutes.ToString('F1')) minutes." "Red"
    Write-Log "  Check detailed log: $LogFile" "Yellow"
    exit 1
}

Write-Log "[Phase 5/6] Compilation complete in $($buildDuration.TotalMinutes.ToString('F1')) minutes." "Green"

# ==============================================================================
# PHASE 6: Stage Outputs to native_lib/
# ==============================================================================
Write-Log "[Phase 6/6] Staging compiled binaries to $OutputDir..." "Cyan"

# Create output directory
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

# Locate binaries in the build tree
$buildBinDir = Join-Path (Join-Path (Join-Path $RepoDir "build") "bin") "Release"
if (-not (Test-Path $buildBinDir)) {
    # Some CMake generators put it directly in build/bin/
    $buildBinDir = Join-Path (Join-Path $RepoDir "build") "bin"
}

# Collect all critical files: .dll, .exe, .lib
$filesToCopy = @()

# Search for llama.dll / ggml.dll and llama-server.exe recursively
$searchDirs = @(
    (Join-Path (Join-Path (Join-Path $RepoDir "build") "bin") "Release"),
    (Join-Path (Join-Path $RepoDir "build") "bin"),
    (Join-Path (Join-Path $RepoDir "build") "src"),
    (Join-Path (Join-Path (Join-Path $RepoDir "build") "ggml") "src"),
    (Join-Path (Join-Path (Join-Path $RepoDir "build") "examples") "server"),
    (Join-Path $RepoDir "build")
)

$patterns = @("*.dll", "*.exe")

foreach ($dir in $searchDirs) {
    if (Test-Path $dir) {
        foreach ($pattern in $patterns) {
            $found = Get-ChildItem -Path $dir -Filter $pattern -Recurse -ErrorAction SilentlyContinue
            $filesToCopy += $found
        }
    }
}

# Deduplicate by name, prefer Release builds
$uniqueFiles = @{}
foreach ($f in $filesToCopy) {
    $key = $f.Name.ToLower()
    if (-not $uniqueFiles.ContainsKey($key)) {
        $uniqueFiles[$key] = $f
    } elseif ($f.FullName -match "Release") {
        $uniqueFiles[$key] = $f  # Prefer Release variant
    }
}

$copiedCount = 0
foreach ($entry in $uniqueFiles.GetEnumerator()) {
    $src = $entry.Value.FullName
    $dst = Join-Path $OutputDir $entry.Value.Name
    Copy-Item -Path $src -Destination $dst -Force
    Write-Log "  Staged: $($entry.Value.Name) ($([math]::Round($entry.Value.Length / 1MB, 1)) MB)" "Green"
    $copiedCount++
}

# Verify critical outputs
$hasDll    = Test-Path (Join-Path $OutputDir "llama.dll")
$hasGgml   = (Get-ChildItem -Path $OutputDir -Filter "ggml*.dll" -ErrorAction SilentlyContinue).Count -gt 0
$hasServer = Test-Path (Join-Path $OutputDir "llama-server.exe")

if (-not $hasDll) {
    # Try alternate name: the DLL might be named differently
    $altDll = Get-ChildItem -Path $OutputDir -Filter "llama*.dll" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($altDll) {
        $hasDll = $true
        Write-Log "  Note: Core library found as $($altDll.Name)" "Yellow"
    }
}

Write-Log "" "White"
Write-Log "================================================================" "Cyan"
Write-Log " BUILD SUMMARY" "Cyan"
Write-Log "================================================================" "Cyan"
Write-Log "  GPU Target     : $gpuName (SM $computeCapRaw)" "White"
Write-Log "  CUDA Toolkit   : $nvccVer" "White"
Write-Log "  Build Duration : $($buildDuration.TotalMinutes.ToString('F1')) minutes" "White"
Write-Log "  Files Staged   : $copiedCount" "White"
Write-Log "  Output Dir     : $OutputDir" "White"
Write-Log "" "White"

if ($hasDll) {
    Write-Log "  [OK] llama.dll         (CFFI Bridge target)" "Green"
} else {
    Write-Log "  [!!] llama.dll         NOT FOUND" "Red"
}

if ($hasGgml) {
    Write-Log "  [OK] ggml*.dll         (Tensor backend)" "Green"
} else {
    Write-Log "  [!!] ggml*.dll         NOT FOUND - may be statically linked" "Yellow"
}

if ($hasServer) {
    Write-Log "  [OK] llama-server.exe  (Expert Model executor)" "Green"
} else {
    Write-Log "  [!!] llama-server.exe  NOT FOUND" "Red"
}

Write-Log "" "White"

if ($hasDll -and $hasServer) {
    Write-Log "================================================================" "Green"
    Write-Log " SUCCESS: Zero-Overhead Native Engine Ready!" "Green"
    Write-Log " LIVA will use these binaries on next startup." "Green"
    Write-Log "================================================================" "Green"
    exit 0
} else {
    Write-Log "================================================================" "Red"
    Write-Log " PARTIAL BUILD: Some critical binaries are missing." "Red"
    Write-Log " Check log: $LogFile" "Red"
    Write-Log " LIVA will fall back to legacy engine.py" "Yellow"
    Write-Log "================================================================" "Red"
    exit 1
}
