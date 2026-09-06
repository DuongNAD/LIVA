#!/usr/bin/env bash
# LIVA System - Start All Services (macOS / bash)
# Run: ./scripts/start_all.sh              (full bootstrap)
#      ./scripts/start_all.sh --check-only (preflight only, no process changes)
#
# macOS port of scripts/start_all.ps1. Differences vs the Windows version:
# - Port/process guards use lsof instead of Win32 APIs.
# - No CUDA probing: on Apple Silicon llama.cpp uses Metal automatically through
#   llama-cpp-2's default build, so no extra --features are needed for GPU.

set -euo pipefail

CHECK_ONLY=0
[[ "${1:-}" == "--check-only" ]] && CHECK_ONLY=1

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UI_PORT=5173
CORE_PORT=8002

log()  { printf '%s\n' "$*"; }
warn() { printf '\033[33m%s\033[0m\n' "$*"; }
ok()   { printf '\033[32m%s\033[0m\n' "$*"; }
info() { printf '\033[36m%s\033[0m\n' "$*"; }
dim()  { printf '\033[2m%s\033[0m\n' "$*"; }

# ------------------------------------------------------------
# Port Guard helpers
# ------------------------------------------------------------

pids_on_port() {
    lsof -t -iTCP:"$1" -sTCP:LISTEN 2>/dev/null || true
}

is_liva_owned_pid() {
    local pid="$1" exe="" cmd=""
    exe="$(lsof -p "$pid" 2>/dev/null | awk '$4=="txt" {print $NF; exit}')"
    [[ -n "$exe" && "$exe" == "$PROJECT_ROOT"* ]] && return 0
    # Node/vite dev servers may resolve to a global node binary; accept them
    # only when their command line references this checkout.
    cmd="$(ps -o command= -p "$pid" 2>/dev/null || true)"
    [[ "$cmd" == *node* && "$cmd" == *"$PROJECT_ROOT"* ]] && return 0
    return 1
}

clear_liva_port() {
    local port="$1" pid pname
    for pid in $(pids_on_port "$port"); do
        if is_liva_owned_pid "$pid"; then
            if (( CHECK_ONLY )); then
                warn "[Check] Port $port is held by an existing LIVA process (PID $pid)."
            else
                warn "[Guard] Stopping stale LIVA process on port $port (PID $pid)."
                kill -9 "$pid" 2>/dev/null || true
            fi
        else
            pname="$(ps -o comm= -p "$pid" 2>/dev/null || echo unknown)"
            echo "ERROR: Port $port is used by foreign process '$pname' (PID $pid)." >&2
            echo "       Stop it explicitly or configure another port." >&2
            exit 1
        fi
    done
}

wait_local_port() {
    local port="$1" proc_pid="$2" timeout="${3:-15}" waited=0
    while (( waited < timeout * 5 )); do
        if ! kill -0 "$proc_pid" 2>/dev/null; then
            echo "ERROR: Process (PID $proc_pid) exited before port $port became ready." >&2
            return 1
        fi
        if [[ -n "$(pids_on_port "$port")" ]]; then
            return 0
        fi
        sleep 0.2
        waited=$(( waited + 1 ))
    done
    echo "ERROR: Timed out waiting for local port $port after ${timeout}s." >&2
    return 1
}


show_liva_resource_preflight() {
    # Hai bo kiem, hai cau hoi khac nhau (giu nguyen y nghia cua ban Windows):
    #   - binary --preflight : moi truong CHAY (build profile, Metal/GPU, espeak-ng,
    #     ffmpeg, vec0, khoa ma hoa) -> thu Node khong the biet.
    #   - npm run doctor     : FILE MODEL tren dia, kem lenh tai.

    local exe="" candidate
    for candidate in \
        "$PROJECT_ROOT/target/release/liva-native-core" \
        "$PROJECT_ROOT/target/debug/liva-native-core"; do
        if [[ -x "$candidate" ]]; then exe="$candidate"; break; fi
    done

    if [[ -z "$exe" ]]; then
        warn "[Check] Chua build core -> bo qua bao cao moi truong chay."
        dim "        Build: cargo build --release (trong liva-native-core)"
    else
        local build_profile="release"
        [[ "$exe" == */debug/* ]] && build_profile="debug"
        info "[Check] Moi truong chay - doc tu ban $build_profile"
        "$exe" --preflight
        if [[ "$build_profile" == "debug" ]]; then
            dim "        (Chi thay ban debug. Ban release co the khac o dong vision.)"
        fi
    fi

    info "[Check] File model tren dia (npm run doctor)"
    pushd "$PROJECT_ROOT" > /dev/null
    local doctor_rc=0
    npm run doctor || doctor_rc=$?
    if (( doctor_rc != 0 )); then
        warn "[Check] doctor bao thieu model bat buoc (exit $doctor_rc)."
    fi
    popd > /dev/null
}

info "=================================================="
info "     HE DIEU HANH NHAN THUC LIVA - BOOTSTRAP V25 (macOS)"
info "=================================================="
log ""

# ============================================================
# Port Guard
# ============================================================
warn "[Guard] Kiem tra va giai phong cac cong mang..."
clear_liva_port "$UI_PORT"
clear_liva_port "$CORE_PORT"

for pid in $(pgrep -x liva-desktop 2>/dev/null || true); do
    if is_liva_owned_pid "$pid"; then
        if (( CHECK_ONLY )); then
            warn "[Check] Found stale LIVA desktop process (PID $pid)."
        else
            warn "[Guard] Tat tien trinh cu: liva-desktop (PID $pid)"
            kill -9 "$pid" 2>/dev/null || true
        fi
    fi
done

if (( CHECK_ONLY )); then
    log ""
    show_liva_resource_preflight
    log ""
    ok "[OK] Startup preflight completed without changing any process."
    exit 0
fi

sleep 1
ok "[Guard] Cac cong da duoc giai phong."
log ""

# ============================================================
# Start Services
# ============================================================
EXISTING_LLAMA_PIDS="$(pgrep -x llama-server 2>/dev/null | sort | tr '\n' ' ' || true)"

UI_PID=""
cleanup() {
    warn "=================================================="
    warn "[Wait] Dang tat LIVA... Vui long cho xa tai nguyen..."
    warn "=================================================="
    if [[ -n "$UI_PID" ]]; then
        kill -9 "$UI_PID" 2>/dev/null || true
        # Vite spawns children on the port; clear leftovers owned by us.
        for pid in $(pids_on_port "$UI_PORT"); do
            is_liva_owned_pid "$pid" && kill -9 "$pid" 2>/dev/null || true
        done
    fi
    for pid in $(pgrep -x llama-server 2>/dev/null || true); do
        if ! grep -qw "$pid" <<< "$EXISTING_LLAMA_PIDS"; then
            kill -9 "$pid" 2>/dev/null || true
        fi
    done
    ok "[OK] He thong da tat sach se. Hen gap lai Sep!"
}
trap cleanup EXIT INT TERM

# Service 1: UI Dev Server
info "[1/2] Dang khoi dong UI Dev Server (Port $UI_PORT)..."
(
    cd "$PROJECT_ROOT/liva-ui"
    npm run dev > /tmp/liva-ui-dev.log 2>&1 &
    echo $! > /tmp/liva-ui-dev.pid
)
UI_PID="$(cat /tmp/liva-ui-dev.pid)"
if ! wait_local_port "$UI_PORT" "$UI_PID"; then
    dim "--- liva-ui dev log (cuoi) ---"
    tail -20 /tmp/liva-ui-dev.log || true
    exit 1
fi

# Service 2: LIVA Tauri Desktop Shell (with embedded Rust core)
ok "[2/2] Dang kich hoat LIVA Desktop Shell..."
info "      GPU: tren Apple Silicon, llama.cpp tu dong dung Metal (khong can flag)."
dim "      Lan build dau co the lau khoang 6 phut."

cd "$PROJECT_ROOT/liva-desktop"
npx tauri dev --no-dev-server

