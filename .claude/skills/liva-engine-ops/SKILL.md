---
name: liva-engine-ops
description: Manage LIVA Native Engine Daemon lifecycle, monitor runtime subsystem health (LLM, STT, TTS, VAD, AEC), diagnose multi-channel IPC (stdio JSON-lines, Tauri IPC, WebSocket, HTTP), and inspect SQLite WAL connection pools and database integrity.
---

# LIVA Engine Operations

## Workflow

1. **Preflight & Port Validation**:
   - Run startup preflight: `./target/release/liva-native-core --preflight` (or `cargo run -p liva-native-core -- --preflight`).
   - Validate model assets: `npm run doctor`.
   - Inspect port availability: verify Port 5173 (Vite UI) and Port 8002 (Native Engine Daemon HTTP/WS/OpenAI API).
   - On macOS, execute non-destructive preflight check: `./scripts/start_all.sh --check-only`.

2. **Daemon Lifecycle Management**:
   - Standalone Daemon Start: execute `./target/release/liva-native-core` (loads `.env`, boots Tokio runtime, opens SQLite WAL, spawns background services).
   - Full Stack Start: execute `./scripts/start_all.sh` (macOS) or `powershell scripts/start_all.ps1` (Windows).
   - Clean Shutdown & Process Reaping: ensure SIGINT/SIGTERM is sent to allow graceful drain of background tasks (`boot::stop_background_services`). If stale, terminate orphaned processes (`pkill -9 liva-desktop`, `pkill -9 liva-native-core`, `pkill -9 llama-server`).

3. **Subsystem Health Diagnostics**:
   - Query runtime health by invoking `get_system_status` via stdin IPC or WebSocket:
     ```json
     {"id": "diag-1", "command": "get_system_status", "payload": {}}
     ```
   - Evaluate returned subsystem statuses:
     - `llm`: `online` (model loaded, n_ctx, GPU layers), `busy` (generating tokens - normal), or `offline` (unloaded).
     - `stt`: `online` (Parakeet-vi / Nemotron active), `busy`, or `offline` (model directory missing).
     - `tts`: `online` (VieNeu / Piper backends loaded), `busy`, or `offline`.
     - `voice`: Composite status covering VAD, GTCRN denoiser, WebRTC AEC3, and turn-shadow.
     - `db`: Journal mode (`wal`), `vec0` vector extension availability, total encrypted `facts`.
     - `vram` & `gpu`: Total/used VRAM, GPU percent utilization from governor.

4. **Multi-Channel IPC Diagnosis**:
   - **Stdio Line-Delimited IPC**: Verify JSON-lines protocol format. Ping with `{"id": "ping-1", "command": "get_system_status", "payload": {}}` and confirm structured response `{"id": "ping-1", "status": "ok", "data": {...}}`.
   - **Tauri Desktop IPC**: Verify command dispatch through `handle_command` in `liva-native-core`. Check authorization permissions in `src/authorization.rs` for `TauriDashboard` principal.
   - **WebSocket Gateway**: Test connection to `ws://localhost:8002/ws`. Verify auth handshake and event subscription (`system_status`, `voice_event`).
   - **OpenAI-Compatible HTTP API**: Test `http://localhost:8002/v1/models` and `http://localhost:8002/v1/chat/completions` (streaming SSE).

5. **SQLite WAL Database Inspection**:
   - Determine active DB path: inspect `LIVA_DB_PATH` environment variable (defaults to `data_dir/agents/liva_core/structured_memory.sqlite`).
   - Detect stray database fragmentation: verify no orphaned databases exist across project root, `liva-native-core/`, or `src-tauri/`.
   - Check WAL files on disk: verify presence of `.sqlite`, `.sqlite-wal`, and `.sqlite-shm`. Check file size of `.sqlite-wal` (should remain bounded under 50 MB with active checkpoint worker).
   - Direct SQLite diagnostics (read-only):
     ```bash
     sqlite3 <path-to-db> "PRAGMA journal_mode;"         # Must return 'wal'
     sqlite3 <path-to-db> "PRAGMA integrity_check;"     # Must return 'ok'
     sqlite3 <path-to-db> "PRAGMA wal_checkpoint(PASSIVE);" # Inspect checkpoint status
     sqlite3 <path-to-db> "SELECT count(*) FROM facts;" # Verify encrypted memory table
     ```
   - Verify `vec0` dynamic vector search extension: ensure `SELECT vec_version();` succeeds.

6. **Incident Response & Troubleshooting**:
   - *Issue: Port 8002 or 5173 busy* -> identify owner (`lsof -i :8002`), check if LIVA-owned, terminate stale process.
   - *Issue: Database locked / busy timeout* -> check `PRAGMA busy_timeout` (configured at 10,000ms); inspect uncommitted reader/writer transactions.
   - *Issue: vec0 module missing* -> verify `vec0.dylib`/`vec0.so`/`vec0.dll` matches platform in `node_modules/sqlite-vec-*` or Tauri `resources/`.
   - *Issue: Encryption Key Escrow Prompt* -> check stderr for `LIVA - BACK UP ENCRYPTION KEY`; backup local escrow hex safely.

7. **Safety Guardrails & Git Boundary**:
   - Never commit runtime SQLite databases (`*.sqlite`, `*.sqlite-wal`, `*.sqlite-shm`) or `.log` files.
   - All diagnostic queries on active databases must run with query-only or read-only connection semantics.
   - Follow repository Git boundaries: no autonomous commits or remote pushes.
