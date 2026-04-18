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
import json
import ctypes
import ctypes.util
import pathlib
import asyncio
import signal
import time
from typing import List, Optional, Generator

# Force UTF-8 output on Windows terminals
if sys.platform == "win32" and sys.stdout.encoding != "utf-8":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")

# ==============================================================================
# Phase 1: Locate and Mount the Native DLL
# ==============================================================================

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

# --- KV Cache ---
try:
    lib.llama_kv_self_clear.argtypes = [llama_context_p]
    lib.llama_kv_self_clear.restype = None
    HAS_KV_SELF_CLEAR = True
except AttributeError:
    HAS_KV_SELF_CLEAR = False

# --- Sampler ---
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
                 n_batch: int = 4096, n_threads: int = 4,
                 flash_attn: bool = True, temperature: float = 0.7,
                 top_p: float = 0.9, top_k: int = 40, min_p: float = 0.05):
        self._alive = False
        print(f"[LIVA Native] Initializing Zero-Overhead Engine...")
        print(f"  Model: {model_path}")
        print(f"  Context: {n_ctx} | GPU Layers: {n_gpu_layers} | Flash Attn: {flash_attn}")

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
        print(f"  Model loaded: {desc_buf.value.decode('utf-8', errors='replace')}")

        # Get vocab handle
        self.vocab = lib.llama_model_get_vocab(self.model)
        self.eos_token = lib.llama_vocab_eos(self.vocab)
        self.bos_token = lib.llama_vocab_bos(self.vocab)

        # Get default context params and modify
        ctx_params = lib.llama_context_default_params()
        ctx_params.n_ctx = n_ctx
        ctx_params.n_batch = n_batch
        ctx_params.n_threads = n_threads
        ctx_params.n_threads_batch = n_threads
        # Flash attention: 0=disabled, 1=enabled, 2=auto
        ctx_params.flash_attn_type = 1 if flash_attn else 0
        ctx_params.offload_kqv = True
        ctx_params.op_offload = True

        # Create context
        self.ctx = lib.llama_init_from_model(self.model, ctx_params)

        if not self.ctx:
            raise RuntimeError("[LIVA Native] FATAL: Failed to create context")

        actual_ctx = lib.llama_n_ctx(self.ctx)
        print(f"  Context created: n_ctx={actual_ctx}")

        # Initialize sampler chain
        self.temperature = temperature
        self.top_p = top_p
        self.top_k = top_k
        self.min_p = min_p
        self._init_sampler()

        self._alive = True
        print(f"[LIVA Native] Engine ready. EOS={self.eos_token}, BOS={self.bos_token}")

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

    def tokenize(self, text: str, add_special: bool = True) -> List[int]:
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

    def generate_stream(self, prompt_tokens: List[int], max_tokens: int = 512) -> Generator[str, None, None]:
        """
        Zero-overhead autoregressive generation.
        Yields detokenized text chunks as they are generated.
        """
        # Clear KV cache for fresh generation
        if HAS_KV_SELF_CLEAR:
            lib.llama_kv_self_clear(self.ctx)

        # 1. Prompt ingestion
        prompt_arr = (llama_token * len(prompt_tokens))(*prompt_tokens)
        batch = lib.llama_batch_get_one(prompt_arr, len(prompt_tokens))

        rc = lib.llama_decode(self.ctx, batch)
        if rc != 0:
            raise RuntimeError(f"[LIVA Native] llama_decode failed during prompt ingestion (rc={rc})")

        # 2. Autoregressive generation loop
        for _ in range(max_tokens):
            new_token = lib.llama_sampler_sample(self.sampler, self.ctx, -1)

            if new_token == self.eos_token:
                break

            text = self.detokenize(new_token)
            yield text

            single = (llama_token * 1)(new_token)
            batch = lib.llama_batch_get_one(single, 1)

            rc = lib.llama_decode(self.ctx, batch)
            if rc != 0:
                print(f"[LIVA Native] WARNING: llama_decode error (rc={rc}), stopping")
                break

    def generate(self, prompt_tokens: List[int], max_tokens: int = 512) -> str:
        """Non-streaming generation."""
        return "".join(self.generate_stream(prompt_tokens, max_tokens))

    def shutdown(self):
        """RAII cleanup -- free all C++ heap allocations."""
        if not self._alive:
            return
        print("[LIVA Native] Shutting down engine...")
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
        print("[LIVA Native] Engine shutdown complete.")

    def __del__(self):
        self.shutdown()


# ==============================================================================
# Phase 5: JSONL-over-TCP IPC Server (Zero-HTTP Communication with Gateway)
# ==============================================================================

IPC_PORT = 8100

async def handle_ipc_client(reader: asyncio.StreamReader, writer: asyncio.StreamWriter, engine: LivaNativeEngine):
    """Handle a single IPC client connection (Gateway)."""
    peer = writer.get_extra_info("peername")
    print(f"[IPC] Gateway connected from {peer}")

    try:
        while True:
            line = await reader.readline()
            if not line:
                break

            try:
                request = json.loads(line.decode("utf-8").strip())
            except json.JSONDecodeError as e:
                error_resp = json.dumps({"error": f"Invalid JSON: {e}"}) + "\n"
                writer.write(error_resp.encode("utf-8"))
                await writer.drain()
                continue

            method = request.get("method", "")
            req_id = request.get("id", None)
            params = request.get("params", {})

            if method == "generate":
                messages = params.get("messages", [])
                max_tokens = params.get("max_tokens", 512)
                stream = params.get("stream", True)

                # Build prompt from messages using standard Gemma/ChatML format
                prompt_text = ""
                for msg in messages:
                    role = msg.get("role", "user")
                    if role == "assistant":
                        role = "model"
                    # Gemma 4 hỗ trợ system turn natively, giữ nguyên để phân biệt rõ system vs user
                    content = msg.get("content", "")
                    prompt_text += f"<start_of_turn>{role}\n{content}<end_of_turn>\n"
                prompt_text += "<start_of_turn>model\n"

                tokens = engine.tokenize(prompt_text)

                if stream:
                    full_text = ""
                    for chunk in engine.generate_stream(tokens, max_tokens):
                        full_text += chunk
                        # Tấm khiên thép: Chặn AI tự biên tự diễn (Hallucinate) người dùng!
                        if "<start_of_turn>" in full_text or "<|user|>" in full_text or "<|im_start|>" in full_text:
                            print("[IPC] Stop sequence detected, halting generation to prevent hallucination.")
                            break
                        
                        response = json.dumps({
                            "id": req_id, "type": "token", "content": chunk,
                        }) + "\n"
                        writer.write(response.encode("utf-8"))
                        await writer.drain()

                    done = json.dumps({
                        "id": req_id, "type": "done", "content": full_text,
                    }) + "\n"
                    writer.write(done.encode("utf-8"))
                    await writer.drain()
                else:
                    result = engine.generate(tokens, max_tokens)
                    response = json.dumps({
                        "id": req_id, "type": "done", "content": result,
                    }) + "\n"
                    writer.write(response.encode("utf-8"))
                    await writer.drain()

            elif method == "tokenize":
                text = params.get("text", "")
                tokens = engine.tokenize(text)
                response = json.dumps({
                    "id": req_id, "type": "result",
                    "tokens": tokens, "count": len(tokens),
                }) + "\n"
                writer.write(response.encode("utf-8"))
                await writer.drain()

            elif method == "health":
                response = json.dumps({
                    "id": req_id, "type": "result",
                    "status": "ok", "engine": "liva-native-cffi",
                    "eos_token": engine.eos_token,
                }) + "\n"
                writer.write(response.encode("utf-8"))
                await writer.drain()

            elif method == "shutdown":
                resp = json.dumps({"id": req_id, "type": "result", "status": "shutting_down"}) + "\n"
                writer.write(resp.encode("utf-8"))
                await writer.drain()
                break

            else:
                error_resp = json.dumps({"id": req_id, "error": f"Unknown method: {method}"}) + "\n"
                writer.write(error_resp.encode("utf-8"))
                await writer.drain()

    except (ConnectionResetError, BrokenPipeError):
        print("[IPC] Gateway disconnected.")
    finally:
        writer.close()


async def start_ipc_server(engine: LivaNativeEngine):
    """Start the JSONL-over-TCP IPC server."""
    server = await asyncio.start_server(
        lambda r, w: handle_ipc_client(r, w, engine),
        "127.0.0.1", IPC_PORT,
    )
    print(f"[IPC] JSONL-over-TCP server listening on 127.0.0.1:{IPC_PORT}")
    print(f"[IPC] Protocol: Raw JSONL (zero HTTP overhead)")

    async with server:
        await server.serve_forever()


# ==============================================================================
# Phase 6: Main Entry Point
# ==============================================================================

def main():
    from dotenv import load_dotenv

    base_dir = os.path.dirname(os.path.abspath(__file__))
    env_path = os.path.join(os.path.dirname(base_dir), "openclaw-gateway", ".env")
    load_dotenv(env_path, override=True)

    if os.getenv("AI_PROVIDER") == "openai":
        print("=" * 50)
        print("[LIVA Native] Cloud API mode -- local engine not needed.")
        print("=" * 50)
        sys.exit(0)

    models_dir = os.getenv("AI_MODELS_DIR", r"E:\AI_Models")
    model_name = os.getenv("ROUTER_MODEL_NAME", "gemma-4-E4B-it-Q4_K_M.gguf")
    model_path = os.path.join(models_dir, model_name)

    if not os.path.exists(model_path):
        print(f"[LIVA Native] FATAL: Model not found: {model_path}")
        sys.exit(1)

    n_ctx = int(os.getenv("NATIVE_N_CTX", "8192"))
    n_gpu = int(os.getenv("NATIVE_N_GPU_LAYERS", "-1"))
    temp = float(os.getenv("NATIVE_TEMPERATURE", "0.7"))

    print("=" * 60)
    print("[LIVA] Zero-Overhead Native Inference Engine")
    print(f"  DLL: {DLL_PATH}")
    print(f"  Model: {model_path}")
    print(f"  Config: n_ctx={n_ctx}, n_gpu={n_gpu}, temp={temp}")
    print("=" * 60)

    engine = LivaNativeEngine(
        model_path=model_path,
        n_ctx=n_ctx,
        n_gpu_layers=n_gpu,
        temperature=temp,
    )

    def signal_handler(sig, frame):
        print("\n[LIVA Native] Received shutdown signal...")
        engine.shutdown()
        sys.exit(0)

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    try:
        asyncio.run(start_ipc_server(engine))
    except KeyboardInterrupt:
        pass
    finally:
        engine.shutdown()


if __name__ == "__main__":
    main()
