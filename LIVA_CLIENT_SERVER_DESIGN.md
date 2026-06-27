# LIVA Architecture Upgrade Design: Rust-Centric Client-Server Model

This document proposes the architectural design to transform **LIVA** into a unified, decentralized client-server architecture powered by `liva-native-core` as a high-performance backend server, communicating with ultra-lightweight frontend clients via **WebSockets**.

---

## 1. Chosen Communication Protocol and Port

### Protocol: WebSockets (WS / WSS)
WebSockets is chosen as the primary communication protocol because:
- **Bi-directional & Full-Duplex**: Enables real-time, low-latency, simultaneous communication. This is critical for streaming microphone audio input while playing back synthesized text-to-speech speaker output.
- **Low Overhead**: Minimizes header overhead compared to standard HTTP polling or HTTP long-polling.
- **Native Rust Support**: Easily implemented using the asynchronous `tokio-tungstenite` crate, which is already integrated in `Cargo.toml`.
- **Cross-Platform Client Support**: WebSockets are natively supported by Web Browsers (JavaScript), mobile apps (Flutter, Swift, Kotlin), desktop frameworks (Tauri, Electron), and embedded platforms (ESP32, C++).
- **Transport Security**: Can be secured as `wss://` using TLS (via `rustls` on the server or behind a reverse proxy like Nginx or Caddy) for secure remote access.

### Port Configuration
- **Default Port**: `8002` (configurable via environment variable `LIVA_SERVER_PORT`).
- **Endpoint**: `/ws` for the unified control and audio streaming interface.
- **Fallback Support**: Local development can bind to `127.0.0.1:8002/ws`, while remote deployments bind to `0.0.0.0:8002/ws`.

---

## 2. Separation of Concerns

By adopting this client-server design, the LIVA system decouples heavy computational processes from user interaction components.

```
┌────────────────────────────────────────────────────────┐
│                  HEAVY COMPUTE SERVER                  │
│                  (liva-native-core)                    │
│                                                        │
│ ┌────────────────┐ ┌────────────────┐ ┌──────────────┐ │
│ │  Gemma Router  │ │   Nemotron     │ │  Kokoro TTS  │ │
│ │  LLM (Llama)   │ │  STT (ONNX)    │ │    (ONNX)    │ │
│ └────────────────┘ └────────────────┘ └──────────────┘ │
│ ┌────────────────┐ ┌────────────────┐ ┌──────────────┐ │
│ │   Silero VAD   │ │ SQLite WAL/Vec │ │ Telegram Bot │ │
│ │     (ONNX)     │ │    Database    │ │  Smart Home  │ │
│ └────────────────┘ └────────────────┘ └──────────────┘ │
└───────────────────────────▲────────────────────────────┘
                            │
                  WebSockets (WS / WSS)
                            │
┌───────────────────────────▼────────────────────────────┐
│                ULTRA-LIGHTWEIGHT CLIENT                │
│             (Web Browser, Tauri, Mobile)               │
│                                                        │
│ ┌────────────────┐ ┌────────────────┐ ┌──────────────┐ │
│ │ Audio Recorder │ │  Audio Player  │ │ UI Rendering │ │
│ │   (Mic In)     │ │ (Speaker Out)  │ │ (Live2D/VRM) │ │
│ └────────────────┘ └────────────────┘ └──────────────┘ │
└────────────────────────────────────────────────────────┘
```

### Server (Heavy Computing) — `liva-native-core`
The Rust backend is responsible for all core business logic and heavy computations:
- **Local AI Inference**:
  - **LLM**: Running the Gemma-4-E4B-it router model using native `llama.cpp` bindings.
  - **STT**: Running Whisper/Nemotron-ASR model using ONNX runtime (`ort`).
  - **TTS**: Running Kokoro-v1.0 model using ONNX runtime (`ort`) with raw PCM audio streaming.
  - **VAD**: Running Silero VAD using ONNX runtime (`ort`) to detect voice activity.
- **Data Persistence**:
  - Managing structured memory (Facts, Episodic Memory, Tasks) using SQLite in WAL mode.
  - Generating and querying vector embeddings using `sqlite-vec`.
  - Encrypting/decrypting sensitive data.
- **External Integrations**:
  - Managing Telegram bot pooling and message handling.
  - Coordinating Smart Home (Z-Wave, Zigbee, Local APIs) controls.
  - Native Model Context Protocol (MCP) server.

### Client (Ultra-Lightweight)
The client frontend functions purely as an I/O shell:
- **Audio Capture**: Recording input from the system microphone, downsampling it to **PCM f32, 16kHz, mono**, and sending it as binary frames over WebSockets.
- **Audio Playback**: Receiving audio buffers from the server and feeding them to the device speaker queue.
- **Visuals & UI**: Rendering the user interface (chat log, task lists, settings page, and Live2D or VRM avatar animations synced to the audio playback).
- **Resource Footprint**: Minimal CPU and RAM utilization. The client does not load any AI models or manage database transactions directly, making it highly portable.

---

## 3. Data Flow Diagram and Description

The system operates on a unified WebSocket connection utilizing two types of frames:
1. **JSON Text Frames**: For control commands, database queries, and text-based event notifications.
2. **Binary Frames**: For raw audio streaming (PCM f32, 16kHz, mono) prefixed with a custom 9-byte binary header.

### Interaction Sequence: Voice Conversation Loop

```
Client (Mic/Speaker/UI)                         Server (liva-native-core)
       │                                                    │
       ├───────── 1. OP_AUTH_HANDSHAKE (Binary) ───────────►│
       │◄──────── 2. Handshake Acknowledged ────────────────┤
       │                                                    │
       │   [Voice Interaction Starts]                       │
       ├───────── 3. Stream OP_MIC_IN (Binary) ────────────►│ (Feeds Silero VAD)
       │             (Continuous PCM f32 chunks)            │
       │                                                    │ [VAD triggers SpeechStart]
       │◄──────── 4. Event: state_change (VadStart) ────────┤ (Cancel active LLM/TTS)
       │◄──────── 5. OP_FLUSH (Binary: Interrupt playback) ─┤ (Silence local client)
       │                                                    │
       │   [User finishes speaking]                         │ [VAD triggers SpeechEnd]
       │◄──────── 6. Event: state_change (VadEnd) ──────────┤
       │                                                    │
       │                                                    │──┐ [Process STT (Nemotron)]
       │                                                    │  │
       │                                                    │◄─┘
       │◄──────── 7. Event: stt_completed (Text) ───────────┤
       │                                                    │
       │                                                    │──┐ [Process Agent LLM Graph]
       │                                                    │  │ (Generate response tokens)
       │                                                    │◄─┘
       │◄──────── 8. Event: state_change (LlmGenerating) ───┤
       │                                                    │
       │                                                    │──┐ [Generate TTS Chunks (Kokoro)]
       │                                                    │  │
       │                                                    │◄─┘
       │◄──────── 9. Event: state_change (TtsSpeaking) ─────┤
       │◄──────── 10. Stream OP_SPEAKER_OUT (Binary) ───────┤ (Continuous PCM f32 chunks)
       │              (Audio chunks played by client)       │
       │                                                    │
       │◄──────── 11. Event: state_change (Idle) ───────────┤
```

### Detailed Flow Descriptions:
1. **Handshake**: The client connects to `ws://127.0.0.1:8002/ws` and sends an `OP_AUTH_HANDSHAKE` frame. The server replies with the same frame, establishing the voice session.
2. **Audio Upload & VAD**: The client streams microphone audio as `OP_MIC_IN` binary frames. The server routes these samples into a local ONNX-based `VadEngine`.
3. **Interruption Control**: If the user starts speaking while LIVA is playing audio:
   - The VAD engine fires a `SpeechStart` event.
   - The server cancels any active STT, LLM, or TTS operations, increments the session ID to discard stale responses, and sends an `OP_FLUSH` binary frame to the client.
   - The client immediately clears its audio output queue and silences speaker playback.
4. **Processing**: Once the VAD engine detects the end of user speech:
   - The server transcribes the collected audio buffer to text via `SttManager`.
   - The transcribed text is sent to the client as an `stt_completed` JSON event (updating the UI chat log).
   - The text is passed into `build_pipeline_graph` (Agent graph) which triggers Gemma LLM router model inference.
5. **Audio Streaming & Playback**: As the LLM yields text tokens, they are buffered by `TtsChunker` and synthesized into audio using `TtsManager` (Kokoro ONNX). The resulting PCM f32 samples are pushed to the client as `OP_SPEAKER_OUT` binary frames. The client buffers and plays them sequentially.

---

## 4. Specific API and Payload Structure

### A. Binary Frames (Voice Streaming)
All binary WebSocket frames share a strict 9-byte header format followed by the payload:

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Op Code    |                 Sequence ID                   |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|       Sequence ID (cont)      |          Payload Size         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|       Payload Size (cont)     |      Raw Payload Bytes...     |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+                               +
|                                                               |
```

#### Binary Header Field Specifications
- **Op Code** (1 byte):
  - `0x00` (`OP_AUTH_HANDSHAKE`): Handshake initialization / ping-pong authentication.
  - `0x01` (`OP_MIC_IN`): Input microphone audio streamed from client to server.
  - `0x02` (`OP_SPEAKER_OUT`): Output speaker audio streamed from server to client.
  - `0x03` (`OP_FLUSH`): Interruption command sent by server to instruct the client to immediately clear audio queues and stop playback.
  - `0x04` (`OP_ACK_PLAYING`): Acknowledge playback of a specific packet sequence by the client (for tracking progress).
- **Sequence ID** (4 bytes, Little-Endian `u32`): Used to order packets and manage playback offsets.
- **Payload Size** (4 bytes, Little-Endian `u32`): The size of the raw payload in bytes. Must not exceed `1,048,576` (1MB).
- **Raw Payload**: For `OP_MIC_IN` and `OP_SPEAKER_OUT`, this contains raw `f32` (float32) PCM audio samples, sampled at 16,000 Hz, mono channel. (Each sample is 4 bytes).

---

### B. JSON Text Frames (Control & Telemetry)

All text WebSocket frames are JSON strings. They support bidirectional commands and event broadcasts.

#### 1. Client Command Request (JSON-RPC style)
Sent by the client to invoke actions on the server (mirroring current `handle_command` schema).

```json
{
  "id": "req_001",
  "command": "chat:completion",
  "payload": {
    "messages": [
      {
        "role": "user",
        "content": "Bật đèn phòng khách"
      }
    ],
    "temperature": 0.3,
    "top_p": 0.9,
    "stream": true
  }
}
```

#### 2. Server Response (Standard)
Returned by the server to deliver command results.

```json
{
  "id": "req_001",
  "status": "ok",
  "data": {
    "text": "Tôi đã bật đèn phòng khách.",
    "done": true,
    "usage": {
      "prompt_tokens": 18,
      "completion_tokens": 12,
      "total_tokens": 30
    }
  },
  "error": null
}
```

#### 3. Server Response (Streaming Chunk)
Returned repeatedly when `stream` is `true`.

```json
{
  "id": "req_001",
  "status": "ok",
  "data": {
    "token": "Tôi ",
    "done": false
  },
  "error": null
}
```

#### 4. Server-Sent Events (SSE / Telemetry Broadcast)
Pushed by the server to inform the client of state updates.

- **State Transition Event**:
  ```json
  {
    "event": "state_change",
    "data": {
      "state": "TtsSpeaking",
      "sessionId": 12
    }
  }
  ```
- **STT Transcribed Speech Event**:
  ```json
  {
    "event": "stt_completed",
    "data": {
      "text": "bật đèn phòng khách",
      "sessionId": 12
    }
  }
  ```
- **System Telemetry Event** (e.g. sent periodically):
  ```json
  {
    "event": "system_status",
    "data": {
      "cpuUsage": 8,
      "memoryUsage": 52428800,
      "activeConnections": 1,
      "models": {
        "llm": "loaded",
        "stt": "ready",
        "tts": "ready"
      }
    }
  }
  ```

---

## 5. Backward Compatibility & Stdin/Stdout Legacy IPC

To support existing terminal tooling, regression tests, and local headless integrations:
1. **Stdout Writer Loop**: Keep stdout command reading and JSON-line parsing.
2. **Shared State**: The stdin/stdout loop and the WebSocket server must access the exact same `Arc<AppState>`, ensuring database mutations, memory consolidation, and LLM model-swapping take effect globally regardless of the access channel.
