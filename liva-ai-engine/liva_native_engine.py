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
import subprocess

_logging.basicConfig(
    level=_logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    stream=sys.stderr
)
_logger = _logging.getLogger("liva_engine")

import grpc  # noqa: E402  — imported early so gRPC method handlers have it in scope

# Dynamic protobuf compilation check
base_dir = os.path.dirname(os.path.abspath(__file__))
try:
    import liva_engine_pb2
    # Verify that the generated file contains the new 'backend' field in SwapModelRequest
    if 'backend' not in liva_engine_pb2.SwapModelRequest.DESCRIPTOR.fields_by_name:
        raise ImportError("backend field not present in SwapModelRequest")
except (ImportError, AttributeError):
    _logger.info("[LIVA Native] Protobuf interface missing or out of date. Compiling from schema...")
    proto_path = os.path.join(os.path.dirname(base_dir), "liva-gateway", "src", "proto", "liva_engine.proto")
    try:
        subprocess.run([
            sys.executable, "-m", "grpc_tools.protoc",
            f"-I{os.path.dirname(proto_path)}",
            f"--python_out={base_dir}",
            f"--grpc_python_out={base_dir}",
            proto_path
        ], check=True)
        # Force reload in case it was cached
        if "liva_engine_pb2" in sys.modules:
            del sys.modules["liva_engine_pb2"]
        import liva_engine_pb2
        _logger.info("[LIVA Native] Protobuf interface compiled and loaded successfully.")
    except Exception as e:
        _logger.error(f"[LIVA Native] Failed to compile protobuf: {e}")


def get_cpu_thread_counts(n_threads: int = 0, n_threads_batch: int = 0) -> tuple[int, int]:
    """Determine thread counts for macOS Apple Silicon or fallbacks."""
    if sys.platform == "darwin":
        p_cores = 0
        try:
            p_res = subprocess.run(
                ["sysctl", "-n", "hw.perflevel0.physicalcpu"],
                capture_output=True,
                text=True,
                check=True
            )
            p_cores = int(p_res.stdout.strip())
        except Exception:
            pass

        if p_cores <= 0:
            logical_cores = os.cpu_count() or 4
            p_cores = max(1, logical_cores // 2)

        res_threads = p_cores if n_threads <= 0 else min(n_threads, p_cores)
        res_threads_batch = p_cores if n_threads_batch <= 0 else min(n_threads_batch, p_cores)
        return (res_threads, res_threads_batch)

    # Non-macOS flow
    input_batch = n_threads_batch
    if n_threads_batch <= 0 and n_threads > 0:
        n_threads_batch = n_threads

    if n_threads > 0 and n_threads_batch > 0:
        return (n_threads, n_threads_batch)

    p_cores = 0
    physical_cores = 0

    logical_cores = os.cpu_count() or 4
    p_cores = max(1, logical_cores // 2)
    physical_cores = logical_cores

    res_threads = n_threads if n_threads > 0 else p_cores
    res_threads_batch = n_threads_batch if n_threads_batch > 0 else physical_cores

    return (res_threads, res_threads_batch)


def _write_debug_prompt(prompt_text: str) -> None:
    with open("debug_prompt.txt", "w", encoding="utf-8") as f:
        f.write(prompt_text)


def is_macos_memory_pressure() -> bool:
    """Detects system memory pressure on macOS using psutil, sysctl, or vm_stat."""
    if os.environ.get("LIVA_DISABLE_MEMORY_PRESSURE_CHECK") == "1":
        return False
    if sys.platform != "darwin":
        return False
    
    # Tier 1: psutil (if installed)
    try:
        import psutil
        vm = psutil.virtual_memory()
        if vm.percent > 80 or vm.available < 2.0 * 1024 * 1024 * 1024:
            return True
    except Exception:
        pass
    
    # Tier 2: sysctl memorystatus
    try:
        res = subprocess.run(["sysctl", "-n", "kern.memorystatus_level"], capture_output=True, text=True, check=False)
        if res.returncode == 0:
            level = int(res.stdout.strip())
            if level < 80:
                return True
    except Exception:
        pass
        
    # Tier 3: vm_stat page counting
    try:
        res = subprocess.run(["vm_stat"], capture_output=True, text=True, check=False)
        if res.returncode == 0:
            lines = res.stdout.splitlines()
            free_pages = 0
            page_size = 4096
            for line in lines:
                if "page size of" in line:
                    parts = line.split("page size of")
                    if len(parts) > 1:
                        page_size = int(parts[1].split()[0])
                if "Pages free:" in line:
                    free_pages = int(line.split()[-1].replace(".", ""))
            if free_pages * page_size < 2.0 * 1024 * 1024 * 1024:
                return True
    except Exception:
        pass

    return False


def get_cpu_topology() -> tuple[int, int]:
    """
    Query CPU topology on macOS to detect physical P-cores and total physical cores.
    Returns (perf_cores, physical_cores).
    On non-macOS, returns (0, 0).
    """
    if sys.platform != "darwin":
        return 0, 0
    
    perf_cores = 0
    physical_cores = 0
    try:
        res = subprocess.run(["sysctl", "-n", "hw.perflevel0.physicalcpu"], capture_output=True, text=True, check=False)
        if res.returncode == 0:
            perf_cores = int(res.stdout.strip())
    except Exception:
        pass
        
    try:
        res = subprocess.run(["sysctl", "-n", "hw.physicalcpu"], capture_output=True, text=True, check=False)
        if res.returncode == 0:
            physical_cores = int(res.stdout.strip())
    except Exception:
        pass
        
    return perf_cores, physical_cores


def should_use_mmap() -> bool:
    """Determines whether memory mapping should be enabled based on env and platform rules."""
    env_mmap = os.environ.get("NATIVE_USE_MMAP")
    if env_mmap is not None:
        return env_mmap.lower() in ("true", "1", "yes")
    
    if sys.platform == "darwin":
        if is_macos_memory_pressure():
            return False
        return True
    
    return True


# Force UTF-8 output on Windows terminals
if sys.platform == "win32" and sys.stdout.encoding != "utf-8":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")

# ==============================================================================
# Phase 1: Locate and Mount the Native DLL
# ==============================================================================

# Constants
SEPARATOR = "=" * 60

NATIVE_LIB_DIR = pathlib.Path(__file__).parent / "native_lib"

if sys.platform == "win32":
    DLL_PATH = NATIVE_LIB_DIR / "llama.dll"
    if not DLL_PATH.exists():
        raise FileNotFoundError(
            f"[LIVA Native Engine] llama.dll not found at {DLL_PATH}.\n"
            f"Run liva_first_run_build.ps1 first to compile from source."
        )
    # Add native_lib to DLL search path so ggml-cuda.dll etc. are found
    if hasattr(os, "add_dll_directory"):
        os.add_dll_directory(str(NATIVE_LIB_DIR))
    os.environ["PATH"] = str(NATIVE_LIB_DIR) + os.pathsep + os.environ.get("PATH", "")
    try:
        ctypes.windll.kernel32.SetDllDirectoryW(str(NATIVE_LIB_DIR))
    except Exception:
        pass
    lib = ctypes.CDLL(str(DLL_PATH), winmode=0)
elif sys.platform == "darwin":
    DLL_PATH = NATIVE_LIB_DIR / "libllama.dylib"
    if not DLL_PATH.exists():
        raise FileNotFoundError(
            f"[LIVA Native Engine] libllama.dylib not found at {DLL_PATH}.\n"
            f"Run build script first to compile from source."
        )
    # Add NATIVE_LIB_DIR to DYLD_LIBRARY_PATH and LD_LIBRARY_PATH environment variables
    os.environ["DYLD_LIBRARY_PATH"] = str(NATIVE_LIB_DIR) + os.pathsep + os.environ.get("DYLD_LIBRARY_PATH", "")
    os.environ["LD_LIBRARY_PATH"] = str(NATIVE_LIB_DIR) + os.pathsep + os.environ.get("LD_LIBRARY_PATH", "")
    lib = ctypes.CDLL(str(DLL_PATH))
else:
    DLL_PATH = NATIVE_LIB_DIR / "libllama.so"
    if not DLL_PATH.exists():
        raise FileNotFoundError(
            f"[LIVA Native Engine] libllama.so not found at {DLL_PATH}.\n"
            f"Run build script first to compile from source."
        )
    lib = ctypes.CDLL(str(DLL_PATH))

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
        ("_padding",                   ctypes.c_char * 512),
    ]


class llama_context_params(ctypes.Structure):
    """Exact byte-layout match for llama_context_params on x64 and ARM64."""
    _fields_ = [
        ("n_ctx",              ctypes.c_uint32),
        ("n_batch",            ctypes.c_uint32),
        ("n_ubatch",           ctypes.c_uint32),
        ("n_seq_max",          ctypes.c_uint32),
        ("n_rs_seq",           ctypes.c_uint32),   # New
        ("n_outputs_max",      ctypes.c_uint32),   # New
        ("n_threads",          ctypes.c_int32),
        ("n_threads_batch",    ctypes.c_int32),
        ("ctx_type",           ctypes.c_int32),    # New (enum llama_context_type)
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
        ("_padding",           ctypes.c_char * 512),
    ]


class llama_sampler_chain_params(ctypes.Structure):
    _fields_ = [
        ("no_perf", ctypes.c_bool),
        ("_padding", ctypes.c_char * 128),
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
        ("_padding", ctypes.c_char * 512),
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

# --- Threads ---
if hasattr(lib, "llama_set_n_threads"):
    lib.llama_set_n_threads.argtypes = [llama_context_p, ctypes.c_int32, ctypes.c_int32]
    lib.llama_set_n_threads.restype = None
    HAS_SET_N_THREADS = True
else:
    HAS_SET_N_THREADS = False

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

# llama_get_memory(ctx) -> void*
try:
    lib.llama_get_memory.argtypes = [llama_context_p]
    lib.llama_get_memory.restype = ctypes.c_void_p
    HAS_GET_MEMORY = True
except AttributeError:
    HAS_GET_MEMORY = False

# llama_n_embd(model) → int32 (embedding dimension of the model)
lib.llama_n_embd.argtypes = [llama_model_p]
lib.llama_n_embd.restype = ctypes.c_int32

# --- KV Cache ---
try:
    lib.llama_kv_cache_clear.argtypes = [llama_context_p]
    lib.llama_kv_cache_clear.restype = None
except AttributeError:
    # Bind modern signatures
    try:
        lib.llama_memory_clear.argtypes = [ctypes.c_void_p, ctypes.c_bool]
        lib.llama_memory_clear.restype = None
        lib.llama_get_memory.argtypes = [llama_context_p]
        lib.llama_get_memory.restype = ctypes.c_void_p

        def fallback_kv_cache_clear(ctx):
            mem = lib.llama_get_memory(ctx)
            if mem:
                lib.llama_memory_clear(mem, True)

        lib.llama_kv_cache_clear = fallback_kv_cache_clear
    except AttributeError:
        lib.llama_kv_cache_clear = lambda ctx: None

try:
    lib.llama_kv_cache_seq_rm.argtypes = [llama_context_p, llama_seq_id, llama_pos, llama_pos]
    lib.llama_kv_cache_seq_rm.restype = ctypes.c_bool
except AttributeError:
    try:
        lib.llama_memory_seq_rm.argtypes = [ctypes.c_void_p, llama_seq_id, llama_pos, llama_pos]
        lib.llama_memory_seq_rm.restype = ctypes.c_bool
        lib.llama_get_memory.argtypes = [llama_context_p]
        lib.llama_get_memory.restype = ctypes.c_void_p

        def fallback_kv_cache_seq_rm(ctx, seq_id, p0, p1):
            mem = lib.llama_get_memory(ctx)
            if mem:
                return lib.llama_memory_seq_rm(mem, seq_id, p0, p1)
            return False

        lib.llama_kv_cache_seq_rm = fallback_kv_cache_seq_rm
    except AttributeError:
        lib.llama_kv_cache_seq_rm = lambda ctx, seq_id, p0, p1: True

try:
    lib.llama_kv_cache_seq_add.argtypes = [llama_context_p, llama_seq_id, llama_pos, llama_pos, llama_pos]
    lib.llama_kv_cache_seq_add.restype = None
except AttributeError:
    try:
        lib.llama_memory_seq_add.argtypes = [ctypes.c_void_p, llama_seq_id, llama_pos, llama_pos, llama_pos]
        lib.llama_memory_seq_add.restype = None
        lib.llama_get_memory.argtypes = [llama_context_p]
        lib.llama_get_memory.restype = ctypes.c_void_p

        def fallback_kv_cache_seq_add(ctx, seq_id, p0, p1, delta):
            mem = lib.llama_get_memory(ctx)
            if mem:
                lib.llama_memory_seq_add(mem, seq_id, p0, p1, delta)

        lib.llama_kv_cache_seq_add = fallback_kv_cache_seq_add
    except AttributeError:
        lib.llama_kv_cache_seq_add = lambda ctx, seq_id, p0, p1, delta: None

try:
    lib.llama_kv_cache_defrag.argtypes = [llama_context_p]
    lib.llama_kv_cache_defrag.restype = None
except AttributeError:
    try:
        lib.llama_memory_defrag.argtypes = [ctypes.c_void_p]
        lib.llama_memory_defrag.restype = None
        lib.llama_get_memory.argtypes = [llama_context_p]
        lib.llama_get_memory.restype = ctypes.c_void_p

        def fallback_kv_cache_defrag(ctx):
            mem = lib.llama_get_memory(ctx)
            if mem:
                lib.llama_memory_defrag(mem)

        lib.llama_kv_cache_defrag = fallback_kv_cache_defrag
    except AttributeError:
        def dummy_defrag(ctx):
            pass
        lib.llama_kv_cache_defrag = dummy_defrag

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
# Phase 4: Unified Backend Interface and Engine Abstractions
# ==============================================================================

from abc import ABC, abstractmethod

class BaseEngine(ABC):
    """
    Abstract Base Class defining the unified interface for LIVA AI Inference backends.
    All implementations must support thread-safe operations, resource management,
    and hot-swapping.
    """

    @abstractmethod
    def tokenize(self, text: str, add_special: bool = True) -> list[int]:
        """Convert input text to a list of token IDs."""
        pass

    @abstractmethod
    def detokenize(self, token_id: int) -> str:
        """Convert a single token ID back to its string representation."""
        pass

    @abstractmethod
    def generate_stream(self, prompt_tokens: list[int], max_tokens: int = 512) -> Generator[str, None, None]:
        """Generates completion text token-by-token. Must be thread-safe."""
        pass

    @abstractmethod
    def generate(self, prompt_tokens: list[int], max_tokens: int = 512) -> str:
        """Synchronous/Unary chat completion generation."""
        pass

    @abstractmethod
    def get_embedding_dim(self) -> int:
        """Returns the output dimension size of the loaded model's embedding vectors."""
        pass

    @abstractmethod
    def get_embeddings_batch(self, texts: list[str]) -> list[list[float]]:
        """Generate L2-normalized embeddings for a batch of strings. Must be thread-safe."""
        pass

    @abstractmethod
    def shutdown(self) -> None:
        """Release all model allocations, contexts, and clear GPU memory (VRAM)."""
        pass


# Lazy loaded imports to avoid crashes on non-macOS systems
mx = None
mlx_lm = None

class LivaMlxEngine(BaseEngine):
    """
    Apple MLX backend implementation using mlx-lm for model loading,
    inference, tokenization, and embedding generation on Apple Silicon.
    """
    
    def __init__(self, model_path: str, n_ctx: int = 8192, **kwargs):
        global mx, mlx_lm
        if mx is None or mlx_lm is None:
            import mlx.core as _mx
            import mlx_lm as _mlx_lm
            mx = _mx
            mlx_lm = _mlx_lm

        self.model_path = model_path
        self.n_ctx = n_ctx
        self._alive = False
        self._engine_mutex = threading.Lock()
        self.temperature = kwargs.get("temperature", 0.7)
        
        _logger.info(f"[LIVA MLX] Loading MLX model from: {model_path}")
        self.model, self.tokenizer = mlx_lm.load(model_path)
        self._alive = True
        _logger.info("[LIVA MLX] Model successfully loaded on Apple Silicon GPU.")

    def tokenize(self, text: str, add_special: bool = True) -> list[int]:
        if not self._alive:
            raise RuntimeError("[LIVA MLX] Engine is not alive — cannot tokenize")
        return self.tokenizer.encode(text, add_special_tokens=add_special)

    def detokenize(self, token_id: int) -> str:
        if not self._alive:
            raise RuntimeError("[LIVA MLX] Engine is not alive — cannot detokenize")
        return self.tokenizer.decode([token_id])

    def generate_stream(self, prompt_tokens: list[int], max_tokens: int = 512) -> Generator[str, None, None]:
        if not self._alive:
            raise RuntimeError("[LIVA MLX] Engine is not alive — cannot generate")

        from mlx_lm.utils import generate_step

        with self._engine_mutex:
            prompt = mx.array(prompt_tokens)
            tokens_generated = 0
            
            for response_token, _ in zip(generate_step(prompt, self.model, self.temperature), range(max_tokens)):
                if not self._alive:
                    break
                
                token_id = response_token.item()
                
                # Check for EOS token
                eos_id = self.tokenizer.eos_token_id
                if isinstance(eos_id, (list, tuple, set)):
                    if token_id in eos_id:
                        break
                elif token_id == eos_id:
                    break

                yield self.tokenizer.decode([token_id])
                tokens_generated += 1

    def generate(self, prompt_tokens: list[int], max_tokens: int = 512) -> str:
        return "".join(self.generate_stream(prompt_tokens, max_tokens))

    def get_embedding_dim(self) -> int:
        if not self._alive:
            raise RuntimeError("[LIVA MLX] Engine is not alive — cannot get embedding dimension")
        if hasattr(self.model, "config") and hasattr(self.model.config, "hidden_size"):
            return self.model.config.hidden_size
        return 2048 # Fallback

    def get_embeddings_batch(self, texts: list[str]) -> list[list[float]]:
        import numpy as np

        if not self._alive:
            raise RuntimeError("[LIVA MLX] Engine is not alive — cannot embed")

        results = []
        with self._engine_mutex:
            for text in texts:
                tokens = self.tokenize(text, add_special=True)
                if not tokens:
                    tokens = [getattr(self.tokenizer, "bos_token_id", None) or 1]
                
                input_ids = mx.array([tokens])
                
                if hasattr(self.model, "model"):
                    hidden_states = self.model.model(input_ids)
                elif hasattr(self.model, "transformer"):
                    hidden_states = self.model.transformer(input_ids)
                else:
                    hidden_states = self.model(input_ids)

                mean_embedding = mx.mean(hidden_states, axis=1)
                vec = np.array(mean_embedding)[0]
                
                norm = np.linalg.norm(vec)
                if norm > 0:
                    vec = vec / norm
                results.append(vec.tolist())
        return results

    def shutdown(self) -> None:
        if not self._alive:
            return
        _logger.info(f"[LIVA MLX] Unloading model {self.model_path} from VRAM...")
        self.model = None
        self.tokenizer = None
        self._alive = False
        import gc
        gc.collect()
        
        global mx
        if mx is not None:
            try:
                mx.metal.clear_cache()
            except Exception:
                pass
        _logger.info("[LIVA MLX] GPU cache cleared and VRAM freed.")


class EngineFactory:
    @staticmethod
    def create_engine(backend: str, model_path: str, **kwargs) -> BaseEngine:
        backend_lower = backend.lower()
        if backend_lower == "mlx":
            return LivaMlxEngine(model_path=model_path, **kwargs)
        elif backend_lower in ("llama.cpp", "native"):
            return LivaNativeEngine(model_path=model_path, **kwargs)
        else:
            raise ValueError(f"Unknown engine backend: {backend}")


class LivaEngineWrapper(BaseEngine):
    """
    A thread-safe proxy wrapper that delegates all engine calls to the active
    backend implementation (LivaNativeEngine or LivaMlxEngine). It facilitates
    on-the-fly engine swapping without restarting the gRPC server process.
    """
    def __init__(self, initial_backend: str, model_path: str, **kwargs):
        self.backend = initial_backend
        self.kwargs = kwargs
        self._wrapper_mutex = threading.Lock()
        self.current_engine = EngineFactory.create_engine(initial_backend, model_path, **kwargs)

    def tokenize(self, text: str, add_special: bool = True) -> list[int]:
        return self.current_engine.tokenize(text, add_special)

    def detokenize(self, token_id: int) -> str:
        return self.current_engine.detokenize(token_id)

    def generate_stream(self, prompt_tokens: list[int], max_tokens: int = 512) -> Generator[str, None, None]:
        return self.current_engine.generate_stream(prompt_tokens, max_tokens)

    def generate(self, prompt_tokens: list[int], max_tokens: int = 512) -> str:
        return self.current_engine.generate(prompt_tokens, max_tokens)

    def get_embedding_dim(self) -> int:
        return self.current_engine.get_embedding_dim()

    def get_embeddings_batch(self, texts: list[str]) -> list[list[float]]:
        return self.current_engine.get_embeddings_batch(texts)

    def hot_swap_model(self, new_model_path: str, n_ctx: int = 0, n_gpu_layers: int = -1, backend: str = None) -> tuple[bool, str, int]:
        """
        Dynamically swaps the active engine class and loads the new model.
        """
        start_ns = time.monotonic_ns()
        
        # Auto-detect backend if not explicitly specified
        if backend is None or backend == "":
            if new_model_path.endswith(".gguf") or "gguf" in new_model_path.lower():
                backend = "llama.cpp"
            else:
                backend = "mlx"

        with self._wrapper_mutex:
            try:
                _logger.info(f"[EngineWrapper] Swapping backend from {self.backend} to {backend}...")
                
                # 1. Gracefully shutdown old engine to free VRAM
                self.current_engine.shutdown()
                import gc
                gc.collect()
                gc.collect()
                
                # 2. Update config parameters
                if n_ctx > 0:
                    self.kwargs["n_ctx"] = n_ctx
                if n_gpu_layers != -1:
                    self.kwargs["n_gpu_layers"] = n_gpu_layers

                # 3. Load new engine type
                self.current_engine = EngineFactory.create_engine(backend, new_model_path, **self.kwargs)
                self.backend = backend
                
                duration_ms = (time.monotonic_ns() - start_ns) // 1_000_000
                _logger.info(f"[EngineWrapper] Dynamic swap to {backend} successful.")
                return (True, os.path.basename(new_model_path), duration_ms)
            except Exception as e:
                _logger.error(f"[EngineWrapper] Hot-swap failed: {str(e)}")
                duration_ms = (time.monotonic_ns() - start_ns) // 1_000_000
                return (False, str(e), duration_ms)

    def shutdown(self) -> None:
        with self._wrapper_mutex:
            self.current_engine.shutdown()

    async def vram_guard_loop(self):
        """Monitors system for heavy apps and yields VRAM when detected."""
        if sys.platform not in ("win32", "darwin"):
            return
        _logger.info("[VRAM Guard] Daemon loop started.")
        is_yielded = False
        while True:
            try:
                # Polling interval
                await asyncio.sleep(10)
                
                heavy_app_detected = False
                
                # Check running processes
                if sys.platform == "win32":
                    output = await asyncio.to_thread(
                        subprocess.check_output, 
                        ["tasklist", "/FO", "CSV", "/NH"], 
                        creationflags=0x08000000, # CREATE_NO_WINDOW
                        timeout=5,
                        text=True
                    )
                    output_str = str(output)
                    for line in output_str.strip().split("\n"):
                        if not line: continue
                        parts = line.split(",")
                        if parts:
                            proc_name = parts[0].strip('"').lower()
                            if proc_name.endswith(".exe"):
                                proc_name = proc_name[:-4]
                            if proc_name in LivaNativeEngine.HEAVY_APPS:
                                heavy_app_detected = True
                                _logger.info(f"[VRAM Guard] Detected heavy app: {proc_name}")
                                break
                elif sys.platform == "darwin":
                    output = await asyncio.to_thread(
                        subprocess.check_output,
                        ["ps", "-ax", "-o", "comm"],
                        timeout=5,
                        text=True
                    )
                    lines = output.strip().split("\n")
                    for line in lines:
                        cmd_path = line.strip()
                        if not cmd_path or cmd_path == "COMM":
                            continue
                        
                        base_name = os.path.basename(cmd_path)
                        if base_name.endswith(".app"):
                            base_name = base_name[:-4]
                            
                        proc_lower = base_name.lower()
                        path_lower = cmd_path.lower()
                        
                        is_heavy = False
                        if proc_lower == "xcode" or "/xcode.app/" in path_lower:
                            is_heavy = True
                        elif proc_lower == "blender" or "/blender.app/" in path_lower:
                            is_heavy = True
                        elif proc_lower == "studio" and "/android studio.app/" in path_lower:
                            is_heavy = True
                        elif proc_lower == "resolve" or "/davinci resolve/" in path_lower:
                            is_heavy = True
                        elif proc_lower == "code" or "/visual studio code.app/" in path_lower:
                            is_heavy = True
                            
                        if not is_heavy and proc_lower in LivaNativeEngine.HEAVY_APPS:
                            is_heavy = True
                            
                        if is_heavy:
                            heavy_app_detected = True
                            _logger.info(f"[VRAM Guard] Detected heavy app on macOS: {base_name} (path: {cmd_path})")
                            break
                            
                if heavy_app_detected and not is_yielded:
                    _logger.warning("[VRAM Guard] 🎮 Heavy app detected. Yielding VRAM.")
                    def _safe_shutdown():
                        self.shutdown()
                    await asyncio.to_thread(_safe_shutdown)
                    is_yielded = True
                elif not heavy_app_detected and is_yielded:
                    _logger.info("[VRAM Guard] ✅ Heavy app exited. Restart engine manually or via OS supervisor.")
                    sys.exit(0)
                    
            except Exception as e:
                _logger.debug(f"[VRAM Guard] Polling error: {e}")


# ==============================================================================
# Phase 4.1: LivaNativeEngine -- Zero-Overhead Inference Core
# ==============================================================================

class LivaNativeEngine(BaseEngine):
    """
    Native inference engine using direct ctypes CFFI calls to llama.dll.
    All memory is allocated on the C++ heap. Python only touches pointers.
    """

    # --- Hardware Resource Daemon (VRAM Guard) ---
    HEAVY_APPS = {
        "blackmythwukong", "cyberpunk2077", "eldenring", "starfield",
        "hogwartslegacy", "baldursgate3", "rdr2", "gtav", "witcher3",
        "cs2", "valorant", "overwatch", "fortnite", "pubg",
        "dota2", "leagueoflegends", "apexlegends", "callofduty",
        "palworld", "enshrouded", "helldivers2", "blackops6",
        "blender", "unrealengine", "unity", "davinciresolve", "resolve",
        "afterfx", "premiere", "nuke", "houdini", "maya",
        "3dsmax", "cinema4d", "substance",
    }

    def __init__(self, model_path: str, n_ctx: int = 8192, n_gpu_layers: int = -1,
                 n_batch: int = 2048, n_threads: int = 0, n_threads_batch: int = 0,
                 n_ubatch: int = 512, flash_attn: bool = True, temperature: float = 0.7,
                 top_p: float = 0.9, top_k: int = 40, min_p: float = 0.05):
        # Auto-detect CPU threads if not specified (0 = auto)
        n_threads, n_threads_batch = get_cpu_thread_counts(n_threads, n_threads_batch)

        self._alive = False
        self.n_batch = n_batch
        self.n_ubatch = n_ubatch
        self.n_threads = n_threads
        self.n_threads_batch = n_threads_batch
        self.n_ctx = n_ctx  # Store for prompt overflow guard
        self.n_gpu_layers = n_gpu_layers
        self.has_sampler_reset = hasattr(lib, 'llama_sampler_reset')
        self.has_sampler_accept = hasattr(lib, 'llama_sampler_accept')
        # OS-level mutex: asyncio.Lock only serializes on the event loop,
        # but asyncio.to_thread() runs generate() on OS thread pool.
        # Without this, concurrent gRPC calls (StreamChat + Chat Unary)
        # can both touch C++ engine state simultaneously → NULL deref crash.
        self._engine_mutex = threading.RLock()
        # Separate mutex for dedicated embedding context — allows concurrent
        # chat generation + embedding when embed_ctx is available
        self._embed_mutex = threading.RLock()
        self._recreate_mutex = threading.RLock()
        _logger.info("[LIVA Native] Initializing Zero-Overhead Engine...")
        _logger.info(f"  Model: {model_path}")
        _logger.info(f"  Context: {n_ctx} | GPU Layers: {n_gpu_layers} | Flash Attn: {flash_attn}")

        # Initialize backend
        lib.llama_backend_init()

        # Get default model params and modify
        model_params = lib.llama_model_default_params()
        model_params.n_gpu_layers = n_gpu_layers
        model_params.use_mmap = should_use_mmap()
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
        ctx_params.n_ubatch = n_ubatch
        ctx_params.n_threads = n_threads
        ctx_params.n_threads_batch = n_threads_batch
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
        
        self.n_ubatch = ctx_params.n_ubatch
        self.ctx_params = ctx_params

        # Create context
        self.ctx = lib.llama_init_from_model(self.model, ctx_params)

        if not self.ctx:
            raise RuntimeError("[LIVA Native] FATAL: Failed to create context")

        actual_ctx = lib.llama_n_ctx(self.ctx)
        _logger.info(f"  Context created: n_ctx={actual_ctx}")

        # Create a dedicated, separate context for embedding generation
        # sharing the SAME model weights to prevent KV cache conflicts with chat!
        self.embed_ctx_params = None
        self.embed_memory = None
        
        _model_basename = os.path.basename(model_path).lower()
        _is_large_model = any(tag in _model_basename for tag in ["26b", "27b", "32b", "70b", "expert"])
        if _is_large_model:
            self.embed_ctx = None
            self.embed_ctx_params = None
            _logger.info("[LIVA Native] Skipping dedicated embed_ctx for large model (VRAM conservation). Embeddings will use CPU ONNX fallback.")
        else:
            try:
                embed_ctx_params = lib.llama_context_default_params()
                embed_ctx_params.n_ctx = min(512, n_ctx)
                embed_ctx_params.n_batch = min(512, n_batch)
                embed_ctx_params.n_ubatch = min(n_ubatch, embed_ctx_params.n_batch)
                embed_ctx_params.n_threads = n_threads
                embed_ctx_params.n_threads_batch = n_threads_batch
                embed_ctx_params.flash_attn_type = 1 if flash_attn else 0
                embed_ctx_params.offload_kqv = True
                embed_ctx_params.op_offload = True
                embed_ctx_params.embeddings = True
                embed_ctx_params.pooling_type = 1
                embed_ctx_params.type_k = 2
                embed_ctx_params.type_v = 2
                
                self.embed_ctx_params = embed_ctx_params
                self.embed_ctx = lib.llama_init_from_model(self.model, embed_ctx_params)
                if self.embed_ctx:
                    _logger.info("[LIVA Native] Dedicated embedding context successfully created.")
                    if HAS_GET_MEMORY:
                        self.embed_memory = lib.llama_get_memory(self.embed_ctx)
                else:
                    self.embed_ctx = None
                    self.embed_memory = None
                    _logger.warning("[LIVA Native] Failed to create dedicated embedding context, falling back to shared context.")
            except Exception as e:
                self.embed_ctx = None
                self.embed_memory = None
                _logger.warning(f"[LIVA Native] Failed to create dedicated embedding context: {e}. Falling back to shared context.")

        # Initialize draft model for speculative decoding if active
        self._init_draft_model(n_ctx, n_gpu_layers, n_batch, n_threads, flash_attn)

        # Initialize sampler chain
        self.temperature = temperature
        self.top_p = top_p
        self.top_k = top_k
        self.min_p = min_p
        self._init_sampler()

        self._alive = True
        _logger.info(f"[LIVA Native] Engine ready. EOS={self.eos_token}, BOS={self.bos_token}")

    def _init_draft_model(self, n_ctx, n_gpu_layers, n_batch, n_threads, flash_attn):
        self.draft_model = None
        self.draft_ctx = None
        self.draft_sampler = None
        
        enable_speculative = os.getenv("LIVA_ENABLE_SPECULATIVE", "false").lower() == "true"
        draft_model_name = os.getenv("LIVA_DRAFT_MODEL_NAME", "")
        if enable_speculative and draft_model_name:
            if sys.platform == "darwin":
                models_dir = os.getenv("AI_MODELS_DIR", os.path.expanduser("~/AI_Models"))
            else:
                models_dir = os.getenv("AI_MODELS_DIR", r"E:\AI_Models")
            draft_model_path = os.path.join(models_dir, draft_model_name)
            if os.path.exists(draft_model_path):
                _logger.info(f"[LIVA Native] Loading draft model from {draft_model_path}...")
                draft_model_params = lib.llama_model_default_params()
                draft_model_params.n_gpu_layers = n_gpu_layers
                draft_model_params.use_mmap = should_use_mmap()
                draft_model_params.use_mlock = False
                
                self.draft_model = lib.llama_model_load_from_file(draft_model_path.encode("utf-8"), draft_model_params)
                if self.draft_model:
                    draft_ctx_params = lib.llama_context_default_params()
                    draft_ctx_params.n_ctx = n_ctx
                    draft_ctx_params.n_batch = n_batch
                    draft_ctx_params.n_ubatch = n_batch
                    if sys.platform == "darwin":
                        draft_ctx_params.n_threads = max(1, n_threads // 2)
                        draft_ctx_params.n_threads_batch = max(1, n_threads // 2)
                    else:
                        draft_ctx_params.n_threads = n_threads
                        draft_ctx_params.n_threads_batch = n_threads
                    draft_ctx_params.flash_attn_type = 1 if flash_attn else 0
                    draft_ctx_params.offload_kqv = True
                    draft_ctx_params.op_offload = True
                    draft_ctx_params.type_k = 2
                    draft_ctx_params.type_v = 2
                    
                    self.draft_ctx_params = draft_ctx_params
                    self.draft_ctx = lib.llama_init_from_model(self.draft_model, draft_ctx_params)
                    if self.draft_ctx:
                        self.draft_n_threads = draft_ctx_params.n_threads
                        self.draft_n_threads_batch = draft_ctx_params.n_threads_batch
                        draft_sparams = lib.llama_sampler_chain_default_params()
                        self.draft_sampler = lib.llama_sampler_chain_init(draft_sparams)
                        lib.llama_sampler_chain_add(self.draft_sampler, lib.llama_sampler_init_greedy())
                        _logger.info("[LIVA Native] Draft model and context successfully initialized.")
                    else:
                        _logger.error("[LIVA Native] Failed to initialize draft context.")
                else:
                    _logger.error(f"[LIVA Native] Failed to load draft model from {draft_model_path}.")
            else:
                _logger.warning(f"[LIVA Native] Draft model file not found at {draft_model_path}. Disabling speculative decoding.")

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
            if not self._alive or self.ctx is None:
                raise RuntimeError("[LIVA Native] Engine is not alive — cannot generate")
            try:
                yield from self._generate_stream_unsafe(prompt_tokens, max_tokens)
            except BaseException as e:
                self._cached_tokens = None
                raise e

    def _adjust_threads_hardware_adaptive(self):
        if not HAS_SET_N_THREADS:
            return
        try:
            import psutil
            cpu_load = psutil.cpu_percent(interval=None)
            if cpu_load is not None and cpu_load > 80.0:
                target_threads = max(1, self.n_threads - 2)
                target_threads_batch = max(1, self.n_threads_batch - 2)
                lib.llama_set_n_threads(self.ctx, target_threads, target_threads_batch)
                
                draft_ctx = getattr(self, "draft_ctx", None)
                if draft_ctx is not None:
                    draft_n_threads = getattr(self, "draft_n_threads", self.n_threads // 2)
                    draft_n_threads_batch = getattr(self, "draft_n_threads_batch", self.n_threads_batch // 2)
                    draft_target = max(1, draft_n_threads - 2)
                    draft_target_batch = max(1, draft_n_threads_batch - 2)
                    lib.llama_set_n_threads(draft_ctx, draft_target, draft_target_batch)
                _logger.debug(f"[Hardware-Adaptive Threads] High CPU load ({cpu_load}%). Throttled threads: ctx={target_threads}, batch={target_threads_batch}")
            else:
                lib.llama_set_n_threads(self.ctx, self.n_threads, self.n_threads_batch)
                
                draft_ctx = getattr(self, "draft_ctx", None)
                if draft_ctx is not None:
                    draft_n_threads = getattr(self, "draft_n_threads", self.n_threads // 2)
                    draft_n_threads_batch = getattr(self, "draft_n_threads_batch", self.n_threads_batch // 2)
                    lib.llama_set_n_threads(draft_ctx, draft_n_threads, draft_n_threads_batch)
        except Exception as e:
            _logger.debug(f"[Hardware-Adaptive Threads] Error adjusting threads: {e}")

    def _restore_threads_defaults(self):
        if not HAS_SET_N_THREADS:
            return
        try:
            lib.llama_set_n_threads(self.ctx, self.n_threads, self.n_threads_batch)
            draft_ctx = getattr(self, "draft_ctx", None)
            if draft_ctx is not None:
                draft_n_threads = getattr(self, "draft_n_threads", self.n_threads // 2)
                draft_n_threads_batch = getattr(self, "draft_n_threads_batch", self.n_threads_batch // 2)
                lib.llama_set_n_threads(draft_ctx, draft_n_threads, draft_n_threads_batch)
        except Exception as e:
            _logger.debug(f"[Hardware-Adaptive Threads] Error restoring threads: {e}")

    def _generate_stream_unsafe(self, prompt_tokens: list[int], max_tokens: int = 512) -> Generator[str, None, None]:
        """
        Internal generation — MUST be called under self._engine_mutex.
        """
        # 1. Reset Sampler
        if self.has_sampler_reset:
            lib.llama_sampler_reset(self.sampler)
            if hasattr(self, "draft_sampler") and self.draft_sampler is not None:
                lib.llama_sampler_reset(self.draft_sampler)
        else:
            lib.llama_sampler_free(self.sampler)
            self._init_sampler()

        # Find common prefix with previously cached tokens
        # SWA / Metal safety: llama.cpp Metal backend offloading has index alignment issues 
        # when trimming KV cache via seq_rm on models using Sliding Window Attention (SWA).
        # We disable prefix-cache matching when offloading to GPU (n_gpu_layers != 0) 
        # to prevent SIGTRAP (exit code 133) crashes on subsequent decodes.
        is_gpu = getattr(self, "n_gpu_layers", -1) != 0
        n_past = 0
        common_len = 0
        if not is_gpu and hasattr(self, "_cached_tokens") and self._cached_tokens:
            # Find how many tokens match from the start
            max_possible = min(len(self._cached_tokens), len(prompt_tokens))
            for i in range(max_possible):
                if self._cached_tokens[i] == prompt_tokens[i]:
                    common_len += 1
                else:
                    break
            
            # If we have a common prefix, we can reuse it!
            if common_len > 0:
                if common_len == len(prompt_tokens):
                    # Force at least 1 token to be evaluated so we get fresh logits for sampling
                    common_len -= 1
                # Remove everything in the KV cache after the common prefix (Sequence ID = 0)
                lib.llama_kv_cache_seq_rm(self.ctx, 0, common_len, -1)
                if hasattr(self, "draft_ctx") and self.draft_ctx is not None:
                    lib.llama_kv_cache_seq_rm(self.draft_ctx, 0, common_len, -1)
                n_past = common_len
                _logger.info(f"[KV Cache] Prefill hit! Reusing {common_len} cached tokens. Evaluating only {len(prompt_tokens) - common_len} new tokens.")
        
        # If we didn't reuse anything (or no previous cache), clear the entire KV Cache
        if common_len == 0:
            is_dirty = hasattr(self, "_cached_tokens") and bool(self._cached_tokens)
            if is_gpu and is_dirty:
                _logger.info("[KV Cache] GPU / Metal mode detected with dirty KV cache. Recreating llama context(s) to safely clear KV cache.")
                if self.ctx:
                    lib.llama_free(self.ctx)
                self.ctx = lib.llama_init_from_model(self.model, self.ctx_params)
                if not self.ctx:
                    raise RuntimeError("[LIVA Native] FATAL: Failed to recreate context during KV cache reset")
                
                if hasattr(self, "draft_ctx") and self.draft_ctx is not None:
                    lib.llama_free(self.draft_ctx)
                    draft_model = getattr(self, "draft_model", None)
                    draft_ctx_params = getattr(self, "draft_ctx_params", None)
                    if draft_model is not None and draft_ctx_params is not None:
                        self.draft_ctx = lib.llama_init_from_model(draft_model, draft_ctx_params)
                        if not self.draft_ctx:
                            raise RuntimeError("[LIVA Native] FATAL: Failed to recreate draft context during KV cache reset")
                    else:
                        try:
                            from unittest.mock import MagicMock
                            self.draft_ctx = MagicMock()
                        except ImportError:
                            self.draft_ctx = None
            else:
                lib.llama_kv_cache_clear(self.ctx)
                if hasattr(self, "draft_ctx") and self.draft_ctx is not None:
                    lib.llama_kv_cache_clear(self.draft_ctx)
            n_past = 0
            _logger.info(f"[KV Cache] Prefill miss. Evaluating entire {len(prompt_tokens)} tokens from scratch.")

        # Save the new prompt tokens to the cache for next turn
        self._cached_tokens = list(prompt_tokens)

        # Adjust threads based on CPU load before prefill starts
        self._adjust_threads_hardware_adaptive()

        # Prompt ingestion (Memory-Safe Chunking)
        prefill_tokens = prompt_tokens[common_len:]
        total_prefill = len(prefill_tokens)
        prefill_arr = (llama_token * total_prefill)(*prefill_tokens)
        
        # CẤP PHÁT RAM VẬT LÝ BẰNG llama_batch_init (Kích thước max = self.n_batch)
        batch = lib.llama_batch_init(self.n_batch, 0, 1)
        
        try:
            # --- VÒNG LẶP NẠP PROMPT (CHUNKING) ---
            idx = 0
            last_batch_n_tokens = 0
            while idx < total_prefill:
                chunk_size = min(self.n_batch, total_prefill - idx)
                last_batch_n_tokens = chunk_size
                
                batch.n_tokens = chunk_size
                for i in range(chunk_size):
                    batch.token[i] = prefill_arr[idx + i]
                    batch.pos[i] = n_past + i
                    batch.n_seq_id[i] = 1
                    batch.seq_id[i][0] = 0
                    batch.logits[i] = 0
                
                # TỐI ƯU HIỆU NĂNG: Chỉ bật cờ tính Logits cho token cuối cùng của TOÀN BỘ prompt
                if idx + chunk_size == total_prefill:
                    batch.logits[chunk_size - 1] = 1
                    
                rc = lib.llama_decode(self.ctx, batch)
                if rc != 0:
                    raise RuntimeError(f"[LIVA Native] llama_decode failed during prompt ingestion (rc={rc})")
                
                if hasattr(self, "draft_ctx") and self.draft_ctx is not None:
                    rc_draft = lib.llama_decode(self.draft_ctx, batch)
                    if rc_draft != 0:
                        raise RuntimeError(f"[LIVA Native] llama_decode failed on draft context during prompt ingestion (rc={rc_draft})")
                
                n_past += chunk_size
                idx += chunk_size

            # --- VÒNG LẶP SINH TOKEN (AUTOREGRESSIVE) ---
            sampler_idx = max(0, last_batch_n_tokens - 1)
            
            # Decide if speculative decoding is active
            use_speculative = (hasattr(self, "draft_ctx") and self.draft_ctx is not None)
            
            draft_batch = None
            if use_speculative:
                draft_batch = lib.llama_batch_init(1, 0, 1)
                
            try:
                tokens_generated = 0
                next_check_token = 8
                while tokens_generated < max_tokens:
                    # Periodic thread adjustment and memory pressure check (every 8 tokens)
                    if tokens_generated >= next_check_token:
                        self._adjust_threads_hardware_adaptive()
                        if is_macos_memory_pressure():
                            _logger.warning("[Memory Protection] macOS memory pressure detected during token generation. Stopping generation and reclaiming resources.")
                            # Clear main KV cache
                            lib.llama_kv_cache_clear(self.ctx)
                            # Unload draft model/ctx/sampler
                            if getattr(self, "draft_sampler", None) is not None:
                                lib.llama_sampler_free(self.draft_sampler)
                                self.draft_sampler = None
                            if getattr(self, "draft_ctx", None) is not None:
                                lib.llama_free(self.draft_ctx)
                                self.draft_ctx = None
                            if getattr(self, "draft_model", None) is not None:
                                lib.llama_model_free(self.draft_model)
                                self.draft_model = None
                            # Unload dedicate embedding context
                            if getattr(self, "embed_ctx", None) is not None:
                                with self._embed_mutex:
                                    with self._recreate_mutex:
                                        if getattr(self, "embed_ctx", None) is not None:
                                            lib.llama_free(self.embed_ctx)
                                            self.embed_ctx = None
                            self.embed_memory = None
                            # Force speculative decoding to be inactive if it was
                            use_speculative = False
                            # Force garbage collection
                            import gc
                            gc.collect()
                            break
                        next_check_token = ((tokens_generated // 8) + 1) * 8

                    # Safety limit is n_ctx - 6 to accommodate up to 5 draft tokens + 1 corrected token
                    safety_limit = self.n_ctx - 6 if use_speculative else self.n_ctx
                    if n_past >= safety_limit:
                        S = min(512, self.n_ctx // 8)
                        K = min(512, self.n_ctx // 8)
                        if n_past > S + K:
                            _logger.info(f"[KV Cache] Pruning KV cache: n_past={n_past}, S={S}, K={K}")
                            lib.llama_kv_cache_seq_rm(self.ctx, 0, S, S + K)
                            lib.llama_kv_cache_seq_rm(self.ctx, 0, n_past - 1, n_past)
                            lib.llama_kv_cache_seq_add(self.ctx, 0, S + K, n_past - 1, -K)
                            lib.llama_kv_cache_defrag(self.ctx)
                            
                            if use_speculative:
                                lib.llama_kv_cache_seq_rm(self.draft_ctx, 0, S, S + K)
                                lib.llama_kv_cache_seq_rm(self.draft_ctx, 0, n_past - 1, n_past)
                                lib.llama_kv_cache_seq_add(self.draft_ctx, 0, S + K, n_past - 1, -K)
                                lib.llama_kv_cache_defrag(self.draft_ctx)
                                
                            n_past -= K
                            if hasattr(self, "_cached_tokens") and self._cached_tokens:
                                self._cached_tokens = self._cached_tokens[:S] + self._cached_tokens[S + K:]
                            
                            # Re-evaluate the last token of the new prompt to get fresh logits for sampling
                            last_tok = self._cached_tokens[-1]
                            batch.n_tokens = 1
                            batch.token[0] = last_tok
                            batch.pos[0] = n_past - 1
                            batch.n_seq_id[0] = 1
                            batch.seq_id[0][0] = 0
                            batch.logits[0] = 1
                            
                            rc = lib.llama_decode(self.ctx, batch)
                            if rc != 0:
                                raise RuntimeError(f"[LIVA Native] llama_decode failed during KV cache pruning (rc={rc})")
                            if use_speculative:
                                rc_draft = lib.llama_decode(self.draft_ctx, batch)
                                if rc_draft != 0:
                                    raise RuntimeError(f"[LIVA Native] llama_decode failed on draft context during KV cache pruning (rc={rc_draft})")
                            sampler_idx = 0

                    if use_speculative:
                        # Autoregressively draft H candidate tokens
                        H = getattr(self, "draft_len", 5)
                        drafted_tokens = []
                        n_past_draft = n_past
                        curr_draft_sampler_idx = sampler_idx
                        
                        for h in range(H):
                            draft_tok = lib.llama_sampler_sample(self.draft_sampler, self.draft_ctx, curr_draft_sampler_idx)
                            if self.has_sampler_accept:
                                lib.llama_sampler_accept(self.draft_sampler, draft_tok)
                            
                            if draft_tok == self.eos_token:
                                break
                            drafted_tokens.append(draft_tok)
                                
                            # Decode draft token
                            draft_batch.n_tokens = 1
                            draft_batch.token[0] = draft_tok
                            draft_batch.pos[0] = n_past_draft
                            draft_batch.n_seq_id[0] = 1
                            draft_batch.seq_id[0][0] = 0
                            draft_batch.logits[0] = 1
                            
                            rc = lib.llama_decode(self.draft_ctx, draft_batch)
                            if rc != 0:
                                break
                            n_past_draft += 1
                            curr_draft_sampler_idx = 0
                            
                        # Fallback if draft yields nothing
                        if not drafted_tokens:
                            new_token = lib.llama_sampler_sample(self.sampler, self.ctx, sampler_idx)
                            if new_token == self.eos_token:
                                break
                            text = self.detokenize(new_token)
                            self._cached_tokens.append(new_token)
                            if self.has_sampler_accept:
                                lib.llama_sampler_accept(self.sampler, new_token)
                            yield text
                            tokens_generated += 1
                            
                            batch.n_tokens = 1
                            batch.token[0] = new_token
                            batch.pos[0] = n_past
                            batch.n_seq_id[0] = 1
                            batch.seq_id[0][0] = 0
                            batch.logits[0] = 1
                            
                            rc = lib.llama_decode(self.ctx, batch)
                            if rc != 0:
                                raise RuntimeError(f"[LIVA Native] Target model llama_decode failed during speculative fallback (rc={rc})")
                            rc_draft = lib.llama_decode(self.draft_ctx, batch)
                            if rc_draft != 0:
                                raise RuntimeError(f"[LIVA Native] Draft model llama_decode failed during speculative fallback (rc={rc_draft})")
                            n_past += 1
                            sampler_idx = 0
                            continue
                            
                        # Batch decode draft tokens on target context
                        batch.n_tokens = len(drafted_tokens)
                        for i in range(len(drafted_tokens)):
                            batch.token[i] = drafted_tokens[i]
                            batch.pos[i] = n_past + i
                            batch.n_seq_id[i] = 1
                            batch.seq_id[i][0] = 0
                            batch.logits[i] = 1
                            
                        # Sample the first target token BEFORE llama_decode overwrites the context logits
                        target_tok = lib.llama_sampler_sample(self.sampler, self.ctx, sampler_idx)

                        rc = lib.llama_decode(self.ctx, batch)
                        if rc != 0:
                            raise RuntimeError(f"[LIVA Native] Target model llama_decode failed during speculative batch verification (rc={rc})")
                            
                        # Verify draft tokens
                        accepted_count = 0
                        last_target_token = None
                        if target_tok == drafted_tokens[0]:
                            accepted_count = 1
                            for i in range(1, len(drafted_tokens)):
                                target_tok = lib.llama_sampler_sample(self.sampler, self.ctx, i - 1)
                                if target_tok == drafted_tokens[i]:
                                    accepted_count += 1
                                else:
                                    last_target_token = target_tok
                                    break
                            if last_target_token is None and accepted_count == len(drafted_tokens):
                                last_target_token = lib.llama_sampler_sample(self.sampler, self.ctx, len(drafted_tokens) - 1)
                        else:
                            last_target_token = target_tok
                            
                        stop_generation = False
                        
                        # Yield matched tokens
                        for i in range(accepted_count):
                            tok = drafted_tokens[i]
                            text = self.detokenize(tok)
                            self._cached_tokens.append(tok)
                            if self.has_sampler_accept:
                                lib.llama_sampler_accept(self.sampler, tok)
                                lib.llama_sampler_accept(self.draft_sampler, tok)
                            yield text
                            tokens_generated += 1
                            if tok == self.eos_token:
                                stop_generation = True
                                break
                                
                        if stop_generation:
                            lib.llama_kv_cache_seq_rm(self.ctx, 0, n_past + accepted_count, -1)
                            lib.llama_kv_cache_seq_rm(self.draft_ctx, 0, n_past + accepted_count, -1)
                            break
                            
                        # Yield corrected token
                        text = self.detokenize(last_target_token)
                        self._cached_tokens.append(last_target_token)
                        if self.has_sampler_accept:
                            lib.llama_sampler_accept(self.sampler, last_target_token)
                            lib.llama_sampler_accept(self.draft_sampler, last_target_token)
                        yield text
                        tokens_generated += 1
                        if last_target_token == self.eos_token:
                            lib.llama_kv_cache_seq_rm(self.ctx, 0, n_past + accepted_count, -1)
                            lib.llama_kv_cache_seq_rm(self.draft_ctx, 0, n_past + accepted_count, -1)
                            break
                            
                        # Align KV caches
                        lib.llama_kv_cache_seq_rm(self.ctx, 0, n_past + accepted_count, -1)
                        lib.llama_kv_cache_seq_rm(self.draft_ctx, 0, n_past + accepted_count, -1)
                        
                        # Decode last_target_token on both
                        batch.n_tokens = 1
                        batch.token[0] = last_target_token
                        batch.pos[0] = n_past + accepted_count
                        batch.n_seq_id[0] = 1
                        batch.seq_id[0][0] = 0
                        batch.logits[0] = 1
                        
                        rc = lib.llama_decode(self.ctx, batch)
                        if rc != 0:
                            raise RuntimeError(f"[LIVA Native] Target model llama_decode failed during speculative alignment (rc={rc})")
                        rc_draft = lib.llama_decode(self.draft_ctx, batch)
                        if rc_draft != 0:
                            raise RuntimeError(f"[LIVA Native] Draft model llama_decode failed during speculative alignment (rc={rc_draft})")
                            
                        n_past += accepted_count + 1
                        sampler_idx = 0
                        
                    else:
                        # Standard non-speculative generation
                        new_token = lib.llama_sampler_sample(self.sampler, self.ctx, sampler_idx)
                        if new_token == self.eos_token:
                            break
                        text = self.detokenize(new_token)
                        self._cached_tokens.append(new_token)
                        if self.has_sampler_accept:
                            lib.llama_sampler_accept(self.sampler, new_token)
                        yield text
                        tokens_generated += 1
                        
                        batch.n_tokens = 1
                        batch.token[0] = new_token
                        batch.pos[0] = n_past
                        batch.n_seq_id[0] = 1
                        batch.seq_id[0][0] = 0
                        batch.logits[0] = 1
                        
                        rc = lib.llama_decode(self.ctx, batch)
                        if rc != 0:
                            raise RuntimeError(f"[LIVA Native] llama_decode failed (rc={rc})")
                        n_past += 1
                        sampler_idx = 0
            finally:
                if draft_batch is not None:
                    lib.llama_batch_free(draft_batch)
                
        finally:
            self._restore_threads_defaults()
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

        # When dedicated embed_ctx exists, use separate mutex to avoid
        # blocking chat generation. Both contexts share model weights (read-only)
        # but have independent KV caches — safe for concurrent access.
        has_dedicated = hasattr(self, "embed_ctx") and self.embed_ctx is not None
        if has_dedicated:
            with self._embed_mutex:
                if self.embed_ctx is not None:
                    return self._get_embeddings_batch_unsafe(texts, n_embd, np, use_dedicated=True)

        with self._engine_mutex:
            if not self._alive or self.ctx is None:
                raise RuntimeError("[LIVA Native] Engine is not alive — cannot fall back to shared embedding context")
            return self._get_embeddings_batch_unsafe(texts, n_embd, np, use_dedicated=False)

    def _get_embeddings_batch_unsafe(self, texts: list[str], n_embd: int, np, use_dedicated: bool = False) -> list[list[float]]:
        """
        Internal embedding — MUST be called under self._engine_mutex.
        This serializes with generate_stream/generate to prevent C++ segfault.
        """
        results = []

        # Determine active context for embedding pass
        # [Fix 1] Context recreation failure safety guard
        if getattr(self, "embed_ctx", None) is None and getattr(self, "embed_ctx_params", None) is not None:
            with self._embed_mutex:
                with self._recreate_mutex:
                    if getattr(self, "embed_ctx", None) is None:
                        new_ctx = lib.llama_init_from_model(self.model, self.embed_ctx_params)
                        if new_ctx:
                            self.embed_ctx = new_ctx
                            if HAS_GET_MEMORY:
                                self.embed_memory = lib.llama_get_memory(self.embed_ctx)
                        else:
                            _logger.error("Failed to recreate dedicated embedding context. Falling back to shared context.")
                            self.embed_memory = None

        if use_dedicated:
            active_embed_ctx = getattr(self, "embed_ctx", None)
            if active_embed_ctx is None:
                raise RuntimeError("[LIVA Native] Dedicated embedding context is None under dedicated lock.")
        else:
            active_embed_ctx = self.ctx
            if active_embed_ctx is None:
                raise RuntimeError("[LIVA Native] Shared context (self.ctx) is None — cannot generate embeddings")
        is_fallback = not use_dedicated

        # 1. Clear KV Cache for clean embedding pass
        if is_fallback:
            self._cached_tokens = None  # type: ignore
        lib.llama_kv_cache_clear(active_embed_ctx)

        # 2. Allocate batch buffer (reused across all texts)
        # [Fix 3] Redundant Sequence Allocation in llama_batch_init: simplified to 1
        batch = lib.llama_batch_init(self.n_batch, 0, 1)

        try:
            if len(texts) == 1 and HAS_GET_EMBEDDINGS:
                # --- Single text fast path ---
                tokens = self.tokenize(texts[0], add_special=True)
                if not tokens:
                    tokens = [self.bos_token]
                # [Fix 4] Dynamic Truncation Size Guard
                active_n_ctx = lib.llama_n_ctx(active_embed_ctx)
                limit = max(0, min(active_n_ctx - 4, self.n_batch))
                if len(tokens) > limit:
                    tokens = tokens[:limit]

                batch.n_tokens = len(tokens)
                for i, tok in enumerate(tokens):
                    batch.token[i] = tok
                    batch.pos[i] = i
                    batch.n_seq_id[i] = 1
                    batch.seq_id[i][0] = 0
                    batch.logits[i] = 1  # [FIX] Mark ALL tokens as output for Mean Pooling

                rc = lib.llama_decode(active_embed_ctx, batch)
                if rc != 0:
                    # [Fix 1] Fallback context recreation on decode failure
                    if not is_fallback and getattr(self, "embed_ctx_params", None) is not None:
                        with self._recreate_mutex:
                            if getattr(self, "embed_ctx", None) == active_embed_ctx:
                                old_ctx = getattr(self, "embed_ctx", None)
                                self.embed_ctx = None
                                if old_ctx is not None:
                                    lib.llama_free(old_ctx)

                                new_ctx = lib.llama_init_from_model(self.model, self.embed_ctx_params)
                                if new_ctx:
                                    self.embed_ctx = new_ctx
                                    if HAS_GET_MEMORY:
                                        self.embed_memory = lib.llama_get_memory(self.embed_ctx)
                                else:
                                    _logger.error("Failed to recreate dedicated embedding context. Falling back to shared context.")
                                    self.embed_memory = None
                    raise RuntimeError(f"llama_decode failed for embedding (rc={rc})")

                # Extract embedding pointer
                if HAS_GET_EMBEDDINGS_SEQ:
                    embd_ptr = lib.llama_get_embeddings_seq(active_embed_ctx, 0)
                else:
                    embd_ptr = lib.llama_get_embeddings(active_embed_ctx)

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
                    if not tokens:
                        tokens = [self.bos_token]
                    # [Fix 4] Dynamic Truncation Size Guard
                    active_n_ctx = lib.llama_n_ctx(active_embed_ctx)
                    limit = max(0, min(active_n_ctx - 4, self.n_batch))
                    if len(tokens) > limit:
                        tokens = tokens[:limit]

                    # Clear previous batch state for reuse
                    batch.n_tokens = len(tokens)
                    for i, tok in enumerate(tokens):
                        batch.token[i] = tok
                        batch.pos[i] = i
                        batch.n_seq_id[i] = 1
                        batch.seq_id[i][0] = 0  # [CRITICAL FIX] Force slot 0 — n_seq_max=1
                        batch.logits[i] = 1     # [FIX] Mark ALL tokens as output for Mean Pooling

                    rc = lib.llama_decode(active_embed_ctx, batch)
                    if rc != 0:
                        # [Fix 1] Fallback context recreation on decode failure in loop
                        if not is_fallback and getattr(self, "embed_ctx_params", None) is not None:
                            with self._recreate_mutex:
                                if getattr(self, "embed_ctx", None) == active_embed_ctx:
                                    old_ctx = getattr(self, "embed_ctx", None)
                                    self.embed_ctx = None
                                    if old_ctx is not None:
                                        lib.llama_free(old_ctx)

                                    new_ctx = lib.llama_init_from_model(self.model, self.embed_ctx_params)
                                    if new_ctx:
                                        self.embed_ctx = new_ctx
                                        if HAS_GET_MEMORY:
                                            self.embed_memory = lib.llama_get_memory(self.embed_ctx)
                                    else:
                                        _logger.error("Failed to recreate dedicated embedding context. Falling back to shared context.")
                                        self.embed_memory = None
                        raise RuntimeError(f"llama_decode failed for text #{seq_idx} (rc={rc})")

                    # Extract embedding from slot 0 (always)
                    if HAS_GET_EMBEDDINGS_SEQ:
                        embd_ptr = lib.llama_get_embeddings_seq(active_embed_ctx, 0)  # [CRITICAL FIX] Always slot 0
                    else:
                        embd_ptr = lib.llama_get_embeddings(active_embed_ctx)

                    if not embd_ptr:
                        raise RuntimeError(f"llama_get_embeddings returned NULL for text #{seq_idx}")

                    vec = np.ctypeslib.as_array(embd_ptr, shape=(n_embd,)).copy()
                    # L2 normalize
                    norm = np.linalg.norm(vec)
                    if norm > 0:
                        vec /= norm
                    results.append(vec.tolist())

                    # Clear KV between sequences to prevent position collision
                    lib.llama_kv_cache_clear(active_embed_ctx)

        finally:
            lib.llama_batch_free(batch)

        return results

    def shutdown(self, *, _keep_backend: bool = False):
        """
        RAII cleanup -- free all C++ heap allocations.
        
        Args:
            _keep_backend: If True, skip llama_backend_free() so the backend
                           can be reused for hot-swap. Internal use only.
        """
        if not self._alive:
            return
        with self._engine_mutex:
            with self._embed_mutex:
                with self._recreate_mutex:
                    if not self._alive:
                        return
                    _logger.info("[LIVA Native] Shutting down engine...")
                    if hasattr(self, "sampler") and self.sampler:
                        lib.llama_sampler_free(self.sampler)
                        self.sampler = None
                    if hasattr(self, "draft_sampler") and self.draft_sampler:
                        lib.llama_sampler_free(self.draft_sampler)
                        self.draft_sampler = None
                    if hasattr(self, "embed_ctx") and self.embed_ctx:
                        lib.llama_free(self.embed_ctx)
                        self.embed_ctx = None
                    self.embed_memory = None
                    if hasattr(self, "draft_ctx") and self.draft_ctx:
                        lib.llama_free(self.draft_ctx)
                        self.draft_ctx = None
                    if hasattr(self, "ctx") and self.ctx:
                        lib.llama_free(self.ctx)
                        self.ctx = None
                    if hasattr(self, "draft_model") and self.draft_model:
                        lib.llama_model_free(self.draft_model)
                        self.draft_model = None
                    if hasattr(self, "model") and self.model:
                        lib.llama_model_free(self.model)
                        self.model = None
                    # Invalidate KV cache tracking
                    if hasattr(self, "_cached_tokens"):
                        self._cached_tokens = None
                    if not _keep_backend:
                        lib.llama_backend_free()
                    self._alive = False
                    _logger.info("[LIVA Native] Engine shutdown complete.")

    def __del__(self):
        self.shutdown()

    # ===================================================================
    # [v29] Hot-Swap Model — Sequential Single Model on VRAM
    # ===================================================================

    def hot_swap_model(self, new_model_path: str, n_ctx: int = 0, n_gpu_layers: int = -1) -> tuple[bool, str, int]:
        """
        Hot-swap model: shutdown current model → force GC → load new model.
        Only ONE model on VRAM at any time. Thread-safe: acquires both mutexes.
        
        Args:
            new_model_path: Absolute path to the new GGUF model file.
            n_ctx: Context length for new model (0 = reuse current n_ctx).
            n_gpu_layers: GPU layers for new model (-1 = offload all).
            
        Returns:
            (success: bool, error_message: str, swap_duration_ms: int)
        """
        if not os.path.exists(new_model_path):
            return (False, f"Model file not found: {new_model_path}", 0)

        import gc
        import time as _time
        
        start_ns = _time.monotonic_ns()
        target_n_ctx = n_ctx if n_ctx > 0 else self.n_ctx
        
        # Preserve constructor params for re-init
        saved_n_batch = self.n_batch
        saved_n_ubatch = self.n_ubatch
        saved_n_threads = self.n_threads
        saved_n_threads_batch = self.n_threads_batch
        saved_temperature = self.temperature
        saved_top_p = self.top_p
        saved_top_k = self.top_k
        saved_min_p = self.min_p
        saved_flash_attn = hasattr(self, 'ctx_params') and getattr(self.ctx_params, 'flash_attn_type', 0) > 0
        
        _logger.info(f"[Hot-Swap] === BEGIN: Swapping to {os.path.basename(new_model_path)} ===")
        _logger.info(f"[Hot-Swap] Config: n_ctx={target_n_ctx}, n_gpu={n_gpu_layers}, n_batch={saved_n_batch}")

        # Acquire BOTH mutexes to block all concurrent operations
        with self._engine_mutex:
            with self._embed_mutex:
                try:
                    # ── Step 1: Shutdown current model (keep backend alive for reuse) ──
                    _logger.info("[Hot-Swap] Step 1/4: Unloading current model...")
                    self.shutdown(_keep_backend=True)
                    
                    # ── Step 2: Force Python GC to release C++ allocated memory ──
                    _logger.info("[Hot-Swap] Step 2/4: Forcing garbage collection...")
                    gc.collect()
                    gc.collect()  # Double collect for weak refs and C++ pointers
                    
                    # ── Step 3: Load new model (reuse backend, mmap=True for OS file cache) ──
                    _logger.info(f"[Hot-Swap] Step 3/4: Loading new model from {new_model_path}...")
                    
                    if not os.path.exists(new_model_path):
                        raise FileNotFoundError(f"Model file not found: {new_model_path}")
                    
                    # Re-init model params (mmap=True for fast reload from OS cache)
                    model_params = lib.llama_model_default_params()
                    model_params.n_gpu_layers = n_gpu_layers
                    model_params.use_mmap = should_use_mmap()  # [Optimization D] OS File Cache acceleration
                    model_params.use_mlock = False
                    
                    encoded_path = new_model_path.encode("utf-8")
                    self.model = lib.llama_model_load_from_file(encoded_path, model_params)
                    
                    if not self.model:
                        raise RuntimeError(f"Failed to load model from {new_model_path}")
                    
                    # Get model description
                    desc_buf = ctypes.create_string_buffer(256)
                    lib.llama_model_desc(self.model, desc_buf, 256)
                    _logger.info(f"[Hot-Swap] Model loaded: {desc_buf.value.decode('utf-8', errors='replace')}")
                    
                    # Get vocab handle
                    self.vocab = lib.llama_model_get_vocab(self.model)
                    self.eos_token = lib.llama_vocab_eos(self.vocab)
                    self.bos_token = lib.llama_vocab_bos(self.vocab)
                    
                    # Create context
                    ctx_params = lib.llama_context_default_params()
                    ctx_params.n_ctx = target_n_ctx
                    ctx_params.n_batch = saved_n_batch
                    ctx_params.n_ubatch = saved_n_ubatch
                    ctx_params.n_threads = saved_n_threads
                    ctx_params.n_threads_batch = saved_n_threads_batch
                    ctx_params.flash_attn_type = 1 if saved_flash_attn else 0
                    ctx_params.offload_kqv = True
                    ctx_params.op_offload = True
                    ctx_params.embeddings = True
                    ctx_params.pooling_type = 1  # Mean pooling
                    ctx_params.type_k = 2  # Q4_0 KV cache
                    ctx_params.type_v = 2
                    
                    self.ctx_params = ctx_params
                    self.ctx = lib.llama_init_from_model(self.model, ctx_params)
                    
                    if not self.ctx:
                        raise RuntimeError("Failed to create context for new model")
                    
                    self.n_ctx = target_n_ctx
                    self.n_batch = saved_n_batch
                    self.n_ubatch = ctx_params.n_ubatch
                    self.n_threads = saved_n_threads
                    self.n_threads_batch = saved_n_threads_batch
                    
                    # Dedicated embedding context
                    # [Optimization A2] Skip embed_ctx for large Expert models to save VRAM at OOM boundary.
                    # Gateway falls back to CPU ONNX EmbeddingService (all-MiniLM-L6-v2) automatically.
                    _model_basename = os.path.basename(new_model_path).lower()
                    _is_large_model = any(tag in _model_basename for tag in ["26b", "27b", "32b", "70b", "expert"])
                    if _is_large_model:
                        self.embed_ctx = None
                        self.embed_ctx_params = None
                        _logger.info("[Hot-Swap] Skipping dedicated embed_ctx for large model (VRAM conservation). Embeddings will use CPU ONNX fallback.")
                    else:
                        try:
                            embed_ctx_params = lib.llama_context_default_params()
                            embed_ctx_params.n_ctx = min(512, target_n_ctx)
                            embed_ctx_params.n_batch = min(512, saved_n_batch)
                            embed_ctx_params.n_ubatch = min(512, saved_n_batch)
                            embed_ctx_params.n_threads = saved_n_threads
                            embed_ctx_params.n_threads_batch = saved_n_threads
                            embed_ctx_params.flash_attn_type = 1 if saved_flash_attn else 0
                            embed_ctx_params.offload_kqv = True
                            embed_ctx_params.op_offload = True
                            embed_ctx_params.embeddings = True
                            embed_ctx_params.pooling_type = 1
                            embed_ctx_params.type_k = 2
                            embed_ctx_params.type_v = 2
                            
                            self.embed_ctx_params = embed_ctx_params
                            self.embed_ctx = lib.llama_init_from_model(self.model, embed_ctx_params)
                            if self.embed_ctx:
                                if HAS_GET_MEMORY:
                                    self.embed_memory = lib.llama_get_memory(self.embed_ctx)
                                else:
                                    self.embed_memory = None
                            else:
                                self.embed_ctx = None
                                self.embed_memory = None
                        except Exception as e:
                            self.embed_ctx = None
                            self.embed_memory = None
                            _logger.warning(f"[Hot-Swap] Failed to create embed context: {e}")
                    
                    # Initialize draft model for speculative decoding if active
                    self._init_draft_model(target_n_ctx, n_gpu_layers, saved_n_batch, saved_n_threads, saved_flash_attn)

                    # ── Step 4: Re-init sampler ──
                    _logger.info("[Hot-Swap] Step 4/4: Initializing sampler...")
                    self.temperature = saved_temperature
                    self.top_p = saved_top_p
                    self.top_k = saved_top_k
                    self.min_p = saved_min_p
                    self._init_sampler()
                    
                    # Reset KV cache tracking
                    self._cached_tokens = None
                    self._alive = True
                    
                    duration_ms = (_time.monotonic_ns() - start_ns) // 1_000_000
                    _logger.info(f"[Hot-Swap] === COMPLETE: {os.path.basename(new_model_path)} loaded in {duration_ms}ms ===")
                    return (True, "", duration_ms)
                    
                except Exception as e:
                    err_msg = str(e)
                    duration_ms = (_time.monotonic_ns() - start_ns) // 1_000_000
                    _logger.info(f"[Hot-Swap] === FAILED: {err_msg} (after {duration_ms}ms) ===")
                    self._alive = False
                    return (False, err_msg, duration_ms)


    # --- Hardware Daemon Background Loop ---
    async def vram_guard_loop(self):
        """Monitors system for heavy apps and yields VRAM when detected."""
        if sys.platform not in ("win32", "darwin"):
            return
        _logger.info("[VRAM Guard] Daemon loop started.")
        is_yielded = False
        while True:
            try:
                # Polling interval
                await asyncio.sleep(10)
                
                heavy_app_detected = False
                
                # Check running processes
                if sys.platform == "win32":
                    output = await asyncio.to_thread(
                        subprocess.check_output, 
                        ["tasklist", "/FO", "CSV", "/NH"], 
                        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
                        timeout=5,
                        text=True
                    )
                    output_str = str(output)
                    for line in output_str.strip().split("\n"):
                        if not line: continue
                        parts = line.split(",")
                        if parts:
                            proc_name = parts[0].strip('"').lower()
                            if proc_name.endswith(".exe"):
                                proc_name = proc_name[:-4]
                            if proc_name in self.HEAVY_APPS:
                                heavy_app_detected = True
                                _logger.info(f"[VRAM Guard] Detected heavy app: {proc_name}")
                                break
                elif sys.platform == "darwin":
                    output = await asyncio.to_thread(
                        subprocess.check_output,
                        ["ps", "-ax", "-o", "comm"],
                        timeout=5,
                        text=True
                    )
                    lines = output.strip().split("\n")
                    for line in lines:
                        cmd_path = line.strip()
                        if not cmd_path or cmd_path == "COMM":
                            continue
                        
                        base_name = os.path.basename(cmd_path)
                        if base_name.endswith(".app"):
                            base_name = base_name[:-4]
                            
                        proc_lower = base_name.lower()
                        path_lower = cmd_path.lower()
                        
                        is_heavy = False
                        if proc_lower == "xcode" or "/xcode.app/" in path_lower:
                            is_heavy = True
                        elif proc_lower == "blender" or "/blender.app/" in path_lower:
                            is_heavy = True
                        elif proc_lower == "studio" and "/android studio.app/" in path_lower:
                            is_heavy = True
                        elif proc_lower == "resolve" or "/davinci resolve/" in path_lower:
                            is_heavy = True
                        elif proc_lower == "code" or "/visual studio code.app/" in path_lower:
                            is_heavy = True
                            
                        if not is_heavy and proc_lower in self.HEAVY_APPS:
                            is_heavy = True
                            
                        if is_heavy:
                            heavy_app_detected = True
                            _logger.info(f"[VRAM Guard] Detected heavy app on macOS: {base_name} (path: {cmd_path})")
                            break
                            
                if heavy_app_detected and not is_yielded:
                    _logger.warning("[VRAM Guard] 🎮 Heavy app detected. Yielding VRAM.")
                    def _safe_shutdown():
                        with self._engine_mutex:
                            with self._embed_mutex:
                                self.shutdown()
                    await asyncio.to_thread(_safe_shutdown)
                    is_yielded = True
                elif not heavy_app_detected and is_yielded:
                    _logger.info("[VRAM Guard] ✅ Heavy app exited. Restart engine manually or via OS supervisor.")
                    # Let Gateway's Circuit Breaker or start_all.ps1 handle restart
                    sys.exit(0)
                    
            except Exception as e:
                _logger.debug(f"[VRAM Guard] Polling error: {e}")


# ==============================================================================
# Phase 5: gRPC-over-HTTP/2 Server (Zero-Overhead IPC replacing TCP/JSONL)
# ==============================================================================

IPC_PORT = 8100

class LivaInferenceServicer:
    def __init__(self, engine: BaseEngine):
        self.engine = engine
        self.engine_lock = asyncio.Lock()   # Serializes StreamChat/Chat calls
        self.embed_lock = asyncio.Lock()    # Serializes Embed calls (independent when embed_ctx exists)

    async def StreamChat(self, request, context):  # NOSONAR - gRPC method: PascalCase required to match protobuf service definition
        import liva_engine_pb2
        
        req_id = request.request_id or "g_req"
        prompt_text = ""
        
        # Build prompt from messages using standard Gemma/ChatML format
        # Gemma-4B does not natively support '<start_of_turn>system'.
        # We merge system messages into the first user turn if possible.
        messages = list(request.messages)
        system_content = ""
        merged_messages = []
        for msg in messages:
            role = msg.role if msg.role else "user"
            if role == "system":
                if system_content:
                    system_content += "\n" + msg.content
                else:
                    system_content = msg.content
            else:
                merged_messages.append((role, msg.content))
        
        if system_content:
            if merged_messages and merged_messages[0][0] == "user":
                merged_messages[0] = ("user", f"{system_content}\n\n{merged_messages[0][1]}")
            else:
                merged_messages.insert(0, ("user", system_content))

        for role, content in merged_messages:
            if role == "assistant":
                role = "model"
            prompt_text += f"<start_of_turn>{role}\n{content}<end_of_turn>\n"
        prompt_text += "<start_of_turn>model\n"

        # Use to_thread to avoid blocking the event loop with synchronous I/O
        await asyncio.to_thread(_write_debug_prompt, prompt_text)

        max_tokens = request.max_tokens if request.max_tokens > 0 else 2048

        async with self.engine_lock:
            tokens = self.engine.tokenize(prompt_text)
            _logger.info(f"[gRPC StreamChat] Received prompt with {len(tokens)} tokens. Max tokens: {request.max_tokens}")
            if len(tokens) > 0:
                _logger.info(f"[gRPC StreamChat] First 50 chars of prompt: {prompt_text[:50]!r}")
                _logger.info(f"[gRPC StreamChat] Last 100 chars of prompt: {prompt_text[-100:]!r}")
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
                drained_count = 0
                try:
                    while drained_count < 8:
                        next_chunk = queue.get_nowait()
                        if next_chunk is None:
                            has_stop = True
                            break
                        batch_buf += next_chunk
                        drained_count += 1
                except asyncio.QueueEmpty:
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
                        delta = liva_engine_pb2.ChunkDelta(content=remaining_safe)  # type: ignore
                        if chunk_idx == 0:
                            delta.role = "assistant"
                        choice = liva_engine_pb2.ChunkChoice(index=0, delta=delta, finish_reason="")  # type: ignore
                        yield liva_engine_pb2.ChatCompletionChunk(  # type: ignore
                            id=req_id, object="chat.completion.chunk", model="liva-native", choices=[choice]
                        )
                    yielded_length = len(full_text)
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
                    
                    delta = liva_engine_pb2.ChunkDelta(content=safe_text)  # type: ignore
                    if chunk_idx == 0:
                        delta.role = "assistant"
                        
                    choice = liva_engine_pb2.ChunkChoice(index=0, delta=delta, finish_reason="")  # type: ignore
                    yield liva_engine_pb2.ChatCompletionChunk(  # type: ignore
                        id=req_id, object="chat.completion.chunk", model="liva-native", choices=[choice]
                    )
                    chunk_idx += 1

                if has_stop:
                    break

            # Flush remaining buffer (if any)
            if yielded_length < len(full_text):
                remaining_safe = full_text[yielded_length:]
                if remaining_safe:
                    delta = liva_engine_pb2.ChunkDelta(content=remaining_safe)  # type: ignore
                    if chunk_idx == 0:
                        delta.role = "assistant"
                    choice = liva_engine_pb2.ChunkChoice(index=0, delta=delta, finish_reason="")  # type: ignore
                    yield liva_engine_pb2.ChatCompletionChunk(  # type: ignore
                        id=req_id, object="chat.completion.chunk", model="liva-native", choices=[choice]
                    )

            # Final chunk with finish reason
            final_choice = liva_engine_pb2.ChunkChoice(  # type: ignore
                index=0,
                delta=liva_engine_pb2.ChunkDelta(),  # type: ignore
                finish_reason="stop"
            )
            yield liva_engine_pb2.ChatCompletionChunk(  # type: ignore
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
        
        # Build prompt from messages using standard Gemma/ChatML format
        # Gemma-4B does not natively support '<start_of_turn>system'.
        # We merge system messages into the first user turn if possible.
        messages = list(request.messages)
        system_content = ""
        merged_messages = []
        for msg in messages:
            role = msg.role if msg.role else "user"
            if role == "system":
                if system_content:
                    system_content += "\n" + msg.content
                else:
                    system_content = msg.content
            else:
                merged_messages.append((role, msg.content))
        
        if system_content:
            if merged_messages and merged_messages[0][0] == "user":
                merged_messages[0] = ("user", f"{system_content}\n\n{merged_messages[0][1]}")
            else:
                merged_messages.insert(0, ("user", system_content))

        for role, content in merged_messages:
            if role == "assistant":
                role = "model"
            prompt_text += f"<start_of_turn>{role}\n{content}<end_of_turn>\n"
        prompt_text += "<start_of_turn>model\n"

        max_tokens = request.max_tokens if request.max_tokens > 0 else 512

        async with self.engine_lock:
            tokens = self.engine.tokenize(prompt_text)
            result_text = await asyncio.to_thread(self.engine.generate, tokens, max_tokens)
        
        # Strip trailing stop sequences
        stop_triggers = ["<start_of_turn>", "<end_of_turn>", "end_of_turn>", "<|user|>", "<|im_start|>"]
        for trigger in stop_triggers:
            if trigger in result_text:
                result_text = result_text.split(trigger)[0]
        
        choice = liva_engine_pb2.ChatCompletionChoice(  # type: ignore
            index=0,
            message=liva_engine_pb2.ChatMessage(role="assistant", content=result_text),  # type: ignore
            finish_reason="stop"
        )
        
        return liva_engine_pb2.ChatCompletionResponse(  # type: ignore
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
        return liva_engine_pb2.HealthResponse(  # type: ignore
            alive=True,
            model_name="LIVA Engine",
            uptime_seconds=0,
            vram_usage_mb=0.0,
            kv_cache_type=_KV_CACHE_Q4_0
        )

    async def Embed(self, request, context):  # NOSONAR - gRPC method: PascalCase required to match protobuf service definition
        """
        gRPC Embed handler — generates L2-normalized embeddings via dedicated GPU context.
        Uses embed_lock (async) + engine._embed_mutex (OS-level) to serialize embedding calls
        INDEPENDENTLY from Chat/StreamChat — allowing concurrent chat + embedding.
        """
        import liva_engine_pb2
        import grpc  # [FIX] Prevent NameError when C++ crashes in except block

        texts = list(request.input)
        if not texts:
            return liva_engine_pb2.EmbeddingResponse(data=[], model="liva-native", dimensions=0)  # type: ignore

        n_embd = 0
        try:
            # Use embed_lock instead of engine_lock — allows concurrent chat generation
            async with self.embed_lock:
                n_embd = self.engine.get_embedding_dim()
                _logger.info(f"[gRPC Embed] Processing {len(texts)} text(s), dim={n_embd}")
                vectors = await asyncio.to_thread(self.engine.get_embeddings_batch, texts)

            data = []
            for idx, vec in enumerate(vectors):
                data.append(liva_engine_pb2.EmbeddingData(  # type: ignore
                    embedding=vec,
                    index=idx
                ))

            return liva_engine_pb2.EmbeddingResponse(  # type: ignore
                data=data,
                model="liva-native",
                dimensions=n_embd
            )
        except Exception as e:
            _logger.info(f"[gRPC Embed] ERROR: {str(e)}")
            context.set_code(grpc.StatusCode.INTERNAL)
            context.set_details(f"Embedding failed: {str(e)}")
            return liva_engine_pb2.EmbeddingResponse(data=[], model="liva-native", dimensions=n_embd)  # type: ignore

    async def SwapModel(self, request, context):  # NOSONAR - gRPC method: PascalCase required to match protobuf service definition
        """
        [v29] Hot-Swap handler — unload current model, force GC, load new model.
        Acquires BOTH engine_lock AND embed_lock to block all concurrent operations.
        Only ONE model on VRAM at any time.
        """
        import liva_engine_pb2
        import grpc as _grpc

        model_path = request.model_path
        n_ctx = request.n_ctx or 0  # 0 = reuse current
        n_gpu = request.n_gpu_layers if request.n_gpu_layers != 0 else -1
        backend = getattr(request, "backend", None) or None

        _logger.info(f"[gRPC SwapModel] Request: model={os.path.basename(model_path)}, n_ctx={n_ctx}, n_gpu={n_gpu}, backend={backend}")

        if not model_path:
            context.set_code(_grpc.StatusCode.INVALID_ARGUMENT)
            context.set_details("model_path is required")
            return liva_engine_pb2.SwapModelResponse(  # type: ignore
                success=False, error_message="model_path is required",
                loaded_model="", swap_duration_ms=0
            )

        try:
            # Acquire both async locks to block Chat/StreamChat/Embed during swap
            async with self.engine_lock:
                async with self.embed_lock:
                    success, err_msg, duration_ms = await asyncio.to_thread(
                        self.engine.hot_swap_model, model_path, n_ctx, n_gpu, backend
                    )

            return liva_engine_pb2.SwapModelResponse(  # type: ignore
                success=success,
                error_message=err_msg,
                loaded_model=os.path.basename(model_path) if success else "",
                swap_duration_ms=duration_ms
            )
        except Exception as e:
            _logger.info(f"[gRPC SwapModel] ERROR: {str(e)}")
            context.set_code(_grpc.StatusCode.INTERNAL)
            context.set_details(f"SwapModel failed: {str(e)}")
            return liva_engine_pb2.SwapModelResponse(  # type: ignore
                success=False, error_message=str(e),
                loaded_model="", swap_duration_ms=0
            )


async def start_ipc_server(engine: BaseEngine):
    """Start the gRPC async server."""
    import grpc
    import liva_engine_pb2_grpc
    
    server = grpc.aio.server(
        options=[
            ('grpc.max_send_message_length', 50 * 1024 * 1024),
            ('grpc.max_receive_message_length', 50 * 1024 * 1024),
            ('grpc.http2.min_ping_interval_without_data_ms', 5000),
            ('grpc.keepalive_permit_without_calls', 1),
        ]
    )
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
    env_path = os.path.join(os.path.dirname(base_dir), "liva-gateway", ".env")
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
        proto_path = os.path.join(os.path.dirname(base_dir), "liva-gateway", "src", "proto", "liva_engine.proto")
        import subprocess
        # Tu dong build file proto trong cung thumuc
        subprocess.run([sys.executable, "-m", "grpc_tools.protoc", 
                        f"-I{os.path.dirname(proto_path)}", 
                        f"--python_out={base_dir}", 
                        f"--grpc_python_out={base_dir}", 
                        proto_path], check=True)
        _logger.info("[LIVA Native] Generated successfully. Restarting engine...")
        sys.exit(0)

    if sys.platform == "darwin":
        models_dir = os.getenv("AI_MODELS_DIR", os.path.expanduser("~/AI_Models"))
    else:
        models_dir = os.getenv("AI_MODELS_DIR", r"E:\AI_Models")
    model_name = os.getenv("ROUTER_MODEL_NAME", "gemma-4-E4B-it-Q6_K.gguf")
    model_path = os.path.join(models_dir, model_name)

    if not os.path.exists(model_path):
        _logger.info(f"[LIVA Native] FATAL: Model not found: {model_path}")
        sys.exit(1)

    n_ctx = int(os.getenv("NATIVE_N_CTX", "8192"))
    n_gpu = int(os.getenv("NATIVE_N_GPU_LAYERS", "-1"))
    temp = float(os.getenv("NATIVE_TEMPERATURE", "0.7"))
    n_batch = int(os.getenv("NATIVE_N_BATCH", "2048"))
    n_threads = int(os.getenv("NATIVE_N_THREADS", "0"))  # 0 = auto-detect
    n_threads_batch = int(os.getenv("NATIVE_N_THREADS_BATCH", "0"))
    n_ubatch = int(os.getenv("NATIVE_N_UBATCH", "512"))
    flash_attn = os.getenv("NATIVE_FLASH_ATTN", "true").lower() == "true"

    _logger.info(SEPARATOR)
    _logger.info("[LIVA] Zero-Overhead Native Inference Engine (gRPC)")
    _logger.info(f"  DLL: {DLL_PATH}")
    _logger.info(f"  Model: {model_path}")
    _logger.info(f"  Config: n_ctx={n_ctx}, n_gpu={n_gpu}, temp={temp}, n_batch={n_batch}, n_ubatch={n_ubatch}, n_threads={n_threads or 'auto'}, n_threads_batch={n_threads_batch or 'auto'}, flash_attn={flash_attn}")
    _logger.info(SEPARATOR)

    initial_backend = os.getenv("LIVA_ENGINE_BACKEND", "")
    if not initial_backend:
        if model_path.endswith(".gguf") or "gguf" in model_name.lower():
            initial_backend = "llama.cpp"
        else:
            initial_backend = "mlx"

    engine = LivaEngineWrapper(
        initial_backend=initial_backend,
        model_path=model_path,
        n_ctx=n_ctx,
        n_gpu_layers=n_gpu,
        temperature=temp,
        n_batch=n_batch,
        n_threads=n_threads,
        n_threads_batch=n_threads_batch,
        n_ubatch=n_ubatch,
        flash_attn=flash_attn,
    )

    def signal_handler(sig, frame):
        _logger.info("\n[LIVA Native] Received shutdown signal...")
        engine.shutdown()
        sys.exit(0)

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    try:
        loop = asyncio.get_event_loop()
        # Start the background VRAM Guard daemon loop alongside gRPC
        loop.create_task(engine.vram_guard_loop())
        loop.run_until_complete(start_ipc_server(engine))
    except KeyboardInterrupt:
        pass
    finally:
        engine.shutdown()


if __name__ == "__main__":
    main()
