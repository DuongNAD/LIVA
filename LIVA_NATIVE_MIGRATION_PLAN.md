# LIVA Unified Native Engine (Rust) - Migration Plan

**Goal**: Complete the migration of `liva-gateway` (Node.js) and `liva-ai-engine` (Python) into a single, high-performance binary using Rust and Tokio (`liva-native-core`).

## 🟢 Phase 1: Foundation (COMPLETED)
- [x] Scaffold Rust Cargo project `liva-native-core`.
- [x] Integrate Tokio asynchronous runtime.
- [x] Implement standard IPC bindings / Stdio command parsing for initial testing.
- [x] Pass memory footprint and early EOF stress tests.

## 🟢 Phase 2: Database Migration (COMPLETED)
- [x] Migrate SQLite logic from `liva-gateway` (Connection Pool & WAL).
- [x] Implement Rust-native semantic search (`sqlite-vec` or `libsql`).
- [x] Migrate structured memory interfaces (Facts, Episodic Memory).

## 🟢 Phase 3: AI Engine Migration (COMPLETED)
- [x] Bind `llama.cpp` natively via `llama-cpp-2` crate to run the Gemma Router model.
- [x] Migrate STT (Whisper/Nemotron) to ONNX runtime using Rust's `ort` crate.
- [x] Migrate TTS (Kokoro) to ONNX runtime with direct character-streaming to `cpal` / `rodio` to eliminate token chunking latency.
- [x] Implement VAD and full duplex WebRTC pipeline entirely in Rust.

## 🟢 Phase 4: Integration & Decommission (COMPLETED)
- [x] Move Tauri IPC from WebSocket over to direct Rust bindings `#[tauri::command]`.
- [x] Migrate SmartHome and Telegram Bot integrations.
- [x] Fully deprecate and delete `liva-gateway` and `liva-ai-engine` folders.
