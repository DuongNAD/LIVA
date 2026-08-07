---
title: "voice_pipeline"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-08-05T15:07:00+07:00"
---

# Knowledge: Voice Pipeline

## Executive Summary
This document outlines the design and components of the LIVA Voice Pipeline, including the STT/TTS sentient duplex mechanism, real-time audio-driven lip sync, VAD ONNX offloading, and hybrid backup systems.

## Rust Runtime Delta — 2026-07-23

The detailed section below describes the historical target architecture. The production runtime is
now the unified Rust core:

- STT, LLM/vision and TTS execute locally; the voice hot path is coordinated by
  `webrtc::pipeline::WebRTCActor`.
- Standalone and Tauri both bind `websocket::WebSocketServer` against the same `AppState`.
  `VoiceRuntimeComponents` loads VAD, GTCRN, SmartTurn and AEC once for either entry point.
- Browser capture uses `AudioWorkletNode("liva-mic-capture")`. The worklet aggregates 512
  samples per transferable frame, so the client hop is 32 ms at 16 kHz instead of the former
  2048-sample/128 ms `ScriptProcessorNode` buffer.
- Wake-worker initialization is single-flight. Partial microphone/AudioContext startup failures
  release acquired resources, and a lifecycle generation prevents an in-flight permission request
  from resurrecting the pipeline after `stopPipeline()`.
- The wake worker only segments candidate utterances. It sends `OP_WAKE_PROBE`; Rust performs
  classifier/STT verification and returns `wake_word_triggered` or `wake_probe_rejected`.
  The former browser RMS MLP, weights JSON and browser ONNX artifact were removed on 2026-07-31.
- Wake probes use a fail-closed two-tier decision. The promoted owner-calibrated classifier may
  directly wake above `LIVA_WAKE_THRESHOLD`; lower scores still require exact STT phrase matching.
  Do not add a second hard-coded direct threshold: it invalidates the benchmark/selection gate.
  The response always returns the raw score, transcript and accepted/rejected outcome; diagnostics
  displays that result instead of silently remaining `PASSIVE`.
- `wake_liva_en_v2.onnx` was retrained on 2026-08-05 with 20 owner hard negatives and promoted only
  after the matrix selector chose `fleurs_medium`. At threshold 0.58 its independent holdout result
  was 4/4 owner positives, one false positive and 0.7685 FPPH over 1.3013 negative hours. This is a
  production beta, not enough field evidence for a broad accuracy claim.
- The 2026-08-02 pre-personalization result (1/24 owner positives at threshold 0.58) is superseded by
  that retraining. Runtime observations at 0.595 and 0.641 on the owner's live microphone confirm
  why the obsolete 0.90 safety cap must not override the evaluated 0.58 threshold.
- A rejected core probe is acknowledged back to `LivaWakeWorker` and releases its cooldown
  immediately. An accepted probe keeps cooldown to prevent duplicate activation.
- Personalization requires both owner positives and owner hard negatives. The preparation step
  splits each class by original recording before replication and injects separate 8xxxxx/9xxxxx
  ranges; training fails closed when the negative enrollment directory is missing or undersized.
- Public negative augmentation pins CC BY 4.0 revisions of Vietnamese FLEURS, Speech Commands v2
  and MUSAN. It canonicalizes mono PCM16/16 kHz, filters the exact wake phrase, rejects clips below
  0.5 seconds, deduplicates audio and keeps speaker/source groups in one split. Five isolated
  control/FLEURS/Commands/hybrid variants can be trained, but public data never substitutes for
  owner hard negatives or the one-hour real ambient benchmark. Candidate selection never copies
  an artifact into `models/`; production promotion remains a manual, fail-closed decision.
- The widget reconnects its local gateway with bounded exponential backoff and waits for voice
  cleanup before reconnecting. Unmount disables and clears reconnect timers.
- Turn cancellation uses a generation epoch. Speaker PCM carries that epoch, control frames have a
  priority queue, and stale audio is rejected at both server and client.
- LLM tokens are clause-buffered before TTS. Runtime synthesis fallback is
  VieNeu → Piper → Kokoro, with cancellation checked between attempts.
- Text and vision generation pass through a shared stream-safe `VisibleOutputFilter`. Internal
  think/analysis/reasoning channels never reach UI, TTS, checkpoint, or memory, including when
  control delimiters are split across tokens or opened by the prompt template.
- Hidden reasoning pieces still invoke an empty cancellation heartbeat. The voice path checks the
  epoch but does not place those heartbeats in the TTS queue.
- Nemotron decoder bootstrap is validated once when `SttEngine` is constructed. Its initial
  decoder tensors are retained as an immutable snapshot; utterance reset clones that snapshot and
  clears encoder caches instead of invoking ONNX again. Model outputs must match the expected
  tensor lengths and contain only finite values before entering the streaming state.
- Model/gateway text is escaped before the widget's `v-html` boundary. The renderer generates only
  line breaks and fixed `data-liva-channel` buttons; untrusted tags and inline handlers cannot
  reach the WebView DOM.
- Automatic router/expert model selection and a resource-leasing `ModelCoordinator` are not yet
  implemented; GGUF hot-swap remains manual and sequential.

## Detailed Description
### Sentient Omni-Duplex Pipeline (v23)
The voice pipeline coordinates full-duplex communication with active echo cancellation:
- **Audio Capturing & VAD**: Microphone input is captured on the Frontend with WebRTC Acoustic Echo Cancellation (AEC) and Noise Suppression enabled: `{ echoCancellation: true, noiseSuppression: true }`.
- **Wake candidate segmentation**: The frontend uses an energy floor only to cut a bounded
  utterance. It never treats energy as keyword confidence. Rust verifies the candidate locally
  with the trained classifier and STT phrase matching before the Widget becomes active.
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
