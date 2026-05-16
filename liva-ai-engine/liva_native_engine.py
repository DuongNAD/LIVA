"""
LIVA Zero-Overhead Native Inference Engine
==========================================
Direct ctypes CFFI integration targeting the bespoke hardware-compiled llama.dll.
Communicates with Node.js Gateway via JSONL-over-TCP IPC (zero HTTP overhead).

Architecture:
  Python (this) <--ctypes/CFFI--> llama.dll (SM 12.0 Blackwell)
  Node.js Gateway <--JSONL/TCP:8100--> Python (this)
"""

import os
import sys
import io
import ctypes
import ctypes.util
import pathlib
import asyncio
import threading
import signal
import time
from collections.abc import Generator
import logging as _logging

_logger = _logging.getLogger("liva_engine")

import grpc  # noqa: E402  — imported early so gRPC method handlers have it in scope


def _write_debug_prompt(prompt_text: str) -> None:
    with open("debug_prompt.txt", "w", encoding="utf-8") as f:
        f.write(prompt_text)# Force UTF-8 output on Windows terminals
if sys.platform == "win32" and sys.stdout.encoding != "utf-8":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")

# ==============================================================================
# Phase 1: Locate and Mount the Native DLL
# ==============================================================================

# Constants
SEPARATOR = "=" * 60

NATIVE_LIB_DIR = pathlib.Path(__file__).parent / "native_lib"
DLL_PATH = NATIVE_LIB_DIR / "llama.dll"

if not DLL_PATH.exists():
    raise FileNotFoundError(
        f"[LIVA Native Engine] llama.dll not found at {DLL_PATH}.\n"
        f"Run liva_first_run_build.ps1 first to compile from source."
    )

# Add native_lib to DLL search path so ggml-cuda.dll etc. are found
os.add_dll_directory(str(NATIVE_LIB_DIR))
os.environ["PATH"] = str(NATIVE_LIB_DIR) + os.pathsep + os.environ.get("PATH", "")
if sys.platform == "win32":
    try:
        ctypes.windll.kernel32.SetDllDirectoryW(str(NATIVE_LIB_DIR))
    except Exception:
        pass

lib = ctypes.CDLL(str(DLL_PATH), winmode=0)

# ==============================================================================
# Phase 2: C-Type Definitions (Exact ABI match for x64 Windows MSVC)
# ==============================================================================

llama_model_p = ctypes.c_void_p
llama_context_p = ctypes.c_void_p
llama_vocab_p = ctypes.c_void_p
llama_sampler_p = ctypes.c_void_p
llama_token = ctypes.c_int32
llama_pos = ctypes.c_int32
llama_seq_id = ctypes.c_int32

# Callback types
llama_progress_callback = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_float, ctypes.c_void_p)
ggml_backend_sched_eval_callback = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_bool, ctypes.c_void_p)
ggml_abort_callback = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_void_p)


class llama_model_params(ctypes.Structure):
    """Exact byte-layout match for llama_model_params on x64 MSVC."""
    _fields_ = [
        ("devices",                    ctypes.c_void_p),      # ggml_backend_dev_t *
        ("tensor_buft_overrides",      ctypes.c_void_p),      # const struct *
        ("n_gpu_layers",               ctypes.c_int32),        
        ("split_mode",                 ctypes.c_int32),        # enum
        ("main_gpu",                   ctypes.c_int32),        
        ("_pad0",                      ctypes.c_int32),        # alignment padding
        ("tensor_split",               ctypes.c_void_p),      # const float *
        ("progress_callback",          ctypes.c_void_p),      # function pointer
        ("progress_callback_user_data", ctypes.c_void_p),
        ("kv_overrides",               ctypes.c_void_p),
        ("vocab_only",                 ctypes.c_bool),
        ("use_mmap",                   ctypes.c_bool),
        ("use_direct_io",              ctypes.c_bool),
        ("use_mlock",                  ctypes.c_bool),
        ("check_tensors",              ctypes.c_bool),
        ("use_extra_bufts",            ctypes.c_bool),
        ("no_host",                    ctypes.c_bool),
        ("no_alloc",                   ctypes.c_bool),
    ]


class llama_context_params(ctypes.Structure):
    """Exact byte-layout match for llama_context_params on x64 MSVC."""
    _fields_ = [
        ("n_ctx",              ctypes.c_uint32),
        ("n_batch",            ctypes.c_uint32),
        ("n_ubatch",           ctypes.c_uint32),
        ("n_seq_max",          ctypes.c_uint32),
        ("n_threads",          ctypes.c_int32),
        ("n_threads_batch",    ctypes.c_int32),
        ("rope_scaling_type",  ctypes.c_int32),   # enum
        ("pooling_type",       ctypes.c_int32),   # enum
        ("attention_type",     ctypes.c_int32),   # enum
        ("flash_attn_type",    ctypes.c_int32),   # enum
        ("rope_freq_base",     ctypes.c_float),
        ("rope_freq_scale",    ctypes.c_float),
        ("yarn_ext_factor",    ctypes.c_float),
        ("yarn_attn_factor",   ctypes.c_float),
        ("yarn_beta_fast",     ctypes.c_float),
        ("yarn_beta_slow",     ctypes.c_float),
        ("yarn_orig_ctx",      ctypes.c_uint32),
        ("defrag_thold",       ctypes.c_float),
        ("cb_eval",            ctypes.c_void_p),  # callback
        ("cb_eval_user_data",  ctypes.c_void_p),
        ("type_k",             ctypes.c_int32),   # ggml_type enum
        ("type_v",             ctypes.c_int32),   # ggml_type enum
        ("abort_callback",     ctypes.c_void_p),
        ("abort_callback_data", ctypes.c_void_p),
        ("embeddings",         ctypes.c_bool),
        ("offload_kqv",        ctypes.c_bool),
        ("no_perf",            ctypes.c_bool),
        ("op_offload",         ctypes.c_bool),
        ("swa_full",           ctypes.c_bool),
        ("kv_unified",         ctypes.c_bool),
        ("_pad_bools",         ctypes.c_char * 2),  # alignment padding
        ("samplers",           ctypes.c_void_p),
        ("n_samplers",         ctypes.c_size_t),
    ]


class llama_sampler_chain_params(ctypes.Structure):
    _fields_ = [
        ("no_perf", ctypes.c_bool),
    ]


class llama_batch(ctypes.Structure):
    _fields_ = [
        ("n_tokens", ctypes.c_int32),
        ("_pad0",    ctypes.c_int32),     # alignment padding for pointer
        ("token",    ctypes.POINTER(llama_token)),
        ("embd",     ctypes.POINTER(ctypes.c_float)),
        ("pos",      ctypes.POINTER(llama_pos)),
        ("n_seq_id", ctypes.POINTER(ctypes.c_int32)),
        ("seq_id",   ctypes.POINTER(ctypes.POINTER(llama_seq_id))),
        ("logits",   ctypes.POINTER(ctypes.c_int8)),
    ]


# ==============================================================================
# Phase 3: C-Function Prototypes (ABI Mapping)
# ==============================================================================

# --- Backend lifecycle ---
lib.llama_backend_init.argtypes = []
lib.llama_backend_init.restype = None
lib.llama_backend_free.argtypes = []
lib.llama_backend_free.restype = None

# --- Model ---
lib.llama_model_default_params.argtypes = []
lib.llama_model_default_params.restype = llama_model_params

lib.llama_model_load_from_file.argtypes = [ctypes.c_char_p, llama_model_params]
lib.llama_model_load_from_file.restype = llama_model_p

lib.llama_model_free.argtypes = [llama_model_p]
lib.llama_model_free.restype = None

lib.llama_model_desc.argtypes = [llama_model_p, ctypes.c_char_p, ctypes.c_size_t]
lib.llama_model_desc.restype = ctypes.c_int32

# --- Context ---
lib.llama_context_default_params.argtypes = []
lib.llama_context_default_params.restype = llama_context_params

lib.llama_init_from_model.argtypes = [llama_model_p, llama_context_params]
lib.llama_init_from_model.restype = llama_context_p

lib.llama_free.argtypes = [llama_context_p]
lib.llama_free.restype = None

lib.llama_n_ctx.argtypes = [llama_context_p]
lib.llama_n_ctx.restype = ctypes.c_uint32

# --- Vocab ---
lib.llama_model_get_vocab.argtypes = [llama_model_p]
lib.llama_model_get_vocab.restype = llama_vocab_p

lib.llama_vocab_eos.argtypes = [llama_vocab_p]
lib.llama_vocab_eos.restype = llama_token

lib.llama_vocab_bos.argtypes = [llama_vocab_p]
lib.llama_vocab_bos.restype = llama_token

# --- Tokenizer ---
lib.llama_tokenize.argtypes = [
    llama_vocab_p, ctypes.c_char_p, ctypes.c_int32,
    ctypes.POINTER(llama_token), ctypes.c_int32,
    ctypes.c_bool, ctypes.c_bool,
]
lib.llama_tokenize.restype = ctypes.c_int32

lib.llama_token_to_piece.argtypes = [
    llama_vocab_p, llama_token, ctypes.c_char_p,
    ctypes.c_int32, ctypes.c_int32, ctypes.c_bool,
]
lib.llama_token_to_piece.restype = ctypes.c_int32

# --- Batch ---
lib.llama_batch_get_one.argtypes = [ctypes.POINTER(llama_token), ctypes.c_int32]
lib.llama_batch_get_one.restype = llama_batch

lib.llama_batch_init.argtypes = [ctypes.c_int32, ctypes.c_int32, ctypes.c_int32]
lib.llama_batch_init.restype = llama_batch

lib.llama_batch_free.argtypes = [llama_batch]
lib.llama_batch_free.restype = None

# --- Decode ---
lib.llama_decode.argtypes = [llama_context_p, llama_batch]
lib.llama_decode.restype = ctypes.c_int32

# --- Embeddings ---
# llama_get_embeddings(ctx) → float* (pointer to full-context embedding output)
try:
    lib.llama_get_embeddings.argtypes = [llama_context_p]
    lib.llama_get_embeddings.restype = ctypes.POINTER(ctypes.c_float)
    HAS_GET_EMBEDDINGS = True
except AttributeError:
    HAS_GET_EMBEDDINGS = False

# llama_get_embeddings_seq(ctx, seq_id) → float* (per-sequence embedding for batch)
try:
    lib.llama_get_embeddings_seq.argtypes = [llama_context_p, llama_seq_id]
    lib.llama_get_embeddings_seq.restype = ctypes.POINTER(ctypes.c_float)
    HAS_GET_EMBEDDINGS_SEQ = True
except AttributeError:
    HAS_GET_EMBEDDINGS_SEQ = False

# llama_n_embd(model) → int32 (embedding dimension of the model)
lib.llama_n_embd.argtypes = [llama_model_p]
lib.llama_n_embd.restype = ctypes.c_int32

# --- Memory handle ---
# New API: llama_get_memory returns a llama_memory_t handle from context
llama_memory_t = ctypes.c_void_p
try:
    lib.llama_get_memory.argtypes = [llama_context_p]
    lib.llama_get_memory.restype = llama_memory_t
    HAS_GET_MEMORY = True
except AttributeError:
    HAS_GET_MEMORY = False

# --- KV Cache / Memory ---
# New API (llama.cpp 2025+): llama_memory_clear(llama_memory_t mem, bool data)
try:
    lib.llama_memory_clear.argtypes = [llama_memory_t, ctypes.c_bool]
    lib.llama_memory_clear.restype = None
    HAS_MEMORY_CLEAR = True
except AttributeError:
    HAS_MEMORY_CLEAR = False

try:
    lib.llama_memory_seq_rm.argtypes = [llama_memory_t, llama_seq_id, llama_pos, llama_pos]
    lib.llama_memory_seq_rm.restype = ctypes.c_bool
    HAS_MEMORY_SEQ_RM = True
except AttributeError:
    HAS_MEMORY_SEQ_RM = False

# --- Sampler ---
try:
    lib.llama_sampler_reset.argtypes = [llama_sampler_p]
    lib.llama_sampler_reset.restype = None
except AttributeError:
    pass

try:
    lib.llama_sampler_accept.argtypes = [llama_sampler_p, llama_token]
    lib.llama_sampler_accept.restype = None
except AttributeError:
    pass

lib.llama_sampler_chain_default_params.argtypes = []
lib.llama_sampler_chain_default_params.restype = llama_sampler_chain_params

lib.llama_sampler_chain_init.argtypes = [llama_sampler_chain_params]
lib.llama_sampler_chain_init.restype = llama_sampler_p

lib.llama_sampler_chain_add.argtypes = [llama_sampler_p, llama_sampler_p]
lib.llama_sampler_chain_add.restype = None

lib.llama_sampler_init_greedy.argtypes = []
lib.llama_sampler_init_greedy.restype = llama_sampler_p

lib.llama_sampler_init_temp.argtypes = [ctypes.c_float]
lib.llama_sampler_init_temp.restype = llama_sampler_p

lib.llama_sampler_init_top_p.argtypes = [ctypes.c_float, ctypes.c_size_t]
lib.llama_sampler_init_top_p.restype = llama_sampler_p

lib.llama_sampler_init_top_k.argtypes = [ctypes.c_int32]
lib.llama_sampler_init_top_k.restype = llama_sampler_p

lib.llama_sampler_init_min_p.argtypes = [ctypes.c_float, ctypes.c_size_t]
lib.llama_sampler_init_min_p.restype = llama_sampler_p

lib.llama_sampler_init_dist.argtypes = [ctypes.c_uint32]
lib.llama_sampler_init_dist.restype = llama_sampler_p

lib.llama_sampler_sample.argtypes = [llama_sampler_p, llama_context_p, ctypes.c_int32]
lib.llama_sampler_sample.restype = llama_token

lib.llama_sampler_free.argtypes = [llama_sampler_p]
lib.llama_sampler_free.restype = None


# ==============================================================================
# Phase 4: LivaNativeEngine -- Zero-Overhead Inference Core
# ==============================================================================

class LivaNativeEngine:
    """
    Native inference engine using direct ctypes CFFI calls to llama.dll.
    All memory is allocated on the C++ heap. Python only touches pointers.
    """

    def __init__(self, model_path: str, n_ctx: int = 8192, n_gpu_layers: int = -1,
                 n_batch: int = 2048, n_threads: int = 0,
                 flash_attn: bool = True, temperature: float = 0.7,
                 top_p: float = 0.9, top_k: int = 40, min_p: float = 0.05):
        # Auto-detect CPU threads if not specified (0 = auto)
        if n_threads <= 0:
            n_threads = max(1, (os.cpu_count() or 4) - 1)
        self._alive = False
        self.n_batch = n_batch
        self.n_ctx = n_ctx  # Store for prompt overflow guard
        self.has_sampler_reset = hasattr(lib, 'llama_sampler_reset')
        self.has_sampler_accept = hasattr(lib, 'llama_sampler_accept')
        # OS-level mutex: asyncio.Lock only serializes on the event loop,
        # but asyncio.to_thread() runs generate() on OS thread pool.
        # Without this, concurrent gRPC calls (StreamChat + Chat Unary)
        # can both touch C++ engine state simultaneously → NULL deref crash.
        self._engine_mutex = threading.Lock()
        _logger.info("[LIVA Native] Initializing Zero-Overhead Engine...")
        _logger.info(f"  Model: {model_path}")
        _logger.info(f"  Context: {n_ctx} | GPU Layers: {n_gpu_layers} | Flash Attn: {flash_attn}")

        # Initialize backend
        lib.llama_backend_init()

        # Get default model params and modify
        model_params = lib.llama_model_default_params()
        model_params.n_gpu_layers = n_gpu_layers
        model_params.use_mmap = True
        model_params.use_mlock = False

        # Load model
        encoded_path = model_path.encode("utf-8")
        self.model = lib.llama_model_load_from_file(encoded_path, model_params)

        if not self.model:
            raise RuntimeError(f"[LIVA Native] FATAL: Failed to load model from {model_path}")

        # Get model description
        desc_buf = ctypes.create_string_buffer(256)
        lib.llama_model_desc(self.model, desc_buf, 256)
        _logger.info(f"  Model loaded: {desc_buf.value.decode('utf-8', errors='replace')}")

        # Get vocab handle
        self.vocab = lib.llama_model_get_vocab(self.model)
        self.eos_token = lib.llama_vocab_eos(self.vocab)
        self.bos_token = lib.llama_vocab_bos(self.vocab)

        # Get default context params and modify
        ctx_params = lib.llama_context_default_params()
        ctx_params.n_ctx = n_ctx
        ctx_params.n_batch = n_batch
        
        # Thêm dòng này để đồng bộ kích thước micro-batch với batch vật lý
        ctx_params.n_ubatch = n_batch 
        
        ctx_params.n_threads = n_threads
        ctx_params.n_threads_batch = n_threads
        # Flash attention: 0=disabled, 1=enabled, 2=auto
        ctx_params.flash_attn_type = 1 if flash_attn else 0
        ctx_params.offload_kqv = True
        ctx_params.op_offload = True

        # [EMBEDDING SUPPORT] Enable embedding output on shared context.
        # This allocates an extra embedding tensor but reuses 100% model weights.
        # ZERO additional VRAM for model — only ~n_embd * sizeof(float) per token.
        ctx_params.embeddings = True
        # Mean pooling for sentence embeddings (LLAMA_POOLING_TYPE_MEAN = 1)
        ctx_params.pooling_type = 1

        # [TURBO QUANT] Compress KV cache to 4-bit (GGML_TYPE_Q4_0 = 2)
        # This saves ~4x VRAM for the context window without significant quality loss
        ctx_params.type_k = 2
        ctx_params.type_v = 2
        
        self.ctx_params = ctx_params

        # Create context
        self.ctx = lib.llama_init_from_model(self.model, ctx_params)

        if not self.ctx:
            raise RuntimeError("[LIVA Native] FATAL: Failed to create context")

        actual_ctx = lib.llama_n_ctx(self.ctx)
        _logger.info(f"  Context created: n_ctx={actual_ctx}")

        # Get memory handle for KV cache operations (new API)
        if HAS_GET_MEMORY:
            self.memory = lib.llama_get_memory(self.ctx)
            _logger.info(f"  Memory handle acquired: {hex(self.memory) if self.memory else 'NULL'}")
        else:
            self.memory = None

        # Initialize sampler chain
        self.temperature = temperature
        self.top_p = top_p
        self.top_k = top_k
        self.min_p = min_p
        self._init_sampler()

        self._alive = True
        _logger.info(f"[LIVA Native] Engine ready. EOS={self.eos_token}, BOS={self.bos_token}")

    def _init_sampler(self):
        """Create sampler chain with temperature, top_k, top_p, min_p."""
        sparams = lib.llama_sampler_chain_default_params()
        self.sampler = lib.llama_sampler_chain_init(sparams)

        if self.temperature <= 0:
            lib.llama_sampler_chain_add(self.sampler, lib.llama_sampler_init_greedy())
        else:
            lib.llama_sampler_chain_add(self.sampler, lib.llama_sampler_init_top_k(self.top_k))
            lib.llama_sampler_chain_add(self.sampler, lib.llama_sampler_init_top_p(self.top_p, 1))
            lib.llama_sampler_chain_add(self.sampler, lib.llama_sampler_init_min_p(self.min_p, 1))
            lib.llama_sampler_chain_add(self.sampler, lib.llama_sampler_init_temp(self.temperature))
            lib.llama_sampler_chain_add(self.sampler, lib.llama_sampler_init_dist(int(time.time()) % (2**32)))

    def tokenize(self, text: str, add_special: bool = True) -> list[int]:
        """Convert text to token IDs via direct C pointer calls."""
        encoded = text.encode("utf-8")
        # First call with 0 buffer: returns negative of required token count
        n_tokens = lib.llama_tokenize(self.vocab, encoded, len(encoded),
                                       None, 0, add_special, True)
        n_tokens = abs(n_tokens)
        if n_tokens == 0:
            return []

        tokens = (llama_token * n_tokens)()
        actual = lib.llama_tokenize(self.vocab, encoded, len(encoded),
                                     tokens, n_tokens, add_special, True)
        return list(tokens[:actual])

    def detokenize(self, token_id: int) -> str:
        """Convert a single token ID back to text via direct C pointer."""
        buf = ctypes.create_string_buffer(256)
        n = lib.llama_token_to_piece(self.vocab, token_id, buf, 256, 0, False)
        if n < 0:
            return ""
        return buf.raw[:n].decode("utf-8", errors="replace")

    def generate_stream(self, prompt_tokens: list[int], max_tokens: int = 512) -> Generator[str, None, None]:
        """
        Zero-overhead autoregressive generation.
        Yields detokenized text chunks as they are generated.
        Uses OS-level mutex to prevent concurrent C++ access.
        """
        if not self._alive:
            raise RuntimeError("[LIVA Native] Engine is not alive — cannot generate")

        # Guard: Truncate prompt if it exceeds context window (reserve tokens for generation)
        max_prompt_tokens = self.n_ctx - min(max_tokens, 512)  # Reserve at least 512 for output
        if len(prompt_tokens) > max_prompt_tokens:
            _logger.info(f"[LIVA Native] WARNING: Prompt ({len(prompt_tokens)} tokens) exceeds safe limit ({max_prompt_tokens}). Truncating.")
            prompt_tokens = prompt_tokens[-max_prompt_tokens:]  # Keep the tail (most recent context)

        with self._engine_mutex:
            yield from self._generate_stream_unsafe(prompt_tokens, max_tokens)

    def _generate_stream_unsafe(self, prompt_tokens: list[int], max_tokens: int = 512) -> Generator[str, None, None]:
        """
        Internal generation — MUST be called under self._engine_mutex.
        """
        # 1. Reset Sampler
        if self.has_sampler_reset:
            lib.llama_sampler_reset(self.sampler)
        else:
            lib.llama_sampler_free(self.sampler)
            self._init_sampler()

        # 2. Clear KV Cache — prefer llama_memory_clear (fast, ~0ms)
        # New API: llama_memory_clear(memory_handle, data=True) clears both metadata and data
        if HAS_MEMORY_CLEAR and self.memory:
            lib.llama_memory_clear(self.memory, True)
        elif hasattr(self, 'ctx_params'):
            lib.llama_free(self.ctx)
            self.ctx = lib.llama_init_from_model(self.model, self.ctx_params)
            # Re-acquire memory handle after context recreation
            if HAS_GET_MEMORY:
                self.memory = lib.llama_get_memory(self.ctx)

        # 3. Reset Positional Pointer
        n_past = 0

        # Prompt ingestion (Memory-Safe Chunking)
        total_tokens = len(prompt_tokens)
        prompt_arr = (llama_token * total_tokens)(*prompt_tokens)
        
        # CẤP PHÁT RAM VẬT LÝ BẰNG llama_batch_init (Kích thước max = self.n_batch)
        batch = lib.llama_batch_init(self.n_batch, 0, 1)
        
        try:
            # --- VÒNG LẶP NẠP PROMPT (CHUNKING) ---
            while n_past < total_tokens:
                chunk_size = min(self.n_batch, total_tokens - n_past)
                
                batch.n_tokens = chunk_size
                for i in range(chunk_size):
                    batch.token[i] = prompt_arr[n_past + i]
                    batch.pos[i] = n_past + i
                    batch.n_seq_id[i] = 1
                    batch.seq_id[i][0] = 0
                    batch.logits[i] = 0
                
                # TỐI ƯU HIỆU NĂNG: Chỉ bật cờ tính Logits cho token cuối cùng của TOÀN BỘ prompt
                if n_past + chunk_size == total_tokens:
                    batch.logits[chunk_size - 1] = 1
                    
                rc = lib.llama_decode(self.ctx, batch)
                if rc != 0:
                    raise RuntimeError(f"[LIVA Native] llama_decode failed during prompt ingestion (rc={rc})")
                
                n_past += chunk_size

            # --- VÒNG LẶP SINH TOKEN (AUTOREGRESSIVE) ---
            for _ in range(max_tokens):
                new_token = lib.llama_sampler_sample(self.sampler, self.ctx, -1)

                if new_token == self.eos_token:
                    break

                text = self.detokenize(new_token)
                
                # Must update sampler's ring buffer with newly generated token
                if self.has_sampler_accept:
                    lib.llama_sampler_accept(self.sampler, new_token)
                    
                yield text

                # Tái sử dụng vùng nhớ của batch để nạp token vừa sinh ra
                batch.n_tokens = 1
                batch.token[0] = new_token
                batch.pos[0] = n_past
                batch.n_seq_id[0] = 1
                batch.seq_id[0][0] = 0
                batch.logits[0] = 1

                rc = lib.llama_decode(self.ctx, batch)
                if rc != 0:
                    _logger.info(f"[LIVA Native] WARNING: llama_decode error (rc={rc}), stopping")
                    break
                    
                n_past += 1
                
        finally:
            # BẮT BUỘC DỌN RÁC: Trả lại bộ nhớ C++ cho hệ điều hành trong mọi tình huống
            lib.llama_batch_free(batch)

    def generate(self, prompt_tokens: list[int], max_tokens: int = 512) -> str:
        """Non-streaming generation."""
        return "".join(self.generate_stream(prompt_tokens, max_tokens))

    def get_embedding_dim(self) -> int:
        """Get embedding dimension from loaded model."""
        return lib.llama_n_embd(self.model)

    def get_embeddings_batch(self, texts: list[str]) -> list[list[float]]:
        """
        Batch embedding extraction using the SHARED GPU context.
        Thread-safe: acquires _engine_mutex to prevent concurrent C++ access.
        Returns L2-normalized vectors via numpy.

        Architecture:
          - Uses the SAME context as Chat/StreamChat (embeddings=True at init)
          - Clears KV cache, then decodes all texts with separate seq_ids
          - Extracts per-sequence embeddings via llama_get_embeddings_seq()
          - Falls back to llama_get_embeddings() for single-text input
          - L2 normalizes in numpy (offloads math from Node.js main thread)
        """
        import numpy as np

        if not self._alive:
            raise RuntimeError("[LIVA Native] Engine is not alive — cannot embed")

        if not HAS_GET_EMBEDDINGS and not HAS_GET_EMBEDDINGS_SEQ:
            raise RuntimeError("[LIVA Native] llama.dll does not export llama_get_embeddings — update DLL")

        n_embd = self.get_embedding_dim()
        if n_embd <= 0:
            raise RuntimeError(f"[LIVA Native] Invalid embedding dimension: {n_embd}")

        with self._engine_mutex:
            return self._get_embeddings_batch_unsafe(texts, n_embd, np)

    def _get_embeddings_batch_unsafe(self, texts: list[str], n_embd: int, np) -> list[list[float]]:
        """
        Internal embedding — MUST be called under self._engine_mutex.
        This serializes with generate_stream/generate to prevent C++ segfault.
        """
        results = []

        # 1. Clear KV Cache for clean embedding pass
        if HAS_MEMORY_CLEAR and self.memory:
            lib.llama_memory_clear(self.memory, True)
        elif hasattr(self, 'ctx_params'):
            lib.llama_free(self.ctx)
            self.ctx = lib.llama_init_from_model(self.model, self.ctx_params)
            if HAS_GET_MEMORY:
                self.memory = lib.llama_get_memory(self.ctx)

        # 2. Allocate batch buffer (reused across all texts)
        batch = lib.llama_batch_init(self.n_batch, 0, len(texts) if len(texts) > 1 else 1)

        try:
            if len(texts) == 1 and HAS_GET_EMBEDDINGS:
                # --- Single text fast path ---
                tokens = self.tokenize(texts[0], add_special=True)
                if len(tokens) > self.n_ctx - 4:
                    tokens = tokens[:self.n_ctx - 4]

                batch.n_tokens = len(tokens)
                for i, tok in enumerate(tokens):
                    batch.token[i] = tok
                    batch.pos[i] = i
                    batch.n_seq_id[i] = 1
                    batch.seq_id[i][0] = 0
                    batch.logits[i] = 1  # [FIX] Mark ALL tokens as output for Mean Pooling

                rc = lib.llama_decode(self.ctx, batch)
                if rc != 0:
                    raise RuntimeError(f"llama_decode failed for embedding (rc={rc})")

                # Extract embedding pointer
                if HAS_GET_EMBEDDINGS_SEQ:
                    embd_ptr = lib.llama_get_embeddings_seq(self.ctx, 0)
                else:
                    embd_ptr = lib.llama_get_embeddings(self.ctx)

                if not embd_ptr:
                    raise RuntimeError("llama_get_embeddings returned NULL")

                vec = np.ctypeslib.as_array(embd_ptr, shape=(n_embd,)).copy()
                # L2 normalize
                norm = np.linalg.norm(vec)
                if norm > 0:
                    vec /= norm
                results.append(vec.tolist())

            else:
                # --- Multi-text batch path ---
                # Process texts sequentially, each reusing seq_id=0 (n_seq_max=1)
                # KV cache cleared between texts for clean position space
                for seq_idx, text in enumerate(texts):
                    tokens = self.tokenize(text, add_special=True)
                    if len(tokens) > self.n_ctx - 4:
                        tokens = tokens[:self.n_ctx - 4]

                    # Clear previous batch state for reuse
                    batch.n_tokens = len(tokens)
                    for i, tok in enumerate(tokens):
                        batch.token[i] = tok
                        batch.pos[i] = i
                        batch.n_seq_id[i] = 1
                        batch.seq_id[i][0] = 0  # [CRITICAL FIX] Force slot 0 — n_seq_max=1
                        batch.logits[i] = 1     # [FIX] Mark ALL tokens as output for Mean Pooling

                    rc = lib.llama_decode(self.ctx, batch)
                    if rc != 0:
                        raise RuntimeError(f"llama_decode failed for text #{seq_idx} (rc={rc})")

                    # Extract embedding from slot 0 (always)
                    if HAS_GET_EMBEDDINGS_SEQ:
                        embd_ptr = lib.llama_get_embeddings_seq(self.ctx, 0)  # [CRITICAL FIX] Always slot 0
                    else:
                        embd_ptr = lib.llama_get_embeddings(self.ctx)

                    if not embd_ptr:
                        raise RuntimeError(f"llama_get_embeddings returned NULL for text #{seq_idx}")

                    vec = np.ctypeslib.as_array(embd_ptr, shape=(n_embd,)).copy()
                    # L2 normalize
                    norm = np.linalg.norm(vec)
                    if norm > 0:
                        vec /= norm
                    results.append(vec.tolist())

                    # Clear KV between sequences to prevent position collision
                    if HAS_MEMORY_CLEAR and self.memory:
                        lib.llama_memory_clear(self.memory, True)

        finally:
            lib.llama_batch_free(batch)

        return results

    def shutdown(self):
        """RAII cleanup -- free all C++ heap allocations."""
        if not self._alive:
            return
        _logger.info("[LIVA Native] Shutting down engine...")
        if hasattr(self, "sampler") and self.sampler:
            lib.llama_sampler_free(self.sampler)
            self.sampler = None
        if hasattr(self, "ctx") and self.ctx:
            lib.llama_free(self.ctx)
            self.ctx = None
        if hasattr(self, "model") and self.model:
            lib.llama_model_free(self.model)
            self.model = None
        lib.llama_backend_free()
        self._alive = False
        _logger.info("[LIVA Native] Engine shutdown complete.")

    def __del__(self):
        self.shutdown()


# ==============================================================================
# Phase 5: gRPC-over-HTTP/2 Server (Zero-Overhead IPC replacing TCP/JSONL)
# ==============================================================================

IPC_PORT = 8100

class LivaInferenceServicer:
    def __init__(self, engine: LivaNativeEngine):
        self.engine = engine
        self.engine_lock = asyncio.Lock()

    async def StreamChat(self, request, context):  # NOSONAR - gRPC method: PascalCase required to match protobuf service definition
        import liva_engine_pb2
        
        req_id = request.request_id or "g_req"
        prompt_text = ""
        
        # Build prompt from messages using standard Gemma/ChatML format
        for msg in request.messages:
            role = msg.role if msg.role else "user"
            if role == "assistant":
                role = "model"
            prompt_text += f"<start_of_turn>{role}\n{msg.content}<end_of_turn>\n"
        prompt_text += "<start_of_turn>model\n"

        # Use to_thread to avoid blocking the event loop with synchronous I/O
        await asyncio.to_thread(_write_debug_prompt, prompt_text)

        tokens = self.engine.tokenize(prompt_text)
        _logger.info(f"[gRPC StreamChat] Received prompt with {len(tokens)} tokens. Max tokens: {request.max_tokens}")
        if len(tokens) > 0:
            _logger.info(f"[gRPC StreamChat] First 50 chars of prompt: {prompt_text[:50]!r}")
            _logger.info(f"[gRPC StreamChat] Last 100 chars of prompt: {prompt_text[-100:]!r}")

        max_tokens = request.max_tokens if request.max_tokens > 0 else 2048

        async with self.engine_lock:
            queue = asyncio.Queue()
            loop = asyncio.get_running_loop()

            def _generator_worker():
                try:
                    for chunk_text in self.engine.generate_stream(tokens, max_tokens):
                        loop.call_soon_threadsafe(queue.put_nowait, chunk_text)
                except Exception as e:
                    _logger.info(f"[gRPC Worker Error] {str(e)}")
                    loop.call_soon_threadsafe(queue.put_nowait, f"\n[Hệ thống AI gặp lỗi nạp Context: {str(e)}]")
                finally:
                    loop.call_soon_threadsafe(queue.put_nowait, None)

            task = asyncio.create_task(asyncio.to_thread(_generator_worker))

            full_text = ""
            yielded_length = 0
            chunk_idx = 0
            
            stop_triggers = ["<start_of_turn>", "</start_of_turn>", "<end_of_turn>", "</end_of_turn>", "end_of_turn>", "<|user|>", "<|im_start|>"]
            
            # ⚡ [PERF] Micro-batch interval: accumulate tokens within this window
            # 5ms = well below human perceptual threshold (16ms) while batching ~2-4 tokens
            # Lower than 10ms to reduce stuttering when generation is slow (large KV cache)
            MICRO_BATCH_SEC = 0.005
            
            has_stop = False
            batch_buf = ""
            while True:
                chunk_text = await queue.get()
                if chunk_text is None:
                    if batch_buf:
                        full_text += batch_buf
                        batch_buf = ""
                    break
                
                batch_buf += chunk_text
                
                # Drain
                try:
                    while True:
                        next_chunk = await asyncio.wait_for(queue.get(), timeout=MICRO_BATCH_SEC)
                        if next_chunk is None:
                            full_text += batch_buf
                            batch_buf = ""
                            # Set a flag to break outer loop
                            has_stop = True
                            break
                        batch_buf += next_chunk
                except asyncio.TimeoutError:
                    pass
                
                if batch_buf:
                    full_text += batch_buf
                    batch_buf = ""
                    
                # Phase 2
                scan_start = max(0, len(full_text) - 20)
                scan_zone = full_text[scan_start:]
                
                first_stop_idx = len(full_text)
                found_stop = False
                for trigger in stop_triggers:
                    idx = scan_zone.find(trigger)
                    if idx != -1:
                        absolute_idx = scan_start + idx
                        if absolute_idx < first_stop_idx:
                            first_stop_idx = absolute_idx
                            found_stop = True
                            
                if found_stop:
                    remaining_safe = full_text[yielded_length:first_stop_idx]
                    if remaining_safe:
                        delta = liva_engine_pb2.ChunkDelta(content=remaining_safe)
                        if chunk_idx == 0:
                            delta.role = "assistant"
                        choice = liva_engine_pb2.ChunkChoice(index=0, delta=delta, finish_reason="")
                        yield liva_engine_pb2.ChatCompletionChunk(
                            id=req_id, object="chat.completion.chunk", model="liva-native", choices=[choice]
                        )
                    break
                    
                # Phase 3
                partial_match_len = 0
                for trigger in stop_triggers:
                    for i in range(len(trigger) - 1, 0, -1):
                        if full_text.endswith(trigger[:i]):
                            partial_match_len = max(partial_match_len, i)
                            break
                            
                safe_len = max(0, len(full_text) - partial_match_len)
                if safe_len > yielded_length:
                    safe_text = full_text[yielded_length:safe_len]
                    yielded_length = safe_len
                    
                    delta = liva_engine_pb2.ChunkDelta(content=safe_text)
                    if chunk_idx == 0:
                        delta.role = "assistant"
                        
                    choice = liva_engine_pb2.ChunkChoice(index=0, delta=delta, finish_reason="")
                    yield liva_engine_pb2.ChatCompletionChunk(
                        id=req_id, object="chat.completion.chunk", model="liva-native", choices=[choice]
                    )
                    chunk_idx += 1

                if has_stop:
                    break

            # Flush remaining buffer
            if not has_stop and yielded_length < len(full_text):
                remaining_safe = full_text[yielded_length:]
                delta = liva_engine_pb2.ChunkDelta(content=remaining_safe)
                if chunk_idx == 0:
                    delta.role = "assistant"
                choice = liva_engine_pb2.ChunkChoice(index=0, delta=delta, finish_reason="")
                yield liva_engine_pb2.ChatCompletionChunk(
                    id=req_id, object="chat.completion.chunk", model="liva-native", choices=[choice]
                )

            # Final chunk with finish reason
            final_choice = liva_engine_pb2.ChunkChoice(
                index=0,
                delta=liva_engine_pb2.ChunkDelta(),
                finish_reason="stop"
            )
            yield liva_engine_pb2.ChatCompletionChunk(
                id=req_id,
                object="chat.completion.chunk",
                model="liva-native",
                choices=[final_choice]
            )

            await task

    async def Chat(self, request, context):  # NOSONAR - gRPC method: PascalCase required to match protobuf service definition
        import liva_engine_pb2
        
        req_id = request.request_id or "g_req"
        prompt_text = ""
        
        for msg in request.messages:
            role = msg.role if msg.role else "user"
            if role == "assistant":
                role = "model"
            prompt_text += f"<start_of_turn>{role}\n{msg.content}<end_of_turn>\n"
        prompt_text += "<start_of_turn>model\n"

        tokens = self.engine.tokenize(prompt_text)
        max_tokens = request.max_tokens if request.max_tokens > 0 else 512

        async with self.engine_lock:
            result_text = await asyncio.to_thread(self.engine.generate, tokens, max_tokens)
        
        # Strip trailing stop sequences
        stop_triggers = ["<start_of_turn>", "<end_of_turn>", "end_of_turn>", "<|user|>", "<|im_start|>"]
        for trigger in stop_triggers:
            if trigger in result_text:
                result_text = result_text.split(trigger)[0]
        
        choice = liva_engine_pb2.ChatCompletionChoice(
            index=0,
            message=liva_engine_pb2.ChatMessage(role="assistant", content=result_text),
            finish_reason="stop"
        )
        
        return liva_engine_pb2.ChatCompletionResponse(
            id=req_id,
            object="chat.completion",
            model="liva-native",
            choices=[choice]
        )

    async def HealthCheck(self, request, context):  # NOSONAR - gRPC method: PascalCase required; request/context params mandated by gRPC interface
        # Yield to event loop once — required for grpc.aio compatibility (keeps as true coroutine)
        await asyncio.sleep(0)
        import liva_engine_pb2
        _KV_CACHE_Q4_0 = 2  # Q4_0 quantization type identifier (matches llama.cpp enum)
        return liva_engine_pb2.HealthResponse(
            alive=True,
            model_name="LIVA Engine",
            uptime_seconds=0,
            vram_usage_mb=0.0,
            kv_cache_type=_KV_CACHE_Q4_0
        )

    async def Embed(self, request, context):  # NOSONAR - gRPC method: PascalCase required to match protobuf service definition
        """
        gRPC Embed handler — generates L2-normalized embeddings via shared GPU context.
        Thread-safe: uses engine._engine_mutex (OS-level) to serialize with Chat/StreamChat.
        Supports batch input for ConsolidationCron throughput (3-5x speedup vs sequential).
        """
        import liva_engine_pb2
        import grpc  # [FIX] Prevent NameError when C++ crashes in except block

        texts = list(request.input)
        if not texts:
            return liva_engine_pb2.EmbeddingResponse(data=[], model="liva-native", dimensions=0)

        n_embd = self.engine.get_embedding_dim()
        _logger.info(f"[gRPC Embed] Processing {len(texts)} text(s), dim={n_embd}")

        try:
            # Run on thread pool — engine._engine_mutex handles C++ serialization
            vectors = await asyncio.to_thread(self.engine.get_embeddings_batch, texts)

            data = []
            for idx, vec in enumerate(vectors):
                data.append(liva_engine_pb2.EmbeddingData(
                    embedding=vec,
                    index=idx
                ))

            return liva_engine_pb2.EmbeddingResponse(
                data=data,
                model="liva-native",
                dimensions=n_embd
            )
        except Exception as e:
            _logger.info(f"[gRPC Embed] ERROR: {str(e)}")
            context.set_code(grpc.StatusCode.INTERNAL)
            context.set_details(f"Embedding failed: {str(e)}")
            return liva_engine_pb2.EmbeddingResponse(data=[], model="liva-native", dimensions=n_embd)


async def start_ipc_server(engine: LivaNativeEngine):
    """Start the gRPC async server."""
    import grpc
    import liva_engine_pb2_grpc
    
    server = grpc.aio.server()
    liva_engine_pb2_grpc.add_LivaInferenceServiceServicer_to_server(LivaInferenceServicer(engine), server)
    
    server.add_insecure_port(f"127.0.0.1:{IPC_PORT}")
    _logger.info(f"[gRPC] Server listening on 127.0.0.1:{IPC_PORT}")
    _logger.info("[gRPC] KV Cache TurboQuant Mode: Active (Q4_0)")
    
    await server.start()
    await server.wait_for_termination()


# ==============================================================================
# Phase 6: Main Entry Point
# ==============================================================================

def main():
    from dotenv import load_dotenv

    base_dir = os.path.dirname(os.path.abspath(__file__))
    env_path = os.path.join(os.path.dirname(base_dir), "openclaw-gateway", ".env")
    load_dotenv(env_path, override=True)

    if os.getenv("AI_PROVIDER") == "openai":
        _logger.info(SEPARATOR)
        _logger.info("[LIVA Native] Cloud API mode -- local engine not needed.")
        _logger.info(SEPARATOR)
        sys.exit(0)

    # Check for grpc tools BEFORE booting up CUDA to save time if missing
    try:
        import grpc
    except ImportError:
        _logger.info("[LIVA Native] FATAL: Missing gRPC! Run: pip install grpcio grpcio-tools")
        sys.exit(1)
        
    try:
        import liva_engine_pb2
    except ImportError:
        _logger.info("[LIVA Native] ERROR: Missing compiled Protobuf interface.")
        _logger.info("[LIVA Native] Generating python proto files dynamically...")
        proto_path = os.path.join(os.path.dirname(base_dir), "openclaw-gateway", "src", "proto", "liva_engine.proto")
        import subprocess
        # Tu dong build file proto trong cung thumuc
        subprocess.run([sys.executable, "-m", "grpc_tools.protoc", 
                        f"-I{os.path.dirname(proto_path)}", 
                        f"--python_out={base_dir}", 
                        f"--grpc_python_out={base_dir}", 
                        proto_path], check=True)
        _logger.info("[LIVA Native] Generated successfully. Restarting engine...")
        sys.exit(0)

    models_dir = os.getenv("AI_MODELS_DIR", r"E:\AI_Models")
    model_name = os.getenv("ROUTER_MODEL_NAME", "gemma-4-E4B-it-Q4_K_M.gguf")
    model_path = os.path.join(models_dir, model_name)

    if not os.path.exists(model_path):
        _logger.info(f"[LIVA Native] FATAL: Model not found: {model_path}")
        sys.exit(1)

    n_ctx = int(os.getenv("NATIVE_N_CTX", "8192"))
    n_gpu = int(os.getenv("NATIVE_N_GPU_LAYERS", "-1"))
    temp = float(os.getenv("NATIVE_TEMPERATURE", "0.7"))
    n_batch = int(os.getenv("NATIVE_N_BATCH", "2048"))
    n_threads = int(os.getenv("NATIVE_N_THREADS", "0"))  # 0 = auto-detect

    _logger.info(SEPARATOR)
    _logger.info("[LIVA] Zero-Overhead Native Inference Engine (gRPC)")
    _logger.info(f"  DLL: {DLL_PATH}")
    _logger.info(f"  Model: {model_path}")
    _logger.info(f"  Config: n_ctx={n_ctx}, n_gpu={n_gpu}, temp={temp}, n_batch={n_batch}, n_threads={n_threads or 'auto'}")
    _logger.info(SEPARATOR)

    engine = LivaNativeEngine(
        model_path=model_path,
        n_ctx=n_ctx,
        n_gpu_layers=n_gpu,
        temperature=temp,
        n_batch=n_batch,
        n_threads=n_threads,
    )

    def signal_handler(sig, frame):
        _logger.info("\n[LIVA Native] Received shutdown signal...")
        engine.shutdown()
        sys.exit(0)

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    try:
        loop = asyncio.get_event_loop()
        loop.run_until_complete(start_ipc_server(engine))
    except KeyboardInterrupt:
        pass
    finally:
        engine.shutdown()


if __name__ == "__main__":
    main()
