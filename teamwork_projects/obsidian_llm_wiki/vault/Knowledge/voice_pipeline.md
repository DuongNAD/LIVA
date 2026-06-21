---
title: "voice_pipeline"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-06-21T02:21:19Z"
---

# Knowledge: Voice Pipeline

## Executive Summary
This document outlines the design and components of the LIVA Voice Pipeline, including the STT/TTS sentient duplex mechanism, real-time audio-driven lip sync, VAD ONNX offloading, and hybrid backup systems.

## Detailed Description
### Sentient Omni-Duplex Pipeline (v23)
The voice pipeline coordinates full-duplex communication with active echo cancellation:
- **Audio Capturing & VAD**: Microphone input is captured on the Frontend with WebRTC Acoustic Echo Cancellation (AEC) and Noise Suppression enabled: `{ echoCancellation: true, noiseSuppression: true }`.
- **WASM Wake-Word / VAD**: The Frontend ONNX WASM wake-word model (~5KB, `hey_liva.onnx`) is always active locally. Audio data is only sent to the Backend via WebSocket after wake-word detection or Alt+Space hotkey activation. During silence, CPU/GPU usage remains at 0%.
- **Nemotron STT (v31 ASR)**: Uses Nemotron 3.5 ONNX CPU-only model (`onnxruntime-node`) on a worker thread (`NemotronWorker.ts`). It completely replaces the legacy WhisperNode. This offloads STT entirely from the GPU to prevent VRAM conflict with the main LLM.
- **Stage 1 Barge-in**: When speech is detected (`speech_start`), TTS volume ducks to 20% immediately while the LLM continues executing. Speculative RAG starts warming vector/KV caches in memory.
- **Stage 2 Barge-in**: When speech ends (`speech_end`), the transcription is processed by `BackchannelDetector`:
  - If it is a backchannel/filler (e.g. "ừm", "ok", cough, <3 words), the TTS volume is restored to 100%, avoiding LLM cancellation and VRAM waste.
  - Otherwise, `agentLoop.bargeIn()` aborts the current LLM stream, kills TTS audio instantly, and truncates memory buffers with an `<interrupted>` XML tag.

### TTS Formatter & Clause Chunking
- Tokens are never sent directly to TTS to avoid robotic stuttering.
- The `TTSFormatter` buffers tokens and splits them into clean clauses based on Vietnamese conjunctions (và, thì, mà, nhưng...), punctuation (, : ; —), or an 8-word overflow. This achieves a Time-to-First-Sound (TTFS) of less than 300ms.

### RMS Audio-Driven Lip-Sync
- Replaces procedural sine-wave lip-sync with real-time audio frequency analysis.
- The Frontend `use3DModel.ts` leverages a Web Audio API `AnalyserNode` (fftSize=256) to perform 5-band RMS frequency analysis.
- Maps analysis results to VRM blendshapes (aa/oh/ee/ih/ou) with lerp smoothing and dead-zone filtering. Procedural sine wave is maintained as a fallback.
- `VRMEngine.vue` exposes `startAudioLipSync(audioCtx, source)` and `stopAudioLipSync()` APIs.

### VAD Worker Thread (VADWorker.ts)
- Neural VAD inference using Silero ONNX runs exclusively in `VADWorker.ts` to prevent Event Loop blocks.
- Communication with the main gateway goes through `VADWorkerBridge.ts` which implements a Ping/Pong watchdog to detect and restart frozen WASM runtimes.

### Hybrid TTS & Reactive Hot-Swap
- **Default**: Python Edge-TTS via asynchronous `safeFetch` to avoid blocking.
- **Offline Fallback**: If Edge-TTS times out or fails (network errors), a circuit breaker triggers, and the system hot-swaps to `KokoroVoiceEngine` (Kokoro-JS ONNX local-first offline fallback, yielding via `setTimeout`).
