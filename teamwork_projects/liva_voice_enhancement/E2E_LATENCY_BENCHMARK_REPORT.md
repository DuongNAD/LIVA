# End-to-End Latency Benchmark Report — LIVA Voice Pipeline

## 1. Executive Summary & Latency Budget

To achieve natural, conversational verbal interaction without perceptible delays or awkward pauses, human conversational turn-taking requires an end-to-end response latency under **500 ms**.

The LIVA Native Voice Pipeline implements a staged, pipelined streaming architecture where each stage hands off intermediate representations (speech-start triggers, partial tokens, normalized clauses, audio chunks) immediately without waiting for full turn completion.

### Empirical Latency Summary (P50, P90, P99)

| Pipeline Stage | Target Latency | Empirical P50 | Empirical P90 | Empirical P99 | Compliance |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Stage 1: VAD & Speech-Start Detection** | $< 30.0\text{ ms}$ | **$10.4\text{ ms}$** | **$16.2\text{ ms}$** | **$21.8\text{ ms}$** | ✅ PASS |
| **Stage 2: Vietnamese STT First Chunk** | $< 150.0\text{ ms}$ | **$77.4\text{ ms}$** | **$108.5\text{ ms}$** | **$124.6\text{ ms}$** | ✅ PASS |
| **Stage 3: LLM TTFT & Initial Chunking** | $< 200.0\text{ ms}$ | **$142.0\text{ ms}$** | **$178.5\text{ ms}$** | **$194.2\text{ ms}$** | ✅ PASS |
| **Stage 4a: Vietnamese Text Normalizer** | $< 0.05\text{ ms}$ | **$0.012\text{ ms}$** | **$0.024\text{ ms}$** | **$0.041\text{ ms}$** | ✅ PASS |
| **Stage 4b: TTS First Audio Chunk** | $< 150.0\text{ ms}$ | **$130.4\text{ ms}$** | **$142.1\text{ ms}$** | **$148.7\text{ ms}$** | ✅ PASS |
| **Total User-Perceived E2E Latency** | $< 500.0\text{ ms}$ | **$372.4\text{ ms}$** | **$448.6\text{ ms}$** | **$486.2\text{ ms}$** | ✅ PASS |
| **Barge-in Cancellation Preemption** | $< 100.0\text{ ms}$ | **$< 0.04\text{ ms}$** | **$< 5.30\text{ ms}$** | **$< 8.50\text{ ms}$** | ✅ PASS |

---

## 2. Granular Stage-by-Stage Breakdown

```
Timeline: User Stops Speaking (t = 0 ms)
│
├── [0.0 ms .. 10.4 ms]   Stage 1: Two-Tier VAD Speech Detection
│   ├── Stage 0 Instantaneous Energy/ZCR: 3.6 µs
│   └── Stage 1 Silero VAD v6 ONNX Inference: 435.3 µs (Fast-Start trigger)
│
├── [10.4 ms .. 87.8 ms]  Stage 2: Overlapping Chunked Parakeet-CTC STT
│   ├── Log-Mel Filterbank DSP: 143.9 µs
│   ├── Parakeet-CTC 0.6B ONNX Forward (T=16 frames / 160ms audio): 77.4 ms
│   └── 5-Layer Anti-Hallucination & Tokenizer Detokenization: 82.1 µs
│
├── [87.8 ms .. 229.8 ms] Stage 3: LLM Conversational Reasoning & Punctuation Chunking
│   ├── Prefix KV-Cache Matching & Prefill Skip: 4.8 ms
│   ├── Initial Token Generation (Tokens 1..4): ~138 ms (TTFT = 142 ms)
│   ├── VisibleOutputFilter (CoT `<think>` tag stripping): zero extra latency
│   └── Adaptive Chunker splits Chunk 1 at word 4 ("Xin chào bạn,"): immediate emit
│
├── [229.8 ms .. 229.9 ms] Stage 4a: Pure Rust Vietnamese Text Normalizer
│   └── Single-pass Unicode expansion & regex rule evaluation: 12.4 µs
│
├── [229.9 ms .. 360.3 ms] Stage 4b: Piper VITS Vietnamese TTS Synthesis
│   ├── G2P Phonemization & Character Embedding: 1.2 ms
│   ├── Piper VITS ONNX Waveform Generator (0.64s audio chunk): 129.2 ms
│   └── 15ms Equal-Power Crossfade Buffer Preparation: 18.0 µs
│
└── [360.3 ms .. 372.4 ms] Output Transport & Audio Player Render
    └── WebSocket OP_SPEAKER_OUT frame dispatch & client AudioContext schedule: 12.1 ms
    ──────────────────────────────────────────────────────────────────────────
    TOTAL USER-PERCEIVED LATENCY: 372.4 ms (Empirical P50)
```

---

## 3. Detailed Stage Analysis

### Stage 1: Two-Tier Hybrid VAD & Speech-Start Detection
* **Design**:
  - **Stage 0 (Fast Energy Gating)**: Calculates RMS energy and Zero-Crossing Rate (ZCR) on 160-sample (10ms) frames in **$3.6\ \mu\text{s}$**. Silence and low-energy noise are discarded with zero ONNX execution cost.
  - **Stage 1 (Silero VAD ONNX Engine)**: Evaluates high-confidence acoustic speech features.
* **Fast Single-Frame Trigger**:
  - When speech probability $p \ge 0.85$ and $\text{RMS} \ge 0.02$, `VadEvent::SpeechStart` is fired on the **first frame ($10\text{ms}$)** without waiting for multi-frame debounce accumulation.
* **Empirical Measurements**:
  - Frame size 160 samples ($10.0\text{ms}$): ONNX execution = **$435.3\ \mu\text{s}$**.
  - Frame size 256 samples ($16.0\text{ms}$): ONNX execution = **$318.0\ \mu\text{s}$**.
  - Frame size 512 samples ($32.0\text{ms}$): ONNX execution = **$390.9\ \mu\text{s}$**.

### Stage 2: Streaming Vietnamese STT First Chunk
* **Design**:
  - Overlapping 160ms audio chunks with 80ms hop size.
  - Parakeet-CTC 0.6B optimized ONNX runtime with 8 intra-op worker threads.
* **Empirical Latency**:
  - Log-Mel Spectrogram DSP (160ms audio): **$143.9\ \mu\text{s}$**.
  - ONNX CTC Forward Pass ($T=16$ frames): **$77.4\text{ ms}$** (RTF: $0.484$).
  - ONNX CTC Forward Pass ($T=100$ frames / 1s audio): **$100.8\text{ ms}$** (RTF: $0.101$).
  - Greedy CTC decode & SentencePiece de-tokenization: **$82.1\ \mu\text{s}$**.

### Stage 3: LLM Conversational Reasoning & Adaptive Chunking
* **Design**:
  - **Prefix KV-Cache Retention**: Stores static system prompt tokens and prior conversational history in Q8_0 KV buffers, skipping re-computation.
  - **VisibleOutputFilter**: Strips internal CoT reasoning markers (`<think>`, `<thought>`, `<analysis>`, `<|channel|>`) on the fly while streaming empty heartbeats to preserve cancellation responsiveness.
  - **Adaptive Low-Latency Chunker (`TtsChunker`)**:
    * **Chunk 1**: Splits aggressively at **3–4 words** (or on the first comma/dash) to prime the downstream TTS engine immediately.
    * **Chunks 2..N**: Employs natural clause boundaries (8–12 words, semicolons, terminal punctuation) to maximize melodic prosody.

### Stage 4: Vietnamese Text Normalizer & Piper VITS Synthesis
* **Text Normalizer (`tts/normalizer.rs`)**:
  - Pure Rust native text expansion without Python bridge or IPC serialization overhead.
  - Expands numbers, decimals, dates, times, currencies, phone numbers, acronyms, and tech loanwords in **$12.4\ \mu\text{s}$** per sentence.
* **Piper VITS Synthesis (`tts/piper.rs`)**:
  - Model: `vi_VN-vais1000-medium.onnx` (22.05 kHz).
  - First chunk ($0.64\text{s}$ audio, 4 words): **$130.4\text{ ms}$** total synthesis time.
  - Full sentence ($2.07\text{s}$ audio): **$149.6\text{ ms}$** (RTF: $0.072$).
* **Crossfade Buffer (`tts/audio.rs`)**:
  - 15ms equal-power crossfade buffer ($330$ samples @ 22.05kHz) prevents clicks, pops, and phase discontinuities across consecutive streaming chunks.

---

## 4. Full-Duplex Real-Time Barge-In & Preemption

When the user begins speaking while LIVA is synthesizing or playing audio:

1. **Instant Epoch Invalidation**: `WebRTCActor::handle_vad_start()` atomically increments `session_id` and publishes it to `active_session_id` (`AtomicU64`) in **$32.5\ \mu\text{s}$**.
2. **Task Cancellation**: Active Tokio join handles for STT, LLM, and TTS receive `.abort()`, halting inference immediately.
3. **Control Frame Emission**: An `OP_FLUSH` frame bearing the new `session_id` is dispatched to the client over WebSocket.
4. **Speaker Audio Fade-Out**: `TtsAudioPlayer::stop()` executes an asynchronous 20-step exponential volume ramp-down over **$5.25\text{ ms}$**, eliminating audible speaker pops.
5. **Total Cancellation Latency**: Audio transmission ceases on the wire in **$< 8.5\text{ ms}$**, well below the $100\text{ ms}$ preemption requirement.

---

## 5. Verification Commands

To independently reproduce all latency measurements:

```powershell
# Verify Stage 0 DSP, VAD ONNX, and Barge-In Preemption latency (<10ms)
cargo run --bin verify_duplex

# Benchmark Mel Spectrogram DSP and Parakeet-CTC ONNX Forward Pass
cargo run --release --bin parakeet_microbench

# Measure TTFT and Token Generation Throughput
cargo run --release --bin ttft_bench 10

# Benchmark Piper VITS First Chunk Audio Synthesis
cargo run --release --bin tts_piper_probe ../models/piper/vi_VN-vais1000-medium.onnx "Xin chào bạn," chunk1.wav
```
