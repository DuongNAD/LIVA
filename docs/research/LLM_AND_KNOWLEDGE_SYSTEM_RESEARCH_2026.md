# 🔬 Nghiên Cứu Toàn Diện: Open Source LLMs & Wiki Knowledge Systems cho LIVA

> **Date**: 2026-06-10
> **Authors**: LIVA Research Team (AI-Assisted)
> **Target audience**: All LIVA project contributors
> **Scope**: Model selection, inference frameworks, RAG architecture, wiki-LLM integration

---

## Mục Lục (Table of Contents)

- [Executive Summary](#executive-summary)
- [1. LLM Model Evaluation (R1)](#1-llm-model-evaluation-r1)
  - [1.1 Methodology & Criteria](#11-methodology--criteria)
  - [1.2 Model Comparison Table](#12-model-comparison-table-8-families--7-criteria)
  - [1.3 Router Candidates (≤4B, ~2-5.5GB)](#13-router-candidates-4b-25-55gb)
  - [1.4 Expert Candidates (7-14B, ~4-7.5GB)](#14-expert-candidates-7-14b-4-75gb)
  - [1.5 Optimal Pair Assessment](#15-optimal-pair-assessment)
  - [1.6 Vietnamese Language Support Analysis](#16-vietnamese-language-support-analysis)
  - [1.7 Verdict: Gemma 4 E4B + 12B Validation](#17-verdict-gemma-4-e4b--12b-validation)
- [2. Inference Framework Comparison (R2)](#2-inference-framework-comparison-r2)
  - [2.1 Current Architecture Analysis](#21-current-architecture-analysis)
  - [2.2 Framework Comparison Matrix (8 × 10 Criteria)](#22-framework-comparison-matrix-8--10-criteria)
  - [2.3 Speculative Decoding Feasibility](#23-speculative-decoding-feasibility)
  - [2.4 MLX Deep-Dive (macOS)](#24-mlx-deep-dive-macos)
  - [2.5 Migration Risk Assessment](#25-migration-risk-assessment)
  - [2.6 Verdict: Stay with llama.cpp + MLX](#26-verdict-stay-with-llamacpp--mlx)
- [3. RAG Architecture Design (R3)](#3-rag-architecture-design-r3)
  - [3.1 Current LIVA RAG Stack](#31-current-liva-rag-stack)
  - [3.2 RAG Pipeline Architecture](#32-rag-pipeline-architecture)
  - [3.3 Chunking Strategies Comparison](#33-chunking-strategies-comparison)
  - [3.4 Embedding Models Comparison (10 Models)](#34-embedding-models-comparison-10-models)
  - [3.5 Retrieval Methods (Dense/Sparse/Hybrid)](#35-retrieval-methods-densesparsehybrid)
  - [3.6 Wikipedia/Wikidata Integration Feasibility](#36-wikipediawikidata-integration-feasibility)
  - [3.7 Recommended Improvements](#37-recommended-improvements)
- [4. Wiki-LLM Projects Analysis (R4)](#4-wiki-llm-projects-analysis-r4)
  - [4.1 Project Review Table (8 Projects)](#41-project-review-table-8-projects)
  - [4.2 Top 3 Applicable Projects](#42-top-3-applicable-projects-self-rag-crag-wikichat)
  - [4.3 Knowledge Grounding Patterns](#43-knowledge-grounding-patterns)
  - [4.4 Citation & Fact-Checking Approaches](#44-citation--fact-checking-approaches)
  - [4.5 License Compatibility](#45-license-compatibility)
- [5. Integration Roadmap (R5 — Synthesis)](#5-integration-roadmap-r5--synthesis)
  - [5.1 Strategic Recommendations Summary](#51-strategic-recommendations-summary)
  - [5.2 Phase 1: Foundation (3-4 weeks) — Quick Wins](#52-phase-1-foundation-3-4-weeks--quick-wins)
  - [5.3 Phase 2: Enhancement (6-8 weeks) — Quality Leap](#53-phase-2-enhancement-6-8-weeks--quality-leap)
  - [5.4 Phase 3: Scale (8-12 weeks) — Full Knowledge System](#54-phase-3-scale-8-12-weeks--full-knowledge-system)
  - [5.5 Phase 4: Autonomous Evolution (6-10 weeks)](#55-phase-4-autonomous-evolution-6-10-weeks)
  - [5.6 Total Effort & Timeline Overview](#56-total-effort--timeline-overview)
- [6. Risk Matrix](#6-risk-matrix)
- [7. Decision Matrix — Quick Reference](#7-decision-matrix--quick-reference)
- [Appendix](#appendix)
  - [A. Benchmark Data Sources & Caveats](#a-benchmark-data-sources--caveats)
  - [B. LIVA Architecture Reference](#b-liva-architecture-reference)
  - [C. Glossary](#c-glossary)

---

## Executive Summary

Báo cáo này tổng hợp kết quả nghiên cứu từ 4 hướng song song về **LLM models, inference frameworks, RAG architecture,** và **wiki-LLM integration** — nhằm đề xuất phương án nâng cấp knowledge system cho LIVA, một hybrid-intelligence AI desktop assistant chạy trên Windows và macOS.

### Kết luận chính

| Lĩnh vực | Kết luận | Hành động |
|-----------|----------|-----------|
| **LLM Models** | **Gemma 4 E4B (Router, 5.3GB) + Gemma 4 12B (Expert, 6.7GB)** là cặp tối ưu nhất cho 8GB VRAM | **Giữ nguyên** — không thay đổi |
| **Inference Framework** | **llama.cpp + MLX dual-backend** đã là kiến trúc tối ưu | **Giữ nguyên** — nâng cấp incremental |
| **RAG Architecture** | Nền tảng hiện có (Hybrid RRF search) rất mạnh, thiếu ingestion pipeline và Vietnamese support | **Xây mới** ingestion + nâng cấp FTS5/embedding |
| **Wiki-LLM** | **CRAG** (3-tier scoring) + **Self-RAG** (reflection tokens) phù hợp nhất | **Tích hợp** patterns vào PromptBuilder |

### Top 5 Recommendations (ưu tiên cao nhất)

1. **CRAG 3-Tier Scoring** tại PromptBuilder — phân loại CORRECT/AMBIGUOUS/INCORRECT thay vì binary pass/fail (1 week)
2. **Memory Provenance Metadata** — thêm citation vào `<context_memory>` XML (0.5 week)
3. **FTS5 Unicode61 Tokenizer** — thay thế Porter stemmer để hỗ trợ Vietnamese search (1 hour migration)
4. **Document Chunker** — xây RAG ingestion pipeline chuẩn (2-3 days)
5. **Multilingual Embedding Migration** — chuyển sang `multilingual-e5-small` cho Vietnamese RAG (2-3 days)

### Memory/VRAM Budget Summary

```
┌─────────────────────────────────────────────────────┐
│            LIVA VRAM BUDGET (8GB Target)              │
├─────────────────────────────────────────────────────┤
│ Router (Gemma 4 E4B QAT 4-bit)     │   5.3 GB      │
│ Expert (Gemma 4 12B QAT 4-bit)     │   6.7 GB      │
│ KV Cache + System overhead          │  ~1.3 GB      │
│ ────────────────────────────────────│──────────     │
│ Chỉ 1 model active tại 1 thời điểm │  ≤ 8.0 GB ✅  │
│                                     │               │
│ Embedding (ONNX CPU, 0 VRAM)       │   ~90 MB RAM  │
│ FTS5 Index (SQLite, 0 VRAM)        │   ~240 MB disk│
│ Vector Store (INT8, 0 VRAM)        │   ~115 MB disk│
└─────────────────────────────────────────────────────┘
```

---

## 1. LLM Model Evaluation (R1)

### 1.1 Methodology & Criteria

**Phương pháp đánh giá**: So sánh 8+ model families dựa trên 7 tiêu chí định lượng và định tính, lọc qua 4 bước loại trừ theo constraints của LIVA.

**Hardware Constraints cứng** (từ `AI_CONTEXT.md` line 155):
- Target: **8GB VRAM** (discrete GPU) hoặc **8-10GB unified memory** (Apple Silicon)
- Router model PHẢI ≤ ~5.5GB (chừa headroom cho KV-cache + system)
- Expert model PHẢI ≤ ~7.5GB (trên 8GB unified) hoặc ≤ ~8GB (trên 10GB+)
- **Chỉ 1 model trên VRAM** tại bất kỳ thời điểm — Sequential Hot-Swap architecture

**Tiêu chí đánh giá (7 criteria)**:

| # | Tiêu chí | Trọng số | Đo lường bằng |
|---|----------|----------|---------------|
| 1 | GGUF Memory Fit | Critical | GGUF Q4_K_M/QAT size (GB) |
| 2 | Benchmark Quality | High | MMLU, HumanEval, GSM8K, IFEval |
| 3 | Tool/Function Calling | Critical | Native support, XML format parsing |
| 4 | Vietnamese Support | High | Training data composition, community reports |
| 5 | Inference Speed | Medium | tok/s trên Apple Silicon (est.) |
| 6 | GGUF Ecosystem | Medium | Official GGUF availability, QAT vs PTQ |
| 7 | Context Window | Low | Tokens (LIVA Router dùng ~2-4K, Expert ~8-32K) |

> **Data Source**: Benchmark scores dựa trên official model cards, Hugging Face Open LLM Leaderboard, LM Arena, và benchmark papers (cutoff ~mid-2025). GGUF sizes là ước tính theo formula `params × bits_per_weight / 8 + overhead`. Quantized versions thường giảm 1-3% so với FP16 baseline.

### 1.2 Model Comparison Table (8 Families × 7 Criteria)

| Model | Params | Type | GGUF Q4 Size (GB) | MMLU | HumanEval | GSM8K | IFEval | Tool Call | Vietnamese | Context |
|-------|--------|------|-------------------|------|-----------|-------|--------|-----------|------------|---------|
| **Gemma 4 E4B** | ~4B (MoE, 2B active) | MoE | ~5.3 (QAT) | ~58 | ~52 | ~68 | ~72 | ✅ Native | ✅ Good | 128K |
| **Gemma 4 12B** | 12B dense | Dense | ~6.7 (QAT) | ~73 | ~65 | ~82 | ~80 | ✅ Native | ✅ Good | 128K |
| **Gemma 3 4B** | 4B | Dense | ~2.5 | ~55 | ~45 | ~60 | ~65 | ⚠️ Limited | ✅ Good | 128K |
| **Gemma 3 12B** | 12B | Dense | ~7.0 | ~71 | ~58 | ~78 | ~75 | ⚠️ Limited | ✅ Good | 128K |
| **Qwen 3 4B** | 4B | Dense | ~2.5 | ~56 | ~50 | ~72 | ~70 | ✅ Good | ⚠️ OK | 32K |
| **Qwen 3 8B** | 8B | Dense | ~4.5 | ~68 | ~62 | ~82 | ~78 | ✅ Good | ⚠️ OK | 128K |
| **Qwen 3 14B** | 14B | Dense | ~8.0 | ~75 | ~68 | ~85 | ~82 | ✅ Good | ⚠️ OK | 128K |
| **Qwen 3 MoE A3B** | ~MoE (3B active) | MoE | ~4.5-5.0† | ~58 | ~48 | ~70 | ~68 | ✅ Good | ⚠️ OK | 128K |
| **Llama 4 Scout** | 17B (MoE) | MoE | ~10-12† | ~74 | ~62 | ~80 | ~78 | ✅ Native | ⚠️ Basic | 128K+ |
| **Mistral Nemo 12B** | 12B | Dense | ~7.0 | ~68 | ~55 | ~75 | ~72 | ⚠️ Limited | ⚠️ Moderate | 128K |
| **Mistral Small 3.1 24B** | 24B | Dense | ~13.5 | ~78 | ~70 | ~85 | ~82 | ✅ Native | ⚠️ Moderate | 128K |
| **DeepSeek R1 Distill 7B** | 7B | Dense | ~4.0 | ~62 | ~52 | ~76 | ~65 | ❌ Weak | ❌ Weak | 64K |
| **DeepSeek R1 Distill 14B** | 14B | Dense | ~8.0 | ~72 | ~65 | ~85 | ~72 | ❌ Weak | ❌ Weak | 64K |
| **Phi-4 Mini (3.8B)** | 3.8B | Dense | ~2.3 | ~63 | ~62 | ~80 | ~72 | ⚠️ Limited | ❌ Weak | 128K |
| **Phi-4 Medium (14B)** | 14B | Dense | ~8.0 | ~76 | ~72 | ~88 | ~80 | ⚠️ Limited | ❌ Weak | 128K |
| **Yi 1.5 6B** | 6B | Dense | ~3.5 | ~58 | ~42 | ~65 | ~60 | ❌ None | ⚠️ OK (CJK) | 32K |
| **Yi 1.5 9B** | 9B | Dense | ~5.0 | ~67 | ~48 | ~72 | ~65 | ❌ None | ⚠️ OK (CJK) | 32K |
| **SmolLM 2 1.7B** | 1.7B | Dense | ~1.0 | ~35 | ~22 | ~30 | ~40 | ❌ None | ❌ None | 8K |
| **Falcon 3 7B** | 7B | Dense | ~4.0 | ~60 | ~38 | ~55 | ~55 | ❌ None | ❌ None | 8K |

> **†** Llama 4 Scout & Qwen 3 MoE: GGUF cho MoE models chứa tất cả expert weights → tổng size vượt budget 8GB mặc dù active params nhỏ.

### 1.3 Router Candidates (≤4B, ~2-5.5GB)

LIVA Router cần: intent classification, `handoff_to_expert` decision, tool calling (93+ MCP skills), trả lời chat đơn giản.

| Model | Active Params | GGUF Size | MMLU | Tool Call | Vietnamese | Est. Speed† | Verdict |
|-------|--------------|-----------|------|-----------|------------|------------|---------|
| **🏆 Gemma 4 E4B** | ~2B active (MoE) | ~5.3GB (QAT) | ~58 | ✅ Native | ✅ Good | ~45 tok/s | **Best Balance** |
| Qwen 3 4B | 4B dense | ~2.5GB | ~56 | ✅ Good | ⚠️ OK | ~55 tok/s | Runner-up (nhỏ hơn) |
| Phi-4 Mini 3.8B | 3.8B dense | ~2.3GB | ~63 | ⚠️ Limited | ❌ Weak | ~55 tok/s | Benchmark cao, thiếu VN |
| Gemma 3 4B | 4B dense | ~2.5GB | ~55 | ⚠️ Limited | ✅ Good | ~55 tok/s | Predecessor, tool call yếu |
| Qwen 3 MoE A3B | ~3B active | ~4.5-5.0GB | ~58 | ✅ Good | ⚠️ OK | ~40 tok/s | VN yếu hơn Gemma 4 |

> **†** Inference speed ước tính trên Apple Silicon M2/M3 với llama.cpp, prompt ngắn (~100 tokens).

**Phân tích**: Gemma 4 E4B chiếm ưu thế nhờ MoE architecture (2B active cho tốc độ, total expertise cao), native tool calling (Google thiết kế với function calling built-in), Vietnamese support tốt nhất tier, và official QAT GGUF (quality 4-bit tốt hơn post-training quantization). Qwen 3 4B là backup nếu cần Router siêu nhỏ (<3GB).

### 1.4 Expert Candidates (7-14B, ~4-7.5GB)

LIVA Expert cần: deep reasoning, phân tích phức tạp, code generation, tool calling mở rộng.

| Model | Params | GGUF Size | MMLU | HumanEval | GSM8K | Tool Call | Vietnamese | Verdict |
|-------|--------|-----------|------|-----------|-------|-----------|------------|---------|
| **🏆 Gemma 4 12B** | 12B | ~6.7GB (QAT) | ~73 | ~65 | ~82 | ✅ Native | ✅ Good | **Best Overall** |
| Qwen 3 8B | 8B | ~4.5GB | ~68 | ~62 | ~82 | ✅ Good | ⚠️ OK | Strong alternative |
| Qwen 3 14B | 14B | ~8.0GB | ~75 | ~68 | ~85 | ✅ Good | ⚠️ OK | Vượt budget (cần Q3) |
| Mistral Nemo 12B | 12B | ~7.0GB | ~68 | ~55 | ~75 | ⚠️ Limited | ⚠️ Moderate | Kém Gemma 4 mọi mặt |
| Gemma 3 12B | 12B | ~7.0GB | ~71 | ~58 | ~78 | ⚠️ Limited | ✅ Good | Predecessor |
| DeepSeek R1 7B | 7B | ~4.0GB | ~62 | ~52 | ~76 | ❌ Weak | ❌ Weak | Reasoning-only |
| Phi-4 Medium 14B | 14B | ~8.0GB | ~76 | ~72 | ~88 | ⚠️ Limited | ❌ Weak | Quá lớn + thiếu VN |

**Phân tích**: Gemma 4 12B QAT chỉ 6.7GB — vừa khít 8GB budget, chừa ~1.3GB cho KV-cache. Benchmark balanced trên mọi mặt, native tool calling, Vietnamese tốt, và official QAT GGUF. Qwen 3 8B (4.5GB) là Plan B nếu cần Expert nhỏ hơn cho Mac 8GB unified memory hạn chế.

### 1.5 Optimal Pair Assessment

**Bước loại trừ** từ 18 models xuống còn 2:

1. **Hardware constraint** → loại Llama 4 Scout (~12GB), Mistral Small 24B (~13.5GB), Qwen 3 14B Q4 (~8GB borderline), Phi-4 Medium (~8GB)
2. **Tool calling requirement** → loại DeepSeek R1 Distills (reasoning-only), Yi models, SmolLM 2, Falcon (không tool support)
3. **Vietnamese requirement** → loại Phi-4 family (English/code focus), DeepSeek (Chinese/English)
4. **GGUF ecosystem** → Gemma 4 (official QAT) và Qwen 3 (official) đều OK

**Final ranking**:

| Rank | Pair | Tổng VRAM (max 1 model) | Điểm mạnh | Điểm yếu |
|------|------|-------------------------|-----------|-----------|
| 🏆 1 | **Gemma 4 E4B + Gemma 4 12B** | 5.3 / 6.7 GB | Best all-round: tool call + VN + benchmark + ecosystem fit | Lớn hơn alternatives |
| 🥈 2 | Qwen 3 4B + Qwen 3 8B | 2.5 / 4.5 GB | Nhỏ gọn, swap nhanh, reasoning tốt | VN support kém hơn |
| 🥉 3 | Gemma 4 E4B + Qwen 3 8B | 5.3 / 4.5 GB | Router tốt + Expert nhỏ hơn | Mixed-family, prompt format khác nhau |

### 1.6 Vietnamese Language Support Analysis

| Model Family | Vietnamese Training Data | Assessment | Evidence |
|-------------|------------------------|------------|---------|
| **Gemma 4** | ✅ Google multilingual corpus lớn, bao gồm Vietnamese | **Good** — hiểu + trả lời tự nhiên | Google đầu tư mạnh multilingual |
| **Qwen 3** | ⚠️ Chinese-first, English second, Vietnamese limited | **OK** — cơ bản nhưng không native | Alibaba focus CJK |
| **Llama 4** | ⚠️ English-centric, multilingual as secondary | **Basic** — nắm ý nhưng trả lời không tự nhiên | Meta focus English |
| **Phi-4** | ❌ English + code chính, Vietnamese rất ít | **Weak** — critical blocker cho LIVA | Microsoft focus English/code |
| **DeepSeek** | ❌ Chinese + English only | **Weak** — critical blocker | Chinese AI lab |
| **Yi** | ⚠️ CJK focus (Chinese, Japanese, Korean) | **OK for CJK** — Vietnamese không phải ưu tiên | 01.AI focus CJK |

> **Lưu ý**: Vietnamese support assessment dựa trên training data composition, không phải benchmark cụ thể (ViMMRC, VLSP). Chưa có benchmark Vietnamese standardized cho hầu hết models.

### 1.7 Verdict: Gemma 4 E4B + 12B Validation

**✅ Xác nhận: Gemma 4 E4B + Gemma 4 12B QAT là cặp model tối ưu nhất cho LIVA.**

Cả R1 (Model Evaluation) và R4 (Wiki-LLM Roadmap) đều đồng thuận:
- R1: "Gemma 4 E4B QAT 4-bit (~5.3GB) + Gemma 4 12B QAT 4-bit (~6.7GB) vẫn là cặp model tối ưu nhất"
- R4: "Giữ nguyên Gemma 4 E4B (Router) + Gemma 4 12B (Expert) — Đủ cho intent routing và deep reasoning"

| Tiêu chí | Gemma 4 Pair | Đối thủ gần nhất (Qwen 3) | Winner |
|-----------|-------------|---------------------------|--------|
| Memory fit (8GB) | 5.3+6.7GB | 2.5+4.5GB | Tie |
| Benchmark (MMLU) | ~58 / ~73 | ~56 / ~68 | **Gemma 4** |
| Tool calling | ✅ Native built-in | ✅ Good | Tie |
| Vietnamese | ✅ Good | ⚠️ OK | **Gemma 4** |
| GGUF quality | ✅ Official QAT | ✅ Official PTQ | **Gemma 4** |
| Ecosystem fit | Same family, shared tokenizer | Different families | **Gemma 4** |

**Khuyến nghị**: Không thay đổi model pair. Monitor: Qwen 3 8B+ releases mới, Llama 4 small variants. Khi hardware upgrade (12-16GB VRAM), consider Gemma 4 27B Q3 hoặc Qwen 3 14B Expert.

---

## 2. Inference Framework Comparison (R2)

### 2.1 Current Architecture Analysis

LIVA đã xây dựng một **dual-backend inference engine** tinh vi:

```
┌──────────────────────────────────────────────────────────────────┐
│                    LIVA INFERENCE ARCHITECTURE                    │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────────────┐     gRPC (port 8100)                    │
│  │   liva-gateway       │◄──────────────────────┐                │
│  │   (Node.js/TS)       │     protobuf           │                │
│  │                      │     Chat, SwapModel,   │                │
│  │   ModelOrchestrator  │     Embed, Health       │                │
│  │   NativeIPCClient    │                         │                │
│  └──────────┬───────────┘                         │                │
│             │                                      │                │
│   ┌─────────▼────────────────────────────────┐    │                │
│   │         EngineFactory                     │    │                │
│   │   ┌──────────────┐  ┌────────────────┐   │    │                │
│   │   │ LivaNative   │  │  LivaMlx       │   │    │                │
│   │   │ Engine       │  │  Engine         │   │    │                │
│   │   │ (llama.cpp)  │  │  (Apple MLX)   │   │    │                │
│   │   │ CUDA/Metal   │  │  Metal native  │   │    │                │
│   │   └──────────────┘  └────────────────┘   │    │                │
│   │              LivaEngineWrapper            │    │                │
│   │         (thread-safe hot-swap proxy)      │    │                │
│   └───────────────────────────────────────────┘    │                │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

**Key metrics đã đo** (macOS Metal, Gemma 4 E4B ↔ 12B):
- Router → Expert swap: **1.2s**
- Expert → Router swap: **2.2s**
- Expert Cooldown TTL: **90-120s** (tránh VRAM thrashing)

### 2.2 Framework Comparison Matrix (8 × 10 Criteria)

| Criteria | llama.cpp ⭐ (Current) | Ollama | vLLM | LM Studio | LocalAI | MLX ⭐ (Current) | TensorRT-LLM | ExLlamaV2 |
|---|---|---|---|---|---|---|---|---|
| **Inference Speed** | ⭐⭐⭐ 40-80 tok/s | ⭐⭐ 30-60 | ⭐⭐⭐ 80-120 CUDA | ⭐⭐ 30-50 | ⭐⭐ 25-50 | ⭐⭐⭐ 50-90 | ⭐⭐⭐ 100-150+ | ⭐⭐⭐ 70-100 |
| **Model Swap Time** | ⭐⭐⭐ 1-3s (mmap) | ⭐ 3-8s | ❌ 10-30s | ❌ 5-15s | ⭐ 3-10s | ⭐⭐⭐ 1-3s | ❌ 30-120s | ⭐⭐ 2-5s |
| **Memory Efficiency** | ⭐⭐⭐ mmap shared | ⭐⭐ Good | ⭐⭐⭐ PagedAttn | ⭐⭐ Std | ⭐⭐ Std | ⭐⭐⭐ Unified | ⭐⭐⭐ Fusion | ⭐⭐⭐ EXL2 |
| **API Compat** (OpenAI) | ⭐⭐⭐ Native | ⭐⭐⭐ Full | ⭐⭐⭐ Full | ⭐⭐⭐ Full | ⭐⭐⭐ Full | ❌ Library only | ⭐⭐ Triton | ⭐ Wrapper |
| **Cross-Platform** | ⭐⭐⭐ All | ⭐⭐⭐ All | ❌ Linux only | ⭐⭐⭐ All | ⭐⭐ Docker | ❌ macOS only | ❌ NVIDIA only | ❌ CUDA only |
| **Hot-Swap** | ⭐⭐⭐ LIVA gRPC | ❌ No API | ❌ None | ❌ GUI only | ⭐ Basic | ⭐⭐⭐ Python swap | ❌ Rebuild | ⭐ Reload |
| **Spec. Decoding** | ⭐⭐⭐ `--draft` | ❌ None | ⭐⭐⭐ Full | ❌ None | ❌ None | ⭐⭐ Experimental | ⭐⭐⭐ Best | ❌ None |
| **Model Format** | ⭐⭐⭐ GGUF | ⭐⭐⭐ GGUF | ⭐⭐ Safetensors | ⭐⭐⭐ GGUF | ⭐⭐ Mixed | ⭐⭐ Safetensors | ⭐⭐ TRT | ⭐⭐⭐ EXL2 |
| **Community** | ⭐⭐⭐ 10K+ commits/yr | ⭐⭐⭐ Huge | ⭐⭐⭐ Enterprise | ⭐⭐ Commercial | ⭐⭐ Smaller | ⭐⭐ Apple-backed | ⭐⭐⭐ NVIDIA | ⭐⭐ Niche |
| **LIVA Integration** | ⭐⭐⭐ **Zero** (đã có) | ⭐⭐ Medium | ❌ High (no macOS) | ❌ Very High | ⭐⭐ Medium | ⭐⭐⭐ **Zero** (đã có) | ❌ Very High | ⭐ High |

**LIVA Fit Score** (tổng hợp):

| Framework | Score | Lý do chính |
|-----------|-------|-------------|
| **llama.cpp** | **10/10** | Đã integrated sâu, hot-swap 1.2s, cross-platform |
| **MLX** | **9/10** | Đã integrated, optimal cho Apple Silicon |
| Ollama | 4/10 | Wrapper overhead, thiếu hot-swap API |
| LocalAI | 3/10 | Docker-primary, thiếu hot-swap |
| vLLM | 2/10 | Linux-only, no macOS Metal |
| ExLlamaV2 | 2/10 | CUDA-only, no macOS |
| TensorRT-LLM | 1/10 | NVIDIA-only, server deployment |
| LM Studio | 1/10 | GUI-dependent, cannot embed |

### 2.3 Speculative Decoding Feasibility

**Câu hỏi**: Có thể dùng Router (Gemma 4 E4B, 5.3GB) làm draft model cho Expert (Gemma 4 12B, 6.7GB)?

**Yêu cầu kỹ thuật**:
- ✅ Same tokenizer — Gemma 4 family chia sẻ tokenizer
- ✅ Draft nhỏ hơn target — 4B << 12B
- ❌ **Cả 2 model phải ở trong VRAM đồng thời** — 5.3 + 6.7 = ~12GB

| Platform | Khả thi? | Ghi chú |
|----------|----------|---------|
| Windows 8GB VRAM | ❌ Không | OOM crash, vi phạm Dual Model ban |
| Windows 12GB+ VRAM | ⭐⭐ Khả thi | llama-server `-md` flag, tight budget |
| macOS 16GB+ Unified | ⭐⭐⭐ **Rất khả thi** | MLX unified memory, cả 2 fit thoải mái |
| macOS 8GB Unified | ❌ Không | Swap to disk quá chậm |

**Khuyến nghị**: Implement speculative decoding trên macOS 16GB+ khi `mlx-lm` API stabilize. Estimated speedup: ~1.5-2x Expert inference. Implementation path: env flag `LIVA_SPECULATIVE_IN_EXPERT_MODE=true`, giữ Router làm draft khi swap sang Expert.

### 2.4 MLX Deep-Dive (macOS)

LIVA đã có `LivaMlxEngine` fully integrated (`liva_native_engine.py` lines 649-769), hiện dùng làm macOS Expert backend.

| Feature | MLX | llama.cpp Metal |
|---------|-----|-----------------|
| Memory Model | Unified (zero-copy GPU) | mmap + Metal buffer copy |
| Model Loading | ~1-2s (lazy eval) | ~1-3s (mmap + GPU offload) |
| Inference Speed | 50-90 tok/s (M-series) | 40-80 tok/s (M-series) |
| VRAM Management | Automatic (unified) | Manual n_gpu_layers |
| Hot-Swap | Fast (Python object swap + gc) | Fast (mmap reload) |

**MLX Optimization Opportunities**:
1. Speculative decoding trên unified memory (cả 2 models fit)
2. KV cache optimization (MLX lazy evaluation)
3. Benchmark per-model: chọn backend tối ưu theo từng model architecture

### 2.5 Migration Risk Assessment

| Target Framework | Effort | Files Affected | Timeline | Risk Level |
|-----------------|--------|---------------|----------|------------|
| **Stay (llama.cpp + MLX)** | **Zero** | 0 | 0 | **None** |
| Ollama | Medium | ~8 files | 2-3 weeks | Hot-swap regression |
| LocalAI | Medium | ~8 files | 2-3 weeks | Hot-swap loss |
| vLLM | Very High | ~15+ files | 2+ months | macOS unsupported |
| ExLlamaV2 | High | ~12 files | 1-2 months | macOS unsupported |
| TensorRT-LLM | Very High | ~15+ files | 3+ months | macOS unsupported |

### 2.6 Verdict: Stay with llama.cpp + MLX

**✅ Khuyến nghị: Giữ nguyên kiến trúc hiện tại** — cả R2 (Frameworks) và R4 (Wiki-LLM Roadmap) đều đồng thuận.

- R2: "LIVA NÊN GIỮ kiến trúc hiện tại (llama.cpp Native + MLX dual-backend)"
- R4: "Giữ llama-server (C++) — Mature, GGUF-native, Metal/CUDA/Vulkan support"

**Incremental improvements** (không cần migration):

| Priority | Improvement | Effort | Impact |
|----------|------------|--------|--------|
| P1 | Upgrade llama.cpp binary (Flash Attention 2) | Low | +10-15% tok/s |
| P1 | Upgrade `mlx-lm` package | Low | Better model coverage |
| P2 | Speculative decoding MLX (macOS 16GB+) | Medium | +50-100% Expert tok/s |
| P2 | KV cache reuse in `LivaMlxEngine` | Medium | Faster repeat queries |
| P3 | Benchmark llama.cpp vs MLX per model | Low | Optimal backend selection |

---

## 3. RAG Architecture Design (R3)

### 3.1 Current LIVA RAG Stack

LIVA đã có một nền tảng RAG **surprisingly mature**. Dưới đây là inventory các components hiện tại:

| Component | File | Status | Chi tiết |
|-----------|------|--------|---------|
| ONNX CPU Embedding | `EmbeddingWorker.ts` | ✅ Production | `all-MiniLM-L6-v2`, 384D, mean pooling, L2 normalize |
| Vector Storage (INT8) | `VectorRepository.ts` | ✅ Production | `vec0(embedding int8[384])`, sqlite-vec |
| KNN Search | `VectorRepository.ts` | ✅ Production | `vec_idx MATCH` + metadata B-Tree pre-filter |
| FTS5 Full-text | `VectorRepository.ts` | ✅ Production | `vectors_fts USING fts5(content, tokenize='porter')` |
| **Hybrid Search (RRF)** | `VectorRepository.ts:577-723` | ✅ Production | Dense + Sparse fusion, K=60 |
| Semantic Router | `SemanticRouter.ts` | ✅ Production | Intent classification, 8 routes, cosine similarity |
| L2 Memory Injection | `PromptBuilder.ts` | ✅ Production | `<context_memory>` XML sandbox |
| Memory Decay | `VectorRepository.ts:772-858` | ✅ Production | Ebbinghaus spaced repetition |

**Gaps xác định**:
1. ❌ Không có ingestion pipeline chuẩn (document loader/chunker)
2. ❌ FTS5 dùng Porter stemmer — chỉ tối ưu cho English
3. ❌ Embedding model (`all-MiniLM-L6-v2`) không hỗ trợ Vietnamese
4. ❌ Không có cross-encoder reranker
5. ❌ Không có Wikipedia/Wikidata integration

### 3.2 RAG Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     LIVA RAG PIPELINE ARCHITECTURE                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌───────────── INGESTION PIPELINE (Offline/Background) ──────┐    │
│  │                                                              │    │
│  │  Document     Chunking      Embedding       Storage          │    │
│  │  Loader  ───► Engine   ───► Worker     ───► (SQLite)         │    │
│  │                             (CPU ONNX)                       │    │
│  │  • Markdown   • Semantic    embedBatch()    • vec_idx INT8   │    │
│  │  • Wikipedia  • Recursive                   • vectors_meta   │    │
│  │  • JSON/YAML  • Header-     all-MiniLM or   • vectors_fts    │    │
│  │  • Plaintext    Aware       multilingual-e5    (FTS5)        │    │
│  │                                                              │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ┌───────────── RETRIEVAL PIPELINE (Online, <100ms) ──────────┐    │
│  │                                                              │    │
│  │  Query ──► Dense (KNN)  ◄──┐                                 │    │
│  │       └──► Sparse (FTS5) ──┤                                 │    │
│  │                            ▼                                  │    │
│  │                    Weighted RRF Fusion (K=60)                 │    │
│  │                            │ Top-10                           │    │
│  │                    Cross-Encoder Reranker (optional)          │    │
│  │                            │ Top-5                            │    │
│  │                    Score Threshold + Ebbinghaus decay         │    │
│  │                            │                                  │    │
│  │                    PromptBuilder <context_memory> injection   │    │
│  │                                                              │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ┌───────────── GENERATION PIPELINE ──────────────────────────┐    │
│  │                                                              │    │
│  │  System Prompt + L3 Profile + L1 Session + L2 RAG context    │    │
│  │       ▼                                                      │    │
│  │  LLM Inference (Router/Expert Hot-Swap) → ZMAS Guard → User │    │
│  │                                                              │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Tích hợp với Memory Tier L0-L3**:

| Tier | RAG Role |
|------|----------|
| **L0 (RAM)** | Working memory — embedding cache, query cache passthrough |
| **L0.5 (Cache)** | SemanticActionCache — bypass LLM nếu cosine > 0.95 |
| **L1 (Session)** | Recent turns — source data cho RAG chunks (drill-down) |
| **L2 (Vector)** | **PRIMARY RAG TIER** — KNN + FTS5 hybrid, `<context_memory>` injection, **🆕 external knowledge** |
| **L3 (Archive)** | Consolidated axioms, PersonalKnowledge, entity graph |

### 3.3 Chunking Strategies Comparison

| Strategy | Chunk Size | Overlap | Ưu điểm | Nhược điểm | LIVA Fit |
|----------|-----------|---------|----------|-------------|----------|
| Fixed-size (Token) | 128-512 tok | 20-50 tok | Đơn giản, predictable | Cắt giữa câu | ⭐⭐ |
| Recursive Character | 200-500 chars | 50-100 chars | Tôn trọng paragraph | Không nhận biết structure | ⭐⭐⭐ |
| Semantic (Paragraph) | Variable | Implicit | Giữ semantic unit | Size không đều | ⭐⭐⭐⭐ |
| Sliding Window | 256 tok | 64 tok (25%) | Coverage tốt | Redundancy ~30% | ⭐⭐⭐ |
| **Document-Aware** | Variable | Metadata carry | Giữ structure + metadata | Complex implementation | ⭐⭐⭐⭐⭐ |
| Sentence-Based | 3-5 sentences | 1 sentence | Tự nhiên | VN sentence detection khó | ⭐⭐⭐ |

**🏆 Khuyến nghị: Document-Aware Recursive** — kết hợp recursive splitting với header/code block awareness.

```
Config khuyến nghị:
  maxTokens:         200-250  (fit all-MiniLM-L6-v2 window 256 tokens)
  overlap:           50 tokens (25%)
  separators:        ["\n## ", "\n### ", "\n\n", "\n", ". ", " "]
  respectHeaders:    true
  respectCodeBlocks: true
  metadata:          { section_title, doc_source, chunk_index }
```

**Vietnamese special handling**: Vietnamese characters map to ~1.5x more tokens than English → target 200-250 tokens/chunk thay vì 256.

### 3.4 Embedding Models Comparison (10 Models)

| Model | Dims | MTEB Avg | Seq Len | Speed (CPU) | RAM | VN Support | Notes |
|-------|------|----------|---------|-------------|-----|------------|-------|
| **all-MiniLM-L6-v2** 🔵 | 384 | 56.3 | 256 | ⚡ ~5ms | ~90MB | ❌ English | **Current LIVA model** |
| bge-small-en-v1.5 | 384 | 62.2 | 512 | ⚡ ~5ms | ~130MB | ❌ English | Better MTEB |
| gte-small | 384 | 61.4 | 512 | ⚡ ~5ms | ~70MB | ❌ English | Lightweight |
| e5-small-v2 | 384 | 59.9 | 512 | ⚡ ~5ms | ~130MB | ❌ English | Instruction-tuned |
| nomic-embed-text-v1.5 | 768 | 62.3 | 8192 | 🐢 ~15ms | ~550MB | ❌ English | Long context |
| mxbai-embed-large-v1 | 1024 | 64.7 | 512 | 🐢 ~25ms | ~1.3GB | ❌ English | High quality |
| **multilingual-e5-small** 🟢 | 384 | 57.1 | 512 | ⚡ ~7ms | ~470MB | **✅ VN** | **Recommended upgrade** |
| multilingual-e5-base | 768 | 59.5 | 512 | 🐢 ~15ms | ~1.1GB | **✅ VN** | Better quality, heavy |
| paraphrase-multi-MiniLM-L12 | 384 | 55.8 | 128 | ⚡ ~5ms | ~470MB | **✅ VN** | Short seq limit |
| bge-m3 | 1024 | 66.1 | 8192 | 🐌 ~50ms | ~2.2GB | **✅ VN** | SOTA but too heavy |

**Weighted Scoring**:

| Model | Quality (40%) | Speed (25%) | RAM (20%) | VN (15%) | **Total** |
|-------|:---:|:---:|:---:|:---:|:---:|
| all-MiniLM-L6-v2 | 5 | 10 | 10 | 0 | **5.5** |
| bge-small-en-v1.5 | 7 | 10 | 9 | 0 | **6.3** |
| **multilingual-e5-small** | 6 | 9 | 7 | 10 | **7.2** 🏆 |
| paraphrase-multi-MiniLM | 5 | 10 | 7 | 10 | **7.0** |
| bge-m3 | 10 | 2 | 1 | 10 | **5.7** |

**Migration path** (all-MiniLM → multilingual-e5-small):
1. `EmbeddingWorker.ts`: Thay model path → `multilingual-e5-small.onnx`
2. `VectorRepository`: Dimension vẫn **384** → **KHÔNG CẦN migrate** vec_idx schema
3. `ConsolidationCron`: Trigger re-embed toàn bộ L2 vectors (background task)
4. `SemanticRouter`: Re-compute route anchor embeddings (auto at init)
5. Rollback: Feature flag `FF_EMBEDDING_MODEL`, giữ backup old model

### 3.5 Retrieval Methods (Dense/Sparse/Hybrid)

| Method | LIVA Status | Cách hoạt động | Ưu/Nhược |
|--------|------------|----------------|----------|
| **Dense (KNN)** | ✅ `searchSimilarVectors()` | `vec_idx MATCH query_vector` | Semantic understanding; misses exact keywords |
| **Sparse (FTS5)** | ✅ `vectors_fts` | `MATCH 'keyword'` BM25 | Exact match; no semantic understanding |
| **Hybrid (RRF)** | ✅ `searchHybridVectors()` | Dense + Sparse → RRF K=60 | Best of both; slightly slower |
| **Cross-encoder** | 🆕 Proposed | Re-score top-K pairs | Highest quality; +100ms latency |

**Proposed Improvement — Weighted RRF (route-adaptive)**:

Hiện tại LIVA dùng equal weight cho dense và sparse. Đề xuất route-adaptive:

| SemanticRouter Route | Dense Weight | Sparse Weight | Lý do |
|---------------------|-------------|---------------|-------|
| `factual_recall` | 0.4 | 0.6 | Facts có specific keywords |
| `deep_reasoning` | 0.7 | 0.3 | Reasoning cần semantic |
| `tool_recall` | 0.5 | 0.5 | Balanced |
| Default | 0.6 | 0.4 | Semantic slightly preferred |

**Proposed Improvement — FTS5 Vietnamese**:

```sql
-- Hiện tại: Porter stemmer (English-optimized)
CREATE VIRTUAL TABLE vectors_fts USING fts5(content, tokenize='porter');

-- Đề xuất: Unicode61 tokenizer (Vietnamese-ready)
CREATE VIRTUAL TABLE vectors_fts USING fts5(content, tokenize='unicode61 remove_diacritics 0');
-- `remove_diacritics 0` giữ nguyên dấu: "mà" ≠ "ma" (khác nghĩa trong tiếng Việt)
```

### 3.6 Wikipedia/Wikidata Integration Feasibility

**Data Size Estimates**:

| Source | Full Size | Selective Filter | Chunks | Storage (INT8) |
|--------|-----------|-----------------|--------|---------------|
| Vietnamese Wikipedia (full 1.9M articles) | ~2.3GB raw | N/A | ~5.7M | **~7.9GB** ⚠️ |
| VN Wikipedia (top 100K articles) | ~500MB | By pageviews | ~300K | **~415MB** ✅ |
| VN Wikipedia (STEM/Tech 20-50K) | ~150MB | By category | ~150K | **~150MB** ✅ |
| English Wikipedia (CS/Tech subset) | ~22GB raw | ~500K articles | ~1.5M | ~2-3GB ⚠️ |
| Wikidata entities | ~120GB raw | Related subset | — | ~500MB-1GB |

**Processing Time Estimates**:

| Task | Selective (20-50K articles) | Medium (100K articles) |
|------|---------------------------|----------------------|
| Parse dump | ~5 min | ~15 min |
| Chunk articles | ~3 min | ~5 min |
| Embed chunks (CPU ONNX) | **~12 min** | **~25 min** |
| Build FTS5 index | ~3 min | ~10 min |

**🏆 Recommended Approach**: Phased selective ingestion.

| Phase | Scope | Storage | Processing | Feasibility |
|-------|-------|---------|-----------|-------------|
| Phase 1 | VN STEM/Tech (20-50K articles) | ~150MB | ~25 min | ✅ Quick win |
| Phase 2 | VN Popular (100K articles) | ~415MB | ~55 min | ✅ Acceptable |
| Phase 3 | Wikidata entities | ~500MB-1GB | Hours | ⚠️ Complex |
| ❌ | Full VN Wikipedia (1.9M) | ~7.9GB | ~8 hours | ❌ Quá lớn |

**Update Mechanism**: Periodic dump reload (monthly/quarterly) — diff against existing articles, re-chunk/re-embed only changes. Phù hợp LIVA's offline-first architecture.

### 3.7 Recommended Improvements

**Priority Matrix tổng hợp** (cross-referencing R3 RAG + R4 CRAG/Self-RAG patterns):

| Priority | Feature | Effort | Impact | Source |
|----------|---------|--------|--------|--------|
| **P0** | Document Chunker (Recursive + Header-Aware) | 2-3 days | High | R3 |
| **P0** | FTS5 tokenizer: porter → unicode61 | 1 hour | Medium | R3 |
| **P1** | RAG Ingestion Pipeline class | 3-5 days | High | R3 |
| **P1** | CRAG 3-Tier Scoring tại PromptBuilder | 1 week | High | R3+R4 |
| **P1** | Memory Provenance Metadata (citations) | 0.5 weeks | High | R4 |
| **P1** | Weighted RRF (route-adaptive) | 1 day | Medium | R3 |
| **P2** | multilingual-e5-small migration | 2-3 days | High | R3 |
| **P2** | Wikipedia Ingester (selective VN) | 3-5 days | High | R3 |
| **P3** | Cross-encoder Reranker (ONNX CPU) | 3-5 days | Medium | R3 |
| **P3** | Wikidata Entity Linking | 1 week | Medium | R3+R4 |

---

## 4. Wiki-LLM Projects Analysis (R4)

### 4.1 Project Review Table (8 Projects)

| # | Project | Origin | Approach | Hallucination Mitigation | License | LIVA Applicability |
|---|---------|--------|----------|--------------------------|---------|-------------------|
| 1 | **Self-RAG** | UW/AI2 | Self-reflective retrieval | Reflection tokens: IsRel, IsSup, IsUse | MIT | ⭐⭐⭐⭐⭐ |
| 2 | **CRAG** | HKU/Meta | Corrective retrieval-augmented gen | Relevance evaluator: Correct/Ambiguous/Incorrect | Apache 2.0 | ⭐⭐⭐⭐⭐ |
| 3 | **WikiChat** | Stanford | Multi-stage grounded dialogue | Claim → Search → Verify → Refine pipeline | Apache 2.0 | ⭐⭐⭐⭐ |
| 4 | **RARR** | Google Research | Post-hoc attribution | Post-hoc evidence search + revision | Apache 2.0 | ⭐⭐⭐ |
| 5 | **FreshLLMs** | Google | Knowledge freshness eval | Fast/Slow/Never-changing classification | Apache 2.0 | ⭐⭐⭐⭐ |
| 6 | **KILT** | Meta AI | Benchmark suite | Standardized eval across 11 datasets | MIT | ⭐⭐⭐ |
| 7 | **Atlas** | Meta AI | Fusion-in-Decoder + dual-encoder | Joint retriever-reader training | CC-BY-NC 4.0 ⚠️ | ⭐⭐ |
| 8 | **REALM** | Google | Retrieval-augmented pre-training | End-to-end trained retriever | Apache 2.0 | ⭐⭐ |

### 4.2 Top 3 Applicable Projects (Self-RAG, CRAG, WikiChat)

#### Self-RAG — Best Overall Fit (⭐⭐⭐⭐⭐)

**Core mechanism**: LLM generates reflection tokens evaluating retrieval necessity and response quality.

```
Input → [Retrieve?] → IF YES → Retrieve → [IsRelevant?] → 
Filter → Generate with evidence → [IsSupported?] → 
Score → [IsUseful?] → Final response
```

**Mapping lên LIVA hiện tại**:

| Self-RAG Token | LIVA Equivalent | Hiện trạng |
|---------------|----------------|------------|
| `[Retrieve]` — Cần retrieval không? | SemanticRouter routes `factual_recall` → L2 search | ✅ Đã có |
| `[IsRel]` — Passage có relevant không? | PromptBuilder threshold scoring | ✅ Có nhưng binary |
| `[IsSup]` — Response có supported không? | ReflectionDaemon Φ/Ψ extraction | ⚠️ Cần mở rộng |
| `[IsUse]` — Response có hữu ích không? | HeraCompass `updateUtilityScore()` | ✅ Đã có |

**Đề xuất adopt**: Thêm `relevance_grade` (high/medium/low) vào search results, inject metadata vào `<context_memory>`, mở rộng ReflectionDaemon với `[IsSup]` check.

#### CRAG — Direct Integration Path (⭐⭐⭐⭐⭐)

**Core mechanism**: Retrieval evaluator phân loại documents thành Correct/Ambiguous/Incorrect, trigger corrective actions.

```
Query → Retrieve K docs → Evaluate relevance →
  CORRECT (>0.6)    → Use directly (standard RAG)
  AMBIGUOUS (0.3-0.6) → Decompose query → Re-retrieve
  INCORRECT (<0.3)   → Discard → Web fallback
```

**Mapping lên LIVA**: PromptBuilder.ts lines 182-222 **đã implement partial CRAG** — binary threshold filtering + abstention warning. **Missing piece: AMBIGUOUS tier** với query decomposition.

**Đề xuất adopt**: Extend PromptBuilder scoring từ binary thành 3-tier. Thêm query decomposition service cho AMBIGUOUS cases. Auto-trigger WebSearch skill khi INCORRECT.

#### WikiChat — Best Citation Model (⭐⭐⭐⭐)

**Core pipeline** (5 stages): Generate → Extract Claims → Retrieve → Verify → Refine

**Assessment**: Multi-stage pipeline quá nặng cho local LLM (5 LLM calls/response). Tuy nhiên, citation model rất mạnh — LIVA nên adopt selectively:
- ✅ Inline memory citation: vec_id + domain + created_at metadata
- ✅ Memory provenance chain: L2 vectors → source_event_ids → L1 events
- ❌ Full claim extraction + verification (quá nhiều LLM calls)

### 4.3 Knowledge Grounding Patterns

Tổng hợp 4 patterns áp dụng được cho LIVA (cross-reference R3 + R4):

**Pattern 1: Adaptive Retrieval + CRAG 3-Tier** (Self-RAG + CRAG)
```
SemanticRouter.route(query)
  → factual_recall / deep_reasoning:
    → Retrieve L2 anchors → Evaluate:
      CORRECT (>0.6): Standard injection
      AMBIGUOUS (0.35-0.6): Query decomposition → re-retrieve
      INCORRECT (<0.35): Abstention + suggest WebSearch
  → chitchat / system_command:
    → Skip retrieval (current behavior ✅)
```
Integration point: `PromptBuilder.ts` lines 159-228

**Pattern 2: Memory Citation Chain** (WikiChat)
```
L2 Vector → source_event_ids → L1 Event Bricks → raw conversation
→ Citation: "Theo em nhớ, [ngày X], Sếp đã nói rằng..."
```
Integration point: `VectorRepository` source_event_ids, `StructuredMemory` searchWithDrilldown()

**Pattern 3: Freshness-Aware Routing** (FreshLLMs)
```
SemanticRouter + freshness classification:
  time_sensitive keywords → bypass L2 → WebSearch skill
  stable_knowledge → L2 priority
  Check decay_weight → stale = re-retrieve
```
Integration point: `SemanticRouter.ts` regex fast-track layer

**Pattern 4: Post-Generation Verification** (RARR-lite)
```
AgentLoop → LLM Response → ReflectionDaemon.queueTurn()
  → Existing Φ/Ψ extraction
  → NEW: Claim extraction from AI response
  → NEW: Search L2 for supporting evidence
  → NEW: Flag unsupported claims (HeraCompass warning)
```
Integration point: `ReflectionDaemon.ts` processBatch()

### 4.4 Citation & Fact-Checking Approaches

Ba cấp độ citation LIVA có thể adopt, từ dễ đến khó:

| Approach | Effort | Mô tả | Impact |
|----------|--------|-------|--------|
| **A. Inline Memory Provenance** | 0.5 weeks | Thêm vec_id + domain + created_at vào `<context_memory>` XML | LLM nói "Theo thông tin em nhớ từ ngày 5/6..." |
| **B. CRAG 3-Tier Scoring** | 1 week | Extend PromptBuilder từ binary → CORRECT/AMBIGUOUS/INCORRECT | Giảm confident hallucination |
| **C. Self-RAG Reflection** | 2 weeks | ReflectionDaemon verify response-evidence alignment | Systematic hallucination detection |

**Approach A** (Recommended — immediate):
```typescript
// Proposed <context_memory> format:
`[Memory ${r.vecId}, ${r.created_at}, Domain: ${r.domain}]: ${r.content}`
// → LLM có thể tự cite: "Theo thông tin em nhớ từ ngày X..."
```

**Approach B** (Recommended — Phase 1):
```typescript
// Current: binary logic
if (bestScore >= thresholdUsed) { inject } else { abstention }

// Proposed: 3-tier
if (bestScore >= 0.6)        { /* CORRECT — high confidence */ }
else if (bestScore >= 0.35)  { /* AMBIGUOUS — inject with caveat */ }
else                         { /* INCORRECT — abstention */ }
```

### 4.5 License Compatibility

| Project | License | Compatible? | Notes |
|---------|---------|------------|-------|
| Self-RAG | MIT | ✅ Fully | Any use allowed |
| CRAG | Apache 2.0 | ✅ Fully | Permissive |
| WikiChat | Apache 2.0 | ✅ Fully | Concepts only |
| RARR | Apache 2.0 | ✅ Fully | Post-hoc patterns |
| FreshLLMs | Apache 2.0 | ✅ Fully | Freshness concepts |
| KILT | MIT | ✅ Fully | Eval framework |
| **Atlas** | **CC-BY-NC 4.0** | **⚠️ Non-commercial** | **DO NOT adopt code directly** |
| REALM | Apache 2.0 | ✅ Fully | Foundational concepts |

> **⚠️ Atlas (CC-BY-NC)**: Nếu LIVA có kế hoạch thương mại hóa, KHÔNG sử dụng code trực tiếp từ Atlas. Các project khác đều MIT hoặc Apache 2.0 — an toàn hoàn toàn.

---

## 5. Integration Roadmap (R5 — Synthesis)

Roadmap này **hợp nhất** R3's technical RAG recommendations với R4's phased plan, đồng thời tích hợp R1 model validation và R2 framework assessment.

### 5.1 Strategic Recommendations Summary

| Lĩnh vực | Recommendation | Confidence |
|-----------|---------------|------------|
| **LLM Models** | Giữ Gemma 4 E4B + 12B QAT — no change needed | Very High |
| **Inference** | Giữ llama.cpp + MLX — incremental upgrades only | Very High |
| **RAG Retrieval** | Enhance existing Hybrid RRF + add CRAG 3-tier scoring | High |
| **RAG Ingestion** | Build DocumentChunker + RAGIngestionPipeline | High |
| **Embedding** | Migrate all-MiniLM → multilingual-e5-small (Phase 2) | High |
| **FTS5** | Porter → Unicode61 tokenizer (Vietnamese) | High |
| **Wikipedia** | Selective VN STEM/Tech (20-50K articles, ~150MB) | Medium |
| **Citation** | Inline memory provenance in `<context_memory>` | High |
| **Verification** | CRAG + Self-RAG-lite patterns at PromptBuilder | Medium |

### 5.2 Phase 1: Foundation (3-4 weeks) — Quick Wins

Tất cả thay đổi Phase 1 là **additive** — không modify existing logic, chỉ mở rộng.

| # | Deliverable | Mô tả | Effort | Risk | Files Affected |
|---|-------------|-------|--------|------|---------------|
| 1.1 | **Memory Provenance Metadata** | Extend `<context_memory>` XML với vec_id, domain, created_at cho inline citation | 0.5 weeks | LOW | PromptBuilder.ts |
| 1.2 | **CRAG 3-Tier Scoring** | Replace binary relevant/abstention → CORRECT/AMBIGUOUS/INCORRECT | 1 week | LOW | PromptBuilder.ts |
| 1.3 | **FTS5 Unicode61 Migration** | Porter → unicode61 tokenizer cho Vietnamese text search | 0.5 days | LOW | VectorRepository.ts + migration script |
| 1.4 | **Document Chunker** | Recursive + header-aware chunking class | 2-3 days | LOW | New file: DocumentChunker.ts |
| 1.5 | **Freshness-Aware Routing** | Time-sensitivity keywords trong SemanticRouter regex fast-track | 0.5 weeks | LOW | SemanticRouter.ts |
| 1.6 | **Abstention Response Tuning** | Improve system prompt cho `<memory_status>` warning injection | 0.5 weeks | LOW | system_prompt.ts |
| 1.7 | **RAG Quality Metrics** | Telemetry: L2 injection hit rate, average score, abstention rate | 1 week | LOW | TelemetryProfiler.ts |

**Dependencies**: None — all additive.
**Total effort**: ~3.5-4 person-weeks.
**Expected impact**: Giảm hallucination từ memory, cải thiện Vietnamese search, baseline metrics cho future improvements.

### 5.3 Phase 2: Enhancement (6-8 weeks) — Quality Leap

| # | Deliverable | Mô tả | Effort | Risk | Dependencies |
|---|-------------|-------|--------|------|-------------|
| 2.1 | **RAG Ingestion Pipeline** | Standardized document loading, chunking, batch embedding, upsert | 3-5 days | LOW | P1.4 (Chunker) |
| 2.2 | **Multilingual Embedding Migration** | all-MiniLM-L6-v2 → multilingual-e5-small (384D, VN support) | 2-3 days | MEDIUM | Testing re-embed |
| 2.3 | **Wikipedia Ingester (VN STEM)** | Selective VN Wikipedia (20-50K STEM/Tech articles, ~150MB) | 3-5 days | MEDIUM | P2.1 (Pipeline) |
| 2.4 | **Query Decomposition** | CRAG-style: AMBIGUOUS → split complex queries → re-embed → union | 2 weeks | MEDIUM | P1.2 (3-tier) |
| 2.5 | **Weighted RRF** | Route-adaptive dense/sparse weights in searchHybridVectors() | 1 day | LOW | P1.2 |
| 2.6 | **Post-Generation Verification** | RARR-lite: ReflectionDaemon claim-evidence alignment check | 2 weeks | MEDIUM | P1.7 (metrics) |
| 2.7 | **Corrective Web Fallback** | Auto-suggest WebSearch khi INCORRECT → cache result vào L2 | 1.5 weeks | MEDIUM | P1.2 (3-tier) |

**Dependencies**: Phase 1 complete (especially P1.2 3-tier scoring, P1.4 Chunker, P1.7 metrics).
**Total effort**: ~7-8 person-weeks.
**Expected impact**: Vietnamese RAG functional, Wikipedia knowledge available, systematic hallucination detection.

### 5.4 Phase 3: Scale (8-12 weeks) — Full Knowledge System

| # | Deliverable | Mô tả | Effort | Risk | Dependencies |
|---|-------------|-------|--------|------|-------------|
| 3.1 | **Knowledge Graph RAG** | Leverage graph_nodes/graph_edges cho multi-hop questions | 3 weeks | HIGH | P2 complete |
| 3.2 | **Temporal Knowledge Management** | FreshLLMs-style: tag vectors with temporal validity, auto-invalidate | 2 weeks | MEDIUM | P2.3 |
| 3.3 | **Multi-Turn Grounded Dialogue** | WikiChat-lite: per-session evidence chain, accumulated citations | 2 weeks | MEDIUM | P2.6 |
| 3.4 | **Cross-encoder Reranker** | ONNX CPU reranker (ms-marco-MiniLM) in EmbeddingWorker | 3-5 days | LOW | None |
| 3.5 | **Wikipedia Expansion** | Top 100K VN articles by pageviews (~415MB) | 1 week | LOW | P2.3 |
| 3.6 | **Wikidata Entity Linking** | Entity-attribute table, fact verification, GraphRepository integration | 1 week | MEDIUM | P3.5, GraphRepo |
| 3.7 | **KILT-style Evaluation Suite** | Internal benchmark: curate LIVA QA pairs, automated RAG regression testing | 2 weeks | LOW | P1.7 |

**Dependencies**: Phase 2 complete (especially P2.3 Wikipedia, P2.6 verification).
**Total effort**: ~10-12 person-weeks.
**Expected impact**: Full knowledge system with temporal awareness, multi-hop reasoning, automated quality regression.

### 5.5 Phase 4: Autonomous Evolution (6-10 weeks)

| # | Deliverable | Mô tả | Effort | Risk |
|---|-------------|-------|--------|------|
| 4.1 | **Proactive Knowledge Gap Detection** | Detect abstention patterns → auto-trigger background research | 3 weeks | HIGH |
| 4.2 | **Memory Contradiction Detection** | ContradictionResolver + cross-source conflict resolution (L2 vs web) | 2 weeks | MEDIUM |
| 4.3 | **User-Correctable Memory** | UI cho user correct/confirm AI memory → feedback loop | 3 weeks | MEDIUM |
| 4.4 | **Self-RAG Training Loop** | Fine-tune Router (LoRA) on reflection token prediction | 3 weeks | HIGH |

**Dependencies**: Phase 3 complete.
**Total effort**: ~8-11 person-weeks.

### 5.6 Total Effort & Timeline Overview

```
Timeline (person-weeks):

Phase 1: Foundation       ████████ 3-4 weeks    Quick Wins
Phase 2: Enhancement      ████████████████ 7-8 weeks    Quality Leap
Phase 3: Scale            ████████████████████████ 10-12 weeks    Full Knowledge
Phase 4: Autonomous       ████████████████████ 8-11 weeks    Evolution

Total: ~28-35 person-weeks (7-9 months at 1 FTE)
```

| Phase | Effort | Cumulative | Key Milestone |
|-------|--------|-----------|---------------|
| Phase 1 | 3-4 weeks | 3-4 weeks | CRAG scoring + citation + VN FTS5 |
| Phase 2 | 7-8 weeks | 10-12 weeks | VN RAG + Wikipedia + verification |
| Phase 3 | 10-12 weeks | 20-24 weeks | Knowledge Graph + temporal + eval |
| Phase 4 | 8-11 weeks | 28-35 weeks | Self-evolving knowledge system |

> **Khuyến nghị**: Bắt đầu Phase 1 ngay — tất cả deliverables đều low-risk, additive, không ảnh hưởng existing features. Phase 1.2 (CRAG 3-Tier) mang lại **highest ROI** vì giảm hallucination trực tiếp.

---

## 6. Risk Matrix

Consolidated risk matrix từ tất cả research areas (R1-R4):

| # | Risk | Probability | Impact | Category | Mitigation |
|---|------|------------|--------|----------|-----------|
| 1 | **L2 injection latency tăng** do CRAG scoring | Medium | Medium | Performance | `withSafeTimeout` 1500ms đã có. 3-tier chỉ thêm CPU logic, không thêm LLM call |
| 2 | **Router overload** bởi reflection verification calls | Medium | High | VRAM | Chỉ verify khi route = `factual_recall`. Rate limit 1 verification/turn |
| 3 | **Embedding model mismatch** khi upgrade | Low | Critical | Data | `multilingual-e5-small` vẫn 384D → schema compatible. Re-embed all L2 vectors via ConsolidationCron |
| 4 | **FTS5 tokenizer migration** (porter → unicode61) | Low | Medium | Data | Requires rebuilding FTS index. Run as one-time background job |
| 5 | **Hot-swap latency spike** nếu thêm VRAM features | Low | High | VRAM | Strict 1-model-only policy. CPU-only embedding. No concurrent model loading |
| 6 | **Query decomposition gây token explosion** | Low | Medium | Performance | Cap sub-queries tại 3. Same total token budget |
| 7 | **False abstention** — AI refuses answer khi score vừa dưới threshold | Medium | Medium | UX | Tunable threshold via ENV. Monitor abstention rate in telemetry |
| 8 | **Memory poisoning** qua `<context_memory>` | Low | High | Security | XML sandbox isolation + `[SYSTEM NOTE]` header + ZMAS_Guard output filter |
| 9 | **Stale knowledge** — outdated vectors with high score | Medium | Medium | Data | Ebbinghaus decay_weight (đã có). Phase 3.2 temporal management |
| 10 | **Breaking existing features** khi modify PromptBuilder | Medium | High | Regression | Vitest test suite. Additive-only changes in Phase 1 |
| 11 | **Wikipedia storage explosion** | Low | Medium | Storage | Selective filtering (20-50K articles, ~150MB). Phase approach |
| 12 | **New model release** vượt trội Gemma 4 | Low | Low | Strategy | Monitor Qwen 3.5, Llama 4 Mini. Re-evaluate nếu +5 MMLU + VN support + tool call |
| 13 | **llama.cpp ABI breaking changes** | Medium | Medium | Maintenance | `liva_native_engine.py` has extensive padding/fallback. Pin llama.cpp version |
| 14 | **MLX speculative decoding instability** | Medium | Low | macOS | Feature flag `LIVA_SPECULATIVE_IN_EXPERT_MODE`. Fallback to standard inference |
| 15 | **Expert model swap** during verification | Low | High | VRAM | Verification chỉ dùng Router. VRAM Guard + agentLoopStateGetter đã bảo vệ |

---

## 7. Decision Matrix — Quick Reference

| Area | Current | Recommendation | Priority | Effort | Risk |
|------|---------|---------------|----------|--------|------|
| **Router Model** | Gemma 4 E4B QAT (5.3GB) | **Keep** ✅ | — | Zero | None |
| **Expert Model** | Gemma 4 12B QAT (6.7GB) | **Keep** ✅ | — | Zero | None |
| **Inference (llama.cpp)** | LivaNativeEngine + llama-server | **Keep** ✅ — upgrade binary | P1 | Low | Low |
| **Inference (MLX)** | LivaMlxEngine | **Keep** ✅ — upgrade mlx-lm | P1 | Low | Low |
| **Speculative Decoding** | Supported but unused | **Implement** on macOS 16GB+ | P2 | Medium | Medium |
| **Embedding Model** | all-MiniLM-L6-v2 (384D, EN) | **Migrate** → multilingual-e5-small (384D, VN) | P2 | 2-3 days | Medium |
| **FTS5 Tokenizer** | Porter (English) | **Change** → unicode61 (Vietnamese) | P0 | 1 hour | Low |
| **RAG Scoring** | Binary (pass/fail) | **Upgrade** → CRAG 3-tier (CORRECT/AMBIGUOUS/INCORRECT) | P1 | 1 week | Low |
| **Document Chunker** | None | **Build** — recursive + header-aware | P0 | 2-3 days | Low |
| **Ingestion Pipeline** | None | **Build** — batch embed + upsert | P1 | 3-5 days | Low |
| **Wikipedia Knowledge** | None | **Build** — selective VN STEM (20-50K articles) | P2 | 3-5 days | Medium |
| **Citation System** | None | **Build** — inline memory provenance metadata | P1 | 0.5 weeks | Low |
| **Reranker** | None | **Build** — ONNX cross-encoder in CPU worker | P3 | 3-5 days | Low |
| **RRF Weights** | Equal (0.5/0.5) | **Upgrade** → route-adaptive (0.3-0.7 range) | P1 | 1 day | Low |
| **Post-gen Verification** | None | **Build** — RARR-lite in ReflectionDaemon | P2 | 2 weeks | Medium |
| **Knowledge Graph RAG** | GraphRepository exists | **Extend** — multi-hop traversal | P3 | 3 weeks | High |
| **Temporal Management** | Ebbinghaus decay | **Extend** — temporal validity tags | P3 | 2 weeks | Medium |

---

## Appendix

### A. Benchmark Data Sources & Caveats

**Data Sources**:
1. Google Gemma 4 Technical Report (2025) — Official model cards, QAT methodology
2. Hugging Face Open LLM Leaderboard v2 (2025) — Standardized benchmarks
3. LM Arena (Chatbot Arena) — Human preference rankings
4. Alibaba Qwen 3 Technical Report (2025) — Official benchmarks
5. Meta Llama 4 Announcement (2025) — Architecture details
6. Microsoft Phi-4 Technical Report (2025) — Benchmark data
7. llama.cpp GitHub — GGUF format support, quantization methods
8. MTEB Leaderboard — Embedding model benchmarks

**Caveats quan trọng**:
- Benchmark scores là **ước tính dựa trên training knowledge** (cutoff ~mid-2025). Models released sau có thể có data mới hơn.
- GGUF sizes có thể khác vài trăm MB tùy llama.cpp version và quantization method.
- Inference speed phụ thuộc hardware cụ thể, context length, batch size.
- Vietnamese support assessment dựa trên training data composition, không phải Vietnamese-specific benchmarks (ViMMRC, VLSP).
- Embedding model CPU speeds là estimates — actual Apple Silicon vs Intel có thể khác.
- Wikipedia size estimates phụ thuộc dump date và filtering criteria.

### B. LIVA Architecture Reference

**Tech Stack chính**:
- **Runtime**: Node.js v22+ (ESM), TypeScript 5.x strict
- **UI**: Tauri v2 (Rust host + WebView)
- **LLM**: llama-server (C++ native) / MLX (macOS) — GGUF format
- **Database**: `node:sqlite` + sqlite-vec (INT8 vectors) + FTS5 (full-text)
- **Embedding**: onnxruntime-node (CPU worker, zero VRAM)
- **Communication**: gRPC (Gateway ↔ Engine), WebSocket (Gateway ↔ UI)

**Memory Tiers**:
- L0 (RAM): Working buffer, query cache
- L0.5 (Cache): SemanticActionCache, cosine > 0.95 bypass
- L1 (Session): Turn layer nodes, ReflectionDaemon Φ/Ψ extraction
- L2 (Vector): sqlite-vec KNN + FTS5 hybrid search (RRF)
- L3 (Archive): ConsolidationCron axioms, PersonalKnowledge, GraphRepository

**Key constraints**:
- 8GB VRAM / 8-10GB unified memory target
- Single model on VRAM at any time (Sequential Hot-Swap)
- CPU-only embedding (zero VRAM overhead)
- No Docker/WSL2, no dual model concurrent load
- GGUF format required (llama.cpp compatibility)

### C. Glossary

| Term | Definition |
|------|-----------|
| **CRAG** | Corrective Retrieval-Augmented Generation — phân loại retrieval quality thành 3 tiers |
| **FTS5** | Full-Text Search 5 — SQLite full-text search engine |
| **GGUF** | GPT-Generated Unified Format — binary format cho quantized LLM weights |
| **Hot-Swap** | Kỹ thuật thay đổi model trên VRAM mà không restart process |
| **KNN** | K-Nearest Neighbors — tìm vectors gần nhất trong vector store |
| **KV Cache** | Key-Value Cache — lưu trữ intermediate computations cho transformer attention |
| **MLX** | Apple Machine Learning framework — optimized cho Apple Silicon |
| **MoE** | Mixture of Experts — kiến trúc chỉ activate subset of parameters per token |
| **MTEB** | Massive Text Embedding Benchmark — benchmark chuẩn cho embedding models |
| **ONNX** | Open Neural Network Exchange — portable inference runtime |
| **PTQ** | Post-Training Quantization — quantize model sau khi training xong |
| **QAT** | Quantization-Aware Training — quantize trong quá trình training (quality tốt hơn PTQ) |
| **RAG** | Retrieval-Augmented Generation — kết hợp search + LLM để giảm hallucination |
| **RRF** | Reciprocal Rank Fusion — phương pháp merge kết quả từ nhiều search sources |
| **Self-RAG** | Self-Reflective RAG — LLM tự đánh giá retrieval quality qua reflection tokens |
| **sqlite-vec** | SQLite extension cho vector similarity search (KNN) |
| **VRAM** | Video RAM — bộ nhớ GPU dùng cho LLM inference |
| **WikiChat** | Stanford project — multi-stage grounded dialogue với Wikipedia |

---

> *Báo cáo này được tổng hợp từ 4 nghiên cứu song song (R1: Model Evaluation, R2: Inference Frameworks, R3: RAG Architecture, R4: Wiki-LLM Projects) bởi LIVA Research Team.*
>
> *Generated: 2026-06-10 — LIVA Research Sprint*
