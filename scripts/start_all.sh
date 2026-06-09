#!/bin/bash

# LIVA System - Start All Services (macOS)
# Run: ./scripts/start_all.sh

# Resolve absolute path of project root
SCRIPTPATH="$( cd -- "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
PROJECT_ROOT="$(cd "$SCRIPTPATH/.." && pwd)"
export PATH="$PROJECT_ROOT/liva-ai-engine/native_lib:$PATH"

echo "=================================================="
echo "     HE DIEU HANH NHAN THUC LIVA - BOOTSTRAP MAC"
echo "=================================================="
echo ""

# 1. Port Guard: Kill processes on required ports
echo "[Guard] Checking and clearing ports..."
ports=(8101 8100 8002 8082 5173 8000)
for port in "${ports[@]}"; do
    pids=$(lsof -t -i:"$port" 2>/dev/null)
    if [ -n "$pids" ]; then
        echo "[Guard] Port $port is in use. Killing PID(s): $pids"
        kill -9 $pids 2>/dev/null
    fi
done

# Kill legacy Tauri desktop shell processes
pids=$(pgrep -f "liva-desktop" 2>/dev/null)
if [ -n "$pids" ]; then
    echo "[Guard] Killing legacy liva-desktop processes..."
    kill -9 $pids 2>/dev/null
fi

pids_liva=$(pgrep -x "LIVA" 2>/dev/null)
if [ -n "$pids_liva" ]; then
    echo "[Guard] Killing legacy LIVA processes..."
    kill -9 $pids_liva 2>/dev/null
fi

sleep 1
echo "[Guard] Ports cleared successfully."
echo ""

# 2. Python Environment Setup
echo "[Setup] Verifying Python virtual environment..."
VENV_DIR="$PROJECT_ROOT/liva-ai-engine/venv"
if [ ! -f "$VENV_DIR/bin/python" ]; then
    echo "[Setup] Creating new virtual environment using system Python 3.9.6..."
    /usr/bin/python3 -m venv "$VENV_DIR"
fi

# Offline dependency check and installation
REQ_FILE="$PROJECT_ROOT/liva-ai-engine/requirements_mac.txt"
if [ -f "$REQ_FILE" ]; then
    echo "[Setup] Installing Python dependencies offline from requirements_mac.txt..."
    # Avoid upgrading pip online as it fails in offline/sandboxed environments
    # Use offline wheels cache for installation
    "$VENV_DIR/bin/pip" install --no-index --find-links="$PROJECT_ROOT/liva-ai-engine/wheels" -r "$REQ_FILE" --quiet
fi

# Generate gRPC protobuf files
GRPC_OUT="$PROJECT_ROOT/liva-ai-engine/liva_engine_pb2.py"
if [ ! -f "$GRPC_OUT" ]; then
    echo "[Setup] Generating gRPC files from liva_engine.proto..."
    cd "$PROJECT_ROOT/liva-ai-engine" || exit
    ./venv/bin/python -m grpc_tools.protoc --python_out=. --grpc_python_out=. --proto_path=../liva-gateway/src/proto ../liva-gateway/src/proto/liva_engine.proto
    cd "$PROJECT_ROOT" || exit
fi
echo ""

# 3. Model directory and llama-server checks
# Load environment vars from gateway env if exists
if [ -f "$PROJECT_ROOT/liva-gateway/.env" ]; then
    export $(grep -v '^#' "$PROJECT_ROOT/liva-gateway/.env" | xargs)
fi

AI_MODELS_DIR="${AI_MODELS_DIR:-$HOME/AI_Models}"
echo "[Setup] Using AI models directory: $AI_MODELS_DIR"

if [ ! -d "$AI_MODELS_DIR" ]; then
    echo "⚠️ [Warning] Models directory $AI_MODELS_DIR does not exist."
    echo "Creating it now. Please put your GGUF models there."
    mkdir -p "$AI_MODELS_DIR"
fi

# Verify llama-server is installed
if ! command -v llama-server &> /dev/null; then
    echo "[Setup] llama-server not found. Attempting to install llama.cpp via Homebrew..."
    brew install llama.cpp
fi

# 4. Start Background Services
declare -a pids

# Clean up all background jobs on script exit
cleanup() {
    echo ""
    echo "=================================================="
    echo "[Wait] Shutting down LIVA services..."
    echo "=================================================="
    
    # Kill background jobs
    for pid in "${pids[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null
        fi
    done
    
    # Ensure llama-server is also killed
    llama_pids=$(pgrep -f "llama-server" 2>/dev/null)
    if [ -n "$llama_pids" ]; then
        kill -9 $llama_pids 2>/dev/null
    fi
    
    echo "[OK] All processes terminated. See you again!"
    exit 0
}

trap cleanup SIGINT SIGTERM EXIT

# Start Service 1: Whisper STT
echo "[1/5] Starting Whisper STT (Port 8101)..."
cd "$PROJECT_ROOT/liva-ai-engine" || exit
./venv/bin/python whisper_stt_server.py > /dev/null 2>&1 &
pids+=($!)
sleep 2

# Start Service 2: Voice Engine
echo "[2/5] Starting Voice Engine (Port 8002)..."
cd "$PROJECT_ROOT/liva-ai-engine" || exit
./venv/bin/python voice_engine.py > /dev/null 2>&1 &
pids+=($!)
sleep 1

# Start Service 3: Gateway (Node.js)
echo "[3/5] Starting LIVA Gateway (Port 8082)..."
cd "$PROJECT_ROOT/liva-gateway" || exit
tail -f /dev/null | npm run dev > /dev/null 2>&1 &
pids+=($!)
sleep 3

# Start Service 4: UI Dev Server
echo "[4/5] Starting LIVA UI (Port 5173)..."
cd "$PROJECT_ROOT/liva-ui" || exit
npm run dev > /dev/null 2>&1 &
pids+=($!)
sleep 2

# Start Service 5: LIVA Tauri Desktop Shell
echo "[5/5] Launching LIVA Desktop Shell..."
cd "$PROJECT_ROOT/liva-desktop" || exit
npx tauri dev --no-dev-server
