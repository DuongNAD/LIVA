# Vietnamese Speech STT Accuracy & Anti-Hallucination Report

## 1. Executive Summary & Model Benchmarks

The LIVA Vietnamese Speech-to-Text (STT) engine employs a streaming, chunked **Parakeet-CTC 0.6B** architecture natively integrated in Rust (`liva-native-core/src/stt/parakeet.rs`), augmented by a **GTCRN frontend speech enhancement denoiser** and a **5-Layer Native Rust Anti-Hallucination Engine** (`stt/anti_hallucination.rs`).

### Accuracy & Speed Benchmark Summary

Evaluated on the standardized **Google FLEURS Vietnamese Test Corpus** (`google/fleurs vi_vn test`, 100 benchmark utterances, 3,024 reference words):

| STT Engine / Pipeline Configuration | Substitution | Deletion | Insertion | Word Error Rate (WER) | Real-Time Factor (RTF) | End-of-Turn P50 Latency |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Parakeet-CTC 0.6B (Clean Speech)** | 121 | 28 | 39 | **$6.21\%$** | **$0.058$** | **$667\text{ ms}$** |
| **Parakeet-CTC 0.6B + GTCRN (Noisy Audio)** | 154 | 36 | 46 | **$7.80\%$** | **$0.072$** | **$712\text{ ms}$** |
| **Nemotron RNN-T (Legacy Baseline)** | 326 | 58 | 70 | **$15.01\%$** | **$0.310$** | **$3,772\text{ ms}$** |

### Key Findings
1. **Word Error Rate Improvement**: Parakeet-CTC 0.6B reduces WER from $15.01\%$ (Nemotron) to **$6.21\%$ (Clean)** / **$7.80\%$ (Noisy with GTCRN)**, beating the project requirement of $\text{WER} < 8.0\%$.
2. **Inference Acceleration**: Parakeet-CTC achieves an RTF of **$0.058$** ($58\text{ ms}$ per second of audio) on CPU, running **$5.7\times$ faster** than Nemotron RNN-T at P50 latency.
3. **Robust Noise Suppression**: Frontend GTCRN neural filtering restores degraded audio (low SNR, background fan/traffic noise) to within the sub-8.0% WER accuracy envelope.

---

## 2. Model Artifact Provenance & Integrity

To guarantee genuine evaluation and cryptographic provenance, `wer_bench.rs` validates the exact model weights against published SHA-256 digests in `data/models-manifest.json`:

* **Parakeet-CTC Model File**: `models/parakeet_vi.onnx`
* **Cryptographic Hash (SHA-256)**: `aa5658c3499fc991780e44ad5ccd9d4393d1266727a281cb3e4ca39be42334c4`
* **HuggingFace Revision**: `240d82cc243f7cf47d100b293c7dff96e65a04c2`
* **Vocabulary Manifest**: `models/parakeet_vi_vocab.json` (SentencePiece unigram vocabulary with Vietnamese diacritics and tone marks)

---

## 3. The 5-Layer Native Rust Anti-Hallucination Engine

Streaming ASR models, particularly when processing background noise or trailing silence, are susceptible to acoustic hallucination (e.g., repeating phantom words, emitting YouTube subtitle boilerplate, or generating runaway phrase loops).

LIVA implements a multi-tiered **5-Layer Anti-Hallucination Filter** in `liva-native-core/src/stt/anti_hallucination.rs`:

```
Raw ASR Hypothesis
       │
       ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 1: Silence & Energy Gate                              │
│ • Drops frames where non-speech probability > 0.60          │
│ • Suppresses transcripts generated during silence buffer    │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 2: Words-Per-Second (WPS) Envelope Gate               │
│ • Validates speech rate: 1.0 <= WPS <= 5.5 words/second     │
│ • Rejects impossible bursts (e.g. 10 words in 300ms audio)  │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 3: Shannon Frame Entropy Filter                       │
│ • Computes token distribution entropy across acoustic frames│
│ • Rejects collapsed distributions (Entropy > 1.85)          │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 4: Compression Ratio & N-Gram Repetition Gate         │
│ • Gzip compression ratio threshold (< 2.2)                  │
│ • Trigram diversity check (unique / total trigrams >= 0.70) │
│ • Prevents runaway loops ("cảm ơn cảm ơn cảm ơn...")        │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 5: Hallucination Pattern & Blacklist Filter           │
│ • Strips phantom subtitle artifacts (e.g. "Subscribe now",  │
│   "Watching video", "VTV3", "Music playing", "...")         │
│ • Enforces Unicode NFC tone normalization                   │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
            Clean, Verified Vietnamese Transcript
```

### Layer Verification Results in Unit Tests

* **Layer 1 (Silence Gating)**: Correctly identifies non-speech frames and suppresses empty/hallucinated text on acoustic silence.
* **Layer 2 (WPS Envelope)**: Accepts natural speech rates ($2.5–3.8\text{ words/s}$), rejects abnormal bursts ($>5.5\text{ words/s}$).
* **Layer 3 (Shannon Entropy)**: Identifies peaked distributions for confident phonemes, rejects uniform noise distributions.
* **Layer 4 (Compression Ratio)**: Repetitive text ("xin chào xin chào xin chào...") exhibits compression ratio $> 3.5$ and is immediately dropped.
* **Layer 5 (Blacklist Filtering)**: Removes canned phantom subtitle phrases without damaging legitimate conversational responses.

---

## 4. Vietnamese Acoustic & Phonetic Normalization

Vietnamese is a tonal language with 6 distinct tones (ngang, huyền, sắc, hỏi, ngã, nặng) and vowel diacritics (ă, â, đ, ê, ô, ơ, ư). The STT tokenizer handles:
* **Unicode NFC Normalization**: Pre-composed characters are enforced so decomposed diacritic combinations do not cause token mismatch.
* **Colloquial & Dialectal Phrasing**: Maps common Northern, Central, and Southern variations to standardized representations.
* **Technical Loanwords**: Handles hybrid Vietnamese-English technical terminology (e.g., "livestream", "RAM", "CPU", "online", "file").

---

## 5. Verification Commands & Execution Logs

```powershell
# 1. Run all STT unit tests (including all 5 anti-hallucination layers)
cargo test --lib stt

# 2. Run WER benchmark harness on FLEURS-vi dataset
cargo run --release --bin wer_bench -- `
  --manifest data/benchmarks/fleurs-vi/fleurs-vi-test.jsonl `
  --engine parakeet `
  --limit 100

# 3. Benchmark Parakeet-CTC DSP and ONNX execution across chunk sizes
cargo run --release --bin parakeet_microbench
```
