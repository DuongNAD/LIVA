#!/bin/bash
set -e

# ==============================================================================
# LIVA Native Inference Engine Compilation Pipeline (macOS)
# ==============================================================================
# Target: macOS (Apple Silicon / Intel) / Native C++ from Source
#
# This script:
#   1. Verifies the build toolchain (Git, CMake)
#   2. Clones or updates the llama.cpp source repository (depth 1)
#   3. Configures CMake with hardware-targeted optimization flags:
#      -GGML_METAL=ON for arm64 (Apple Silicon)
#      -DGGML_ACCELERATE=ON for x86_64 (Intel)
#   4. Compiles from source using all available CPU cores
#   5. Stages outputs (libllama.dylib + llama-server) into native_lib/
# ==============================================================================

# Get absolute path of project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SRC_DIR="$PROJECT_ROOT/liva-ai-engine/llama_cpp_src"
NATIVE_LIB_DIR="$PROJECT_ROOT/liva-ai-engine/native_lib"
BUILD_DIR="$SRC_DIR/build"
LOG_FILE="$PROJECT_ROOT/liva-ai-engine/native_build_mac.log"

# Clear log file
echo "=== LIVA Native Compilation Log - macOS ===" > "$LOG_FILE"

log() {
    local color_prefix=""
    local color_suffix=""
    case "$2" in
        "green")  color_prefix="\033[0;32m" ;;
        "yellow") color_prefix="\033[0;33m" ;;
        "red")    color_prefix="\033[0;31m" ;;
        "cyan")   color_prefix="\033[0;36m" ;;
        *)        color_prefix="" ;;
    esac
    [ -n "$color_prefix" ] && color_suffix="\033[0m"

    local ts
    ts=$(date +"%H:%M:%S")
    echo -e "${color_prefix}[$ts] $1${color_suffix}"
    echo "[$ts] $1" >> "$LOG_FILE"
}

log "================================================================" "cyan"
log " LIVA macOS Native Inference Engine Compilation Pipeline" "cyan"
log "================================================================" "cyan"

# ==============================================================================
# PHASE 1: Toolchain Dependency Verification
# ==============================================================================
log "[Phase 1/5] Verifying build toolchain..." "cyan"

if ! command -v git &>/dev/null; then
    log "FATAL: Git is not installed or not in PATH." "red"
    exit 1
fi
log "  [OK] Git: $(git --version)" "green"

if ! command -v cmake &>/dev/null; then
    log "FATAL: CMake is not installed or not in PATH." "red"
    exit 1
fi
log "  [OK] CMake: $(cmake --version | head -n 1)" "green"

log "[Phase 1/5] Toolchain verification complete." "green"

# ==============================================================================
# PHASE 2: Dynamic Hardware / Architecture Detection
# ==============================================================================
log "[Phase 2/5] Interrogating hardware architecture..." "cyan"

OS=$(uname -s)
ARCH=$(uname -m)

if [ "$OS" != "Darwin" ]; then
    log "FATAL: This script is only supported on macOS (Darwin). Detected OS: $OS" "red"
    exit 1
fi

CMAKE_ARGS=("-DBUILD_SHARED_LIBS=ON" "-DLLAMA_BUILD_EXAMPLES=ON" "-DCMAKE_BUILD_TYPE=Release" "-DLLAMA_BUILD_TESTS=OFF" "-DGGML_LTO=ON")

if [ "$ARCH" = "arm64" ]; then
    log "  [OK] Hardware: Apple Silicon (arm64)" "green"
    log "  [OK] Target: Metal Acceleration (-DGGML_METAL=ON)" "green"
    CMAKE_ARGS+=("-DGGML_METAL=ON" "-DCMAKE_C_FLAGS=-mcpu=native" "-DCMAKE_CXX_FLAGS=-mcpu=native")
else
    log "  [OK] Hardware: Intel CPU ($ARCH)" "green"
    log "  [OK] Target: Accelerate Framework (-DGGML_ACCELERATE=ON)" "green"
    CMAKE_ARGS+=("-DGGML_ACCELERATE=ON")
fi

log "[Phase 2/5] Hardware profile locked." "green"

# ==============================================================================
# PHASE 3: Source Repository Acquisition
# ==============================================================================
log "[Phase 3/5] Acquiring llama.cpp source code..." "cyan"

if [ ! -d "$SRC_DIR" ]; then
    log "  Cloning llama.cpp repository (depth 1)..." "yellow"
    git clone --depth 1 https://github.com/ggml-org/llama.cpp.git "$SRC_DIR" >> "$LOG_FILE" 2>&1
    if [ $? -ne 0 ]; then
        log "FATAL: git clone failed. Check network and log: $LOG_FILE" "red"
        exit 1
    fi
else
    log "  Local repository found at $SRC_DIR. Updating to origin latest..." "yellow"
    cd "$SRC_DIR"
    git fetch origin >> "$LOG_FILE" 2>&1
    # Detect default branch name
    DEFAULT_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD | sed 's@^refs/remotes/origin/@@' 2>/dev/null || echo "master")
    if [ -z "$DEFAULT_BRANCH" ]; then
        DEFAULT_BRANCH="master"
    fi
    log "  Resetting to origin/$DEFAULT_BRANCH..." "yellow"
    git reset --hard "origin/$DEFAULT_BRANCH" >> "$LOG_FILE" 2>&1
    cd "$PROJECT_ROOT"
fi

log "[Phase 3/5] Source code ready at: $SRC_DIR" "green"

# ==============================================================================
# PHASE 4: CMake Configuration & Compilation
# ==============================================================================
log "[Phase 4/5] Configuring and compiling llama.cpp..." "cyan"

# Clean build directory if exists to ensure a clean build
if [ -d "$BUILD_DIR" ]; then
    log "  Cleaning previous build directory..." "yellow"
    rm -rf "$BUILD_DIR"
fi
mkdir -p "$BUILD_DIR"

cd "$SRC_DIR"

log "  CMake arguments: ${CMAKE_ARGS[*]}" "yellow"
cmake -B build "${CMAKE_ARGS[@]}" >> "$LOG_FILE" 2>&1
if [ $? -ne 0 ]; then
    log "FATAL: CMake configuration failed. Check log: $LOG_FILE" "red"
    exit 1
fi

NCPU=$(sysctl -n hw.ncpu)
log "  Compiling with $NCPU parallel threads..." "yellow"

BUILD_START=$(date +%s)
cmake --build build --config Release -j "$NCPU" >> "$LOG_FILE" 2>&1
BUILD_RC=$?
BUILD_END=$(date +%s)
BUILD_DURATION=$((BUILD_END - BUILD_START))

if [ $BUILD_RC -ne 0 ]; then
    log "FATAL: Compilation failed. Check detailed log: $LOG_FILE" "red"
    exit 1
fi

log "[Phase 4/5] Compilation complete in $BUILD_DURATION seconds." "green"

# ==============================================================================
# PHASE 5: Stage Outputs to native_lib/
# ==============================================================================
log "[Phase 5/5] Staging compiled binaries to $NATIVE_LIB_DIR..." "cyan"

mkdir -p "$NATIVE_LIB_DIR"

# Clean old artifacts in native_lib directory
rm -f "$NATIVE_LIB_DIR"/*.dylib
rm -f "$NATIVE_LIB_DIR"/llama-server

# Copy all .dylib files (libllama.dylib and potentially libggml.dylib, etc.)
log "  Locating compiled shared libraries (.dylib)..." "yellow"
find "$BUILD_DIR" -name "*.dylib" -type f | while read -r dylib; do
    log "  Staging $(basename "$dylib") ($(( $(stat -f %z "$dylib") / 1024 / 1024 )) MB)" "green"
    cp -f "$dylib" "$NATIVE_LIB_DIR/"
done

# Copy llama-server executable
log "  Locating llama-server executable..." "yellow"
SERVER_PATH=$(find "$BUILD_DIR" -name "llama-server" -type f | head -n 1)
if [ -n "$SERVER_PATH" ]; then
    log "  Staging llama-server ($(( $(stat -f %z "$SERVER_PATH") / 1024 / 1024 )) MB)" "green"
    cp -f "$SERVER_PATH" "$NATIVE_LIB_DIR/"
else
    log "FATAL: llama-server executable not found!" "red"
    exit 1
fi

# Create symlinks for versioned shared libraries if unversioned ones do not exist
if [ ! -f "$NATIVE_LIB_DIR/libllama.dylib" ]; then
    log "  libllama.dylib not found. Searching for versioned libllama.*.dylib..." "yellow"
    VERSIONED_LIB=$(find "$NATIVE_LIB_DIR" -name "libllama.*.dylib" -type f | head -n 1)
    if [ -n "$VERSIONED_LIB" ]; then
        VERSIONED_NAME=$(basename "$VERSIONED_LIB")
        log "  Found versioned library: $VERSIONED_NAME. Creating symlink..." "green"
        ln -sf "$VERSIONED_NAME" "$NATIVE_LIB_DIR/libllama.dylib"
    else
        log "  No versioned libllama.*.dylib found." "yellow"
    fi
fi

if [ ! -f "$NATIVE_LIB_DIR/libllama-common.dylib" ]; then
    log "  libllama-common.dylib not found. Searching for versioned libllama-common.*.dylib..." "yellow"
    VERSIONED_COMMON=$(find "$NATIVE_LIB_DIR" -name "libllama-common.*.dylib" -type f | head -n 1)
    if [ -n "$VERSIONED_COMMON" ]; then
        VERSIONED_COMMON_NAME=$(basename "$VERSIONED_COMMON")
        log "  Found versioned common library: $VERSIONED_COMMON_NAME. Creating symlink..." "green"
        ln -sf "$VERSIONED_COMMON_NAME" "$NATIVE_LIB_DIR/libllama-common.dylib"
    else
        log "  No versioned libllama-common.*.dylib found." "yellow"
    fi
fi


# ==============================================================================
# SUMMARY & FINAL VERIFICATION
# ==============================================================================
has_libllama=0
has_server=0

[ -f "$NATIVE_LIB_DIR/libllama.dylib" ] && has_libllama=1
[ -f "$NATIVE_LIB_DIR/llama-server" ] && has_server=1

log ""
log "================================================================" "cyan"
log " BUILD SUMMARY" "cyan"
log "================================================================" "cyan"
log "  Architecture   : $ARCH"
log "  Build Duration : $BUILD_DURATION seconds"
log "  Output Dir     : $NATIVE_LIB_DIR"
log ""

if [ $has_libllama -eq 1 ]; then
    log "  [OK] libllama.dylib        (Shared Library CFFI Target)" "green"
else
    log "  [!!] libllama.dylib        NOT FOUND" "red"
fi

if [ $has_server -eq 1 ]; then
    log "  [OK] llama-server          (HTTP/gRPC Inference Server)" "green"
else
    log "  [!!] llama-server          NOT FOUND" "red"
fi

log ""

if [ $has_libllama -eq 1 ] && [ $has_server -eq 1 ]; then
    log "================================================================" "green"
    log " SUCCESS: macOS Native Engine Binaries staged successfully!" "green"
    log "================================================================" "green"
    exit 0
else
    log "================================================================" "red"
    log " PARTIAL BUILD: Some critical binaries are missing." "red"
    log "================================================================" "red"
    exit 1
fi
