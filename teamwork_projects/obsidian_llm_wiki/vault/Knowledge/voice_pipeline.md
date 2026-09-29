---
title: "voice_pipeline"
tags:
  - liva/knowledge
  - liva/voice
  - liva/rust-native
author: "LIVA Core Architecture Team"
last_update: "2026-09-29T11:20:00+07:00"
---

# Knowledge: Voice Pipeline

## Executive Summary

LIVA's real-time duplex voice pipeline is implemented natively in Rust within `liva-native-core/src/webrtc/`, `src/tts/`, and `src/turn_taking.rs`. It replaces all legacy Node.js worker threads (`NemotronWorker.ts`, `VADWorker.ts`, `VADWorkerBridge.ts`) and Python EdgeTTS runtimes with a high-performance WebRTC duplex engine: Sonora AEC3 acoustic echo cancellation, GTCRN STFT speech denoiser, Silero VAD v5, the active `Smart Turn v3.2` two-stage turn gate ($T_{\text{gate}} \le 225\text{ ms}$), native offline TTS synthesis, and Tauri v2 IPC Channel event streaming.

## Full-Duplex WebRTC Architecture

```
┌────────────────────────────────────────────────────────────────────────┐
│                        LIVA Audio Duplex Engine                        │
├────────────────────────────────────────────────────────────────────────┤
│ 1. Frontend AudioWorklet Capture (16 kHz PCM, 512 samples / 32 ms hop) │
│    └─► tauri::ipc::Channel / voice_mic_chunk                           │
├────────────────────────────────────────────────────────────────────────┤
│ 2. Sonora AEC3 (webrtc::aec::SelfEchoCanceller)                        │
│    └─► Subband cancellation of speaker playback from mic feed          │
├────────────────────────────────────────────────────────────────────────┤
│ 3. GTCRN Neural Speech Denoiser (webrtc::denoise::GtcrnDenoiser)       │
│    └─► STFT attenuation of stationary and transient background noise   │
├────────────────────────────────────────────────────────────────────────┤
│ 4. Silero VAD v5 (webrtc::vad::VadEngine)                              │
│    └─► Frame-level speech probability classification                   │
├────────────────────────────────────────────────────────────────────────┤
│ 5. Smart Turn v3.2 Adaptive Gate (crates/liva-core-types & session.rs)  │
│    ├─► ImmediateCutoff (p > 0.92): ~200 ms silence cutoff              │
│    ├─► HesitationWait (0.50 <= p <= 0.92): 200-450 ms adaptive padding │
│    └─► Incomplete (p < 0.50): continue listening                       │
├────────────────────────────────────────────────────────────────────────┤
│ 6. Fast CPU STT Engine (Streaming ONNX CTC / Transducer via ort)       │
│    └─► Zero GPU VRAM contention with LLM                               │
├────────────────────────────────────────────────────────────────────────┤
│ 7. Native TTS Engine Hierarchy & Viseme Dispatcher                     │
│    ├─► tts::vieneu / tts::kokoro / piper local synthesis               │
│    └─► Real-time viseme stream to OffscreenCanvas 3D avatar at 60 FPS  │
└────────────────────────────────────────────────────────────────────────┘
```

## Audio Duplex Subsystems

### 1. Acoustic Echo Cancellation (Sonora AEC3)
- **Component**: `webrtc::aec::SelfEchoCanceller`.
- **Function**: Operates in subband frequency domain to model and subtract the speaker playback signal from the microphone input in real time.
- **Invariant**: Eliminates the "hearing self" feedback loop without clipping the user's speech during barge-in.

### 2. Neural Speech Denoiser (GTCRN)
- **Component**: `webrtc::denoise::GtcrnDenoiser`.
- **Architecture**: Convolutional recurrent network operating on Short-Time Fourier Transform (STFT) frames.
- **Performance**: Attenuates ambient room noise, mechanical fan hum, and keyboard clicks by up to 25 dB with $<6\text{ ms}$ processing latency per frame.

### 3. Voice Activity Detection & Active Turn Gate (Smart Turn v3.2)
- **Silero VAD v5**: Runs frame-level inference ($32\text{ ms}$ windows) to output voice probability $p_{\text{speech}}$.
- **Active Two-Stage Turn Gate**:
  - Replaces fixed silence timeouts (formerly 700ms) with an adaptive classifier in `crates/liva-core-types/src/lib.rs` (`AdaptiveTurnDecision`):
    - `ImmediateCutoff { probability }`: Triggered when $p > 0.92$, executing a hard turn boundary at $\approx 200\text{ ms}$ of silence.
    - `HesitationWait { probability }`: Triggered when $0.50 \le p \le 0.92$, extending the pause window dynamically to $450\text{ ms}$ to accommodate Vietnamese pause particles ("ừm", "ờ", "thì").
    - `Incomplete { probability }`: Triggered when $p < 0.50$, continuing speech accumulation.
- **Latency Budget**: Total turn gate decision time $T_{\text{gate}} \le 225\text{ ms}$ (200 ms silence + 12 ms ONNX inference).
- **Speculative Prefill**: At $140\text{ ms}$ of silence, intermediate STT tokens are speculatively flushed to `crates/liva-llm` to prefill the KV cache in the background.

### 4. Native TTS Engine Hierarchy & Clause Chunking
- **Clause Chunking**: Tokens from the LLM actor are buffered and sliced along Vietnamese syntactic clause boundaries (commas, periods, semicolons, and conjunctions *và, thì, mà, nhưng, hoặc*), achieving Time-to-First-Sound (TTFS) $< 280\text{ ms}$.
- **Local Synthesis Engines**:
  - **`tts::vieneu` / `tts::kokoro`**: High-quality Vietnamese and bilingual neural synthesis running locally via ONNX Runtime / DirectML.
  - **`piper`**: Ultra-fast local fallback synthesis engine.
  - **`liva-voice/`**: Isolated offline Python utility used strictly for custom voice cloning training datasets, not involved in runtime synthesis.

### 5. Multi-Stage Barge-In Protocol
- **Stage 1 (Soft Ducking)**: When Silero VAD flags the start of user vocalization (`speech_start`), TTS playback volume ducks immediately to 20% while awaiting turn verification.
- **Stage 2 (Hard Interruption)**:
  - If `Smart Turn v3.2` confirms a genuine user turn, `voice_interrupt` executes instantly:
    1. Cancels LLM generation (`cancel_token.store(true)`).
    2. Drops the current TTS audio queue and stops WASAPI playback.
    3. Increments the generation epoch counter, ensuring any in-flight synthesized audio chunks are discarded at both server and client.
  - If the vocalization was brief noise or backchannel feedback ($<3$ words, e.g. "ừm", "vâng"), TTS volume restores to 100% without invalidating LLM state.

### 6. Viseme & Three-VRM Avatar Synchronization
- Real-time 5-band RMS frequency analysis extracts blendshape targets (`aa`, `ih`, `ou`, `ee`, `oh`).
- Viseme events are dispatched directly across `tauri::ipc::Channel<VoiceIpcEvent>` to the desktop shell.
- Frontend rendering runs on an isolated Web Worker via `OffscreenCanvas`, ensuring 60 FPS VRM animation without main-thread UI stuttering.

## Voice Pipeline Invariants

| Invariant | Value | Purpose |
|---|---|---|
| **Audio Format** | 16 kHz Mono PCM16 | Standard WebRTC and ONNX acoustic model contract |
| **Hop Size** | 512 samples ($32\text{ ms}$) | Low-latency audio packetization from AudioWorklet |
| **$T_{\text{gate}}$ Upper Bound** | $\le 225\text{ ms}$ | Natural conversational responsiveness |
| **Speculative Prefill** | $140\text{ ms}$ mark | Eliminates LLM prefill delay on turn end |
| **Zero Loopback Ports** | IPC Channels only | Complete elimination of internal WebSocket servers |
| **VRAM Protection** | CPU-bound STT (ort) | Keeps GPU memory dedicated to main LLM and VLM |

## Related Notes

- [[Knowledge/liva_architecture|LIVA Architecture]] — Overall system topology and Tauri v2 IPC.
- [[Knowledge/memory_architecture|Memory Architecture]] — Three-tier memory engine, Radix Trie, and SQLite WAL.
- [[Rules/tech_stack|Tech Stack]] — Multi-crate dependencies and build profiles.
- [[Rules/shutdown_chain|Shutdown Chain]] — Deterministic teardown and audio device release.
- [[Knowledge/anti_patterns|Anti-Patterns]] — Prohibited voice patterns in the native Rust architecture.
