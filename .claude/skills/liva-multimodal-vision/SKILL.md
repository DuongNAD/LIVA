---
name: liva-multimodal-vision
description: Orchestrate real-time screen capture, mouse-guided visual ROI recognition, pixel diff tracking, and duplex voice/audio processing. Use when asking visual questions about on-screen content, tracking desktop UI state changes, debugging STT/TTS voice pipelines, or synchronizing 3D avatar lip-sync blendshapes.
---

# LIVA Multimodal Vision

## Workflow

1. **Screen Capture & Context-Aware Region Optimization**:
   - Capture desktop screen frames via `vision:capture` utilizing Windows Graphics Capture (WGC) or `xcap`.
   - Apply Mouse-Guided Vision primitive: when user is interacting with a specific window or game, crop a region centered around cursor (`region_rgb`), reducing VLM token footprint by up to 80% compared to full 4K frame encoding.
   - Compress raw pixel frames into PNG in `tokio::task::spawn_blocking` to avoid async runtime stalls.

2. **Dirty Region & Pixel Diff Tracking**:
   - Register active monitoring regions via `vision:add_region` with bounding boxes `(x, y, width, height)`.
   - Execute `vision:get_changed_regions` with configured `color_tolerance` threshold (default 0.1).
   - Suppress redundant LLM calls when screen pixels are stationary (`is_changed: false`).

3. **Multimodal Visual Question Answering (`vision:ask`)**:
   - Dispatch visual query with encoded PNG payload to local VLM engine (Qwen3-VL / Gemma4).
   - Support zero-shot UI state inspection, OCR extraction, diagram interpretation, and contextual coding assistance.

4. **Realtime Audio Pipeline Orchestration**:
   - Ingest microphone PCM audio chunks and apply Sonora WebRTC AEC3 (Acoustic Echo Cancellation) to eliminate TTS audio playback bleed during barge-in.
   - Transcribe speech with Parakeet STT running Anti-Hallucination filtering (suppressing phantom repetitions and silence drift).
   - Synthesize expressive speech via VieNeu-TTS / Piper with phoneme normalization (`normalizer.rs`, `g2p.rs`).

5. **3D Avatar & Lip-Sync Synchronization**:
   - Extract `avatarControlTags` from TTS stream (`[blink]`, `[nod]`, `[viseme:aa]`, `[emotion:happy]`).
   - Emit synchronized blendshape weight streams over IPC to the Tauri Vue 3 `VRMEngine.vue` component at 60 FPS (<16ms frame render budget).

6. **State Telemetry & Obsidian Logging**:
   - Track frame latency, STT WER, and TTS TTFT metrics in `turn_telemetry`.
   - Log visual diagnostics into `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/Vision - <Session_Title>.md` via `write_markdown`.
   - Adhere strictly to the Obsidian frontmatter standard (`title`, `tags: [liva/knowledge, liva/vision, voice/multimodal]`, `author: "codex"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Realtime streaming and diagnostic inspection. Visual queries and speech synthesis operate under user request triggers.
- **Resource Management**: Capture frames are compressed to PNG in worker threads; raw uncompressed BGRA frames are never streamed over WebSockets or IPC.
- **Privacy Barrier**: Screen capture is strictly local-first. No video feeds or captured desktop screenshots are ever transmitted to external cloud APIs.

## Stop Conditions

Stop and report immediately when:
- Screen capture fails due to missing OS display permissions or hardware capture device disconnect.
- VLM memory allocation exceeds available GPU VRAM, triggering safety fallback to CPU or text-only mode.
- Audio input pipeline detects persistent hardware clipping or buffer under-runs.
