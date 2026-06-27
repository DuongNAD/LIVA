# LIVA Mobile Client Design Specification

This document details the architectural design, chosen tech stack, UI/UX structure, and network topology for the LIVA Mobile Client, optimized for the Samsung S24+ hardware profile and aligned with the project's native Core backend.

---

## 1. Chosen Tech Stack & Rationale

After evaluating multiple frontend paradigms, **Capacitor 8 wrapping a Vue 3 + Vite + TypeScript web application** is recommended as the optimal stack.

### Stack Breakdown
- **Wrapper**: Capacitor 8 (native Android project bridge)
- **Frontend Framework**: Vue 3 (Composition API) + TypeScript 5.x
- **Build Tool / Bundler**: Vite + ESM configuration (`"type": "module"`)
- **UI & Styling**: UnoCSS (utility-first CSS)
- **Avatar Engines**: Three.js / `@pixiv/three-vrm` (for 3D VRM models) & Pixi.js / `pixi-live2d-display` (for 2D Live2D models)
- **State & Routing**: Vue Router + Pinia
- **Authentication**: `@capgo/capacitor-social-login` (Google Sign-In) + Clerk backend verification

### S24+ Optimization Rationale
- **High-Performance WebView (Android System WebView)**: The Samsung S24+ runs on a Snapdragon 8 Gen 3 (or Exynos 2400) processor with 12GB of RAM. The Chromium-based WebView on this device natively supports dynamic 120Hz refresh rates and hardware-accelerated WebGL. This allows the 3D VRM and 2D Live2D avatars to render smoothly at a locked 120 FPS.
- **Maximum Asset Reuse**: Since the existing `liva-ui` project is already written in Vue 3 and TypeScript with Tauri bindings for desktop, using Capacitor allows **near 100% reuse** of the avatar rendering pipeline, UI layouts, and logic. A pure native approach (Kotlin/Compose) or React Native would require a complete rewrite of the WebGL and rendering components.
- **Alignment with Vault Rules**: The tech stack aligns with `Rules/tech_stack.md` (Node.js v22+, strict TypeScript, ESM-first, no heavy blocking dependencies).
- **Compliance with Capacitor Ops**: Fully utilizes instructions in `Skills/Capacitor Ops.md` regarding versioning mathematical calculations, Chrome Custom Tabs for external links, and intent share target WebView safety (avoiding `removeAllListeners()` crash gotchas).

---

## 2. Network Topology & Protocol Specifications

The LIVA Mobile Client connects to the `liva-native-core` (Rust backend) or the `liva-gateway` (Node.js gateway) over a high-performance network topology.

```
+-------------------------------------------------------+
|                 LIVA Mobile Client                    |
|  (Capacitor 8 / Android WebView / Samsung S24+)       |
+---------------------------+---------------------------+
                            |
                            | (WebSocket connection)
                            | ws://[GATEWAY_IP]:8002/ws
                            v
+-------------------------------------------------------+
|                 liva-native-core                      |
|            (Rust / Tokio WebSocket Server)            |
+-------------------------------------------------------+
```

### WebSocket Protocol Contracts
The client establishes a single WebSocket connection to `ws://[HOST]:8002/ws`. This connection multiplexes text control commands (JSON) and binary audio streaming (custom framed packets).

#### A. Control Channel (JSON Text Frames)
Text frames contain IPC request payloads serialized as JSON.

- **Client Request Format (`IpcRequest`)**:
  ```json
  {
    "id": "unique-request-uuid",
    "command": "command_name",
    "payload": {}
  }
  ```
- **Server Response Format (`IpcResponse`)**:
  ```json
  {
    "id": "unique-request-uuid",
    "status": "ok" | "error",
    "data": {},
    "error": "Error description if status is error"
  }
  ```

##### Supported Commands Index:
1. `ping` / `echo` / `status` - Basic connectivity checks.
2. `get_config` / `get_ai_config` / `get_voice_status` / `get_system_status` - Config and hardware diagnostics.
3. `get_tasks` / `add_task` / `delete_task` / `update_task` - SQLite-based task management.
4. `task_plan_chat` - Interactive task sub-planning (supports token streaming with `"stream": true`).
5. `get_memory_data` / `memory:set_fact` / `memory:get_fact` / `memory:search_hybrid` - Episodic and vector memory interactions.
6. `voice:stt_start` / `voice:stt_chunk` / `voice:stt_stop` - Manual voice transcription chunks.
7. `voice:tts_speak` / `voice:tts_stop` - Trigger speech synthesis.
8. `telegram:send_text` / `integration:smart_home_control` - Remote automation triggers.

#### B. Media Stream Channel (Binary Frames)
Audio packets are serialized into a packed binary layout (Little-Endian) to minimize parsing overhead on both client and server.

##### Frame Structure (`VoiceFrame`):
- **Header (9 bytes)**:
  - Byte 0: `op_code` (1 byte, `u8`)
  - Bytes 1-4: `seq_id` (4 bytes, `u32` little-endian)
  - Bytes 5-8: `payload_size` (4 bytes, `u32` little-endian)
- **Payload (`payload_size` bytes)**:
  - Raw binary array (maximum size limit: 1MB)

##### Operation Codes (`op_code`):
| OpCode | Name | Direction | Payload Description |
|---|---|---|---|
| `0x00` | `OP_AUTH_HANDSHAKE` | Client <-> Server | Echo handshake acknowledgment |
| `0x01` | `OP_MIC_IN` | Client -> Server | Raw 32-bit float PCM audio samples (16kHz, mono) |
| `0x02` | `OP_SPEAKER_OUT` | Server -> Client | Raw 32-bit float PCM audio generated by TTS |
| `0x03` | `OP_FLUSH` | Server -> Client | Command to immediately halt and clear client playback buffer |
| `0x04` | `OP_ACK_PLAYING` | Client -> Server | Acknowledgment of current playback sequence progress |

---

## 3. UI/UX Screens Specification

The mobile UI/UX is structured specifically to support one-handed operation on the Samsung S24+ screen size (6.7 inches, 19.5:9 aspect ratio), keeping core controls in the lower half of the screen.

### 1. Interactive Avatar Screen (Default View)
- **Top 60%**: Full-screen 3D VRM or 2D Live2D avatar. The avatar features dynamic physics, automatic blinking, and eye-tracking (follows touch/drag events).
- **Center**: Floating subtitle overlay representing the transcribed user speech or AI reply.
- **Bottom 40%**:
  - A prominent, glowing radial **Microphone Button** for push-to-talk or toggling continuous voice mode.
  - Small control toggles: **Avatar Mode** (2D vs 3D), **Voice Mute**, and **Menu Expand**.
  - Visualizer ring around the microphone showing voice activity levels.

### 2. Conversation & Logs Dashboard
- **Layout**: Clean chat bubble layout displaying the history of textual and vocal interactions.
- **Features**:
  - Quick-copy buttons for code snippets.
  - Latency dashboard widget showing current server roundtrip times, LLM processing speed, and VAD status.

### 3. Memory & Task Vault
- **Task Planner**: Split screen showing active tasks on the left and a dedicated chat workspace (`task_plan_chat`) on the right. Allows the user to iteratively detail action items.
- **Memory Inspector**: Displays structured data from the three-tiered memory system:
  - **L0 (Turn Layer)**: Recent conversation context nodes.
  - **Facts**: User facts extracted by the AI (e.g. key-value pairs like `hobbies: Học AI`).
  - **Events / Vectors**: Semantic search interface.

### 4. System & Integration Settings
- **Configuration Panels**: Allow updates to local/cloud models, custom system prompts, and Telegram bot bindings.
- **Social Login Hub**: Single-tap authentication setup for Google OAuth via the Social Login plugin.

---

## 4. Port Forwarding & Developer Guidelines

To run and debug the client on a physical Samsung S24+ or an Android Emulator, the local development environment must tunnel the standard ports:

- **Local API Endpoint**: Port `3001` (Tunnels backend REST gateway)
- **Vite Hot Reload Port**: Port `5173` (Tunnels React/Vue HMR asset delivery)
- **Native Core WebSocket Port**: Port `8002` (Tunnels Rust WebSocket endpoint)

### ADB Port Reverse Script (`scripts/adb-reverse.ps1` equivalent)
```powershell
# Tunnel ports from Android emulator/device to host machine
adb reverse tcp:5173 tcp:5173
adb reverse tcp:3001 tcp:3001
adb reverse tcp:8002 tcp:8002
```
