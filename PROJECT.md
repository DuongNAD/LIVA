# Project: Voice I/O Optimization

## Architecture
This project optimizes the Voice Input (STT) and Voice Output (TTS) pipelines in LIVA for natural sentence-by-sentence streaming, low latency, and GPU acceleration under a strict combined VRAM footprint of 1.2 GB.

### Module Boundaries
1. **liva-ai-engine**:
   - `whisper_stt_server.py`: Runs faster-whisper on CUDA GPU (Port 8101).
2. **liva-gateway**:
   - `KokoroVoiceEngine.ts` & `KokoroWorker.ts`: Local-first Kokoro TTS executing on GPU (using ONNX Runtime Node).
   - `VoiceOrchestrator.ts`: Standard voice flow integration.
3. **tests**:
   - `voice_io_benchmark.ts`: Automated voice pipeline benchmark script.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | Sentence-by-sentence streaming (R1) | Refactor Kokoro TTS token push & split logic to execute synthesis strictly on sentence boundaries | none | DONE |
| 2 | GPU Acceleration & VRAM Limit (R2) | Configure Whisper STT server & Kokoro worker to run on GPU via CUDA/DirectML within 1.2 GB VRAM | M1 | DONE |
| 3 | Automated Benchmarking (R3) | Implement voice_io_benchmark.ts script measuring latency, accuracy, and VRAM; write Voice_Optimization_Report.md | M1, M2 | DONE |
| 4 | Verification & Audit Gate (R4) | Run benchmark, execute test suites, verify memory limits and run Forensic Auditor checks | M1, M2, M3 | PLANNED |

## Interface Contracts
### Gateway ↔ Whisper STT Server
- REST Endpoint: `POST /v1/audio/transcriptions`
- Input: Form data containing audio file (WAV).
- Output: JSON response containing transcribed text: `{"text": "..."}`.

### Gateway ↔ KokoroWorker
- Worker postMessage:
  - `init`: Load model ID and dtype.
  - `generate`: Synthesize text on GPU.
  - `audio_result`: Return base64 encoded audio.

### Gateway ↔ UI Client
- WebSocket event `ai_audio_chunk`: Sent from gateway to UI client containing base64 audio chunks.

## Code Layout
- `liva-ai-engine/whisper_stt_server.py`
- `liva-gateway/src/services/KokoroVoiceEngine.ts`
- `liva-gateway/src/workers/KokoroWorker.ts`
- `liva-gateway/tests/voice_io_benchmark.ts`
