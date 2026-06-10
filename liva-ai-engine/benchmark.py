#!/usr/bin/env python3
import sys
import time
import logging
import resource

# Set up logging to stderr
logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(name)s: %(message)s")
logger = logging.getLogger("benchmark")

import os
try:
    from dotenv import load_dotenv
    base_dir = os.path.dirname(os.path.abspath(__file__))
    env_path = os.path.join(os.path.dirname(base_dir), "liva-gateway", ".env")
    load_dotenv(env_path, override=True)
except ImportError:
    pass

try:
    from liva_native_engine import (
        LivaNativeEngine,
        lib,
        llama_token
    )
except ImportError as e:
    logger.error(f"Failed to import liva_native_engine: {e}")
    sys.exit(1)

def run_benchmark():
    model_path = sys.argv[1] if len(sys.argv) > 1 else "/Users/duongnad/AI_Models/gemma-4-12B-it-Q6_K.gguf"
    prompt = "<start_of_turn>user\nExplain the theory of relativity in one simple paragraph.<end_of_turn>\n<start_of_turn>model\n"
    max_tokens = 128
    
    n_ctx = int(os.getenv("NATIVE_N_CTX", "2048"))
    n_batch = int(os.getenv("NATIVE_N_BATCH", "2048"))
    n_threads = int(os.getenv("NATIVE_N_THREADS", "1"))
    n_threads_batch = int(os.getenv("NATIVE_N_THREADS_BATCH", "4"))
    n_ubatch = int(os.getenv("NATIVE_N_UBATCH", "256"))
    n_gpu = int(os.getenv("NATIVE_N_GPU_LAYERS", "-1"))
    
    logger.info(f"Initializing LivaNativeEngine with parameters:")
    logger.info(f"  n_ctx={n_ctx}, n_gpu={n_gpu}, n_batch={n_batch}, n_threads={n_threads}, n_threads_batch={n_threads_batch}, n_ubatch={n_ubatch}")
    
    engine = LivaNativeEngine(
        model_path=model_path,
        n_ctx=n_ctx,
        n_gpu_layers=n_gpu,
        n_batch=n_batch,
        n_threads=n_threads,
        n_threads_batch=n_threads_batch,
        n_ubatch=n_ubatch,
        flash_attn=True
    )
    
    logger.info("Tokenizing prompt...")
    prompt_tokens = engine.tokenize(prompt, add_special=True)
    total_prefill = len(prompt_tokens)
    logger.info(f"Prompt length: {total_prefill} tokens")
    # Ensure KV Cache is clear
    logger.info("Skipping redundant KV Cache clear")
        
    logger.info("Before prefill_arr")
    prefill_arr = (llama_token * total_prefill)(*prompt_tokens)
    logger.info("After prefill_arr")
    
    logger.info("Before batch init")
    batch = lib.llama_batch_init(engine.n_batch, 0, 1)
    logger.info("After batch init")
    
    n_past = 0
    idx = 0
    last_batch_n_tokens = 0
    
    logger.info("Executing prefill phase...")
    start_prefill = time.perf_counter()
    while idx < total_prefill:
        chunk_size = min(engine.n_batch, total_prefill - idx)
        last_batch_n_tokens = chunk_size
        
        batch.n_tokens = chunk_size
        for i in range(chunk_size):
            batch.token[i] = prefill_arr[idx + i]
            batch.pos[i] = n_past + i
            batch.n_seq_id[i] = 1
            batch.seq_id[i][0] = 0
            batch.logits[i] = 0
            
        if idx + chunk_size == total_prefill:
            batch.logits[chunk_size - 1] = 1
            
        rc = lib.llama_decode(engine.ctx, batch)
        if rc != 0:
            raise RuntimeError(f"llama_decode prefill failed with rc={rc}")
            
        n_past += chunk_size
        idx += chunk_size
        
    end_prefill = time.perf_counter()
    prefill_time = end_prefill - start_prefill
    prefill_speed = total_prefill / prefill_time if prefill_time > 0 else 0
    
    logger.info(f"Prefill complete: {total_prefill} tokens in {prefill_time:.4f}s ({prefill_speed:.2f} tokens/sec)")
    
    # Reset sampler
    if engine.has_sampler_reset:
        lib.llama_sampler_reset(engine.sampler)
        
    logger.info("Executing decode phase...")
    decode_tokens = []
    generated_text = []
    
    sampler_idx = max(0, last_batch_n_tokens - 1)
    
    start_decode = time.perf_counter()
    for step in range(max_tokens):
        new_token = lib.llama_sampler_sample(engine.sampler, engine.ctx, sampler_idx)
        
        if new_token == engine.eos_token:
            logger.info("EOS token detected, stopping generation.")
            break
            
        text = engine.detokenize(new_token)
        decode_tokens.append(new_token)
        generated_text.append(text)
        print(text, end="", flush=True)
        
        if engine.has_sampler_accept:
            lib.llama_sampler_accept(engine.sampler, new_token)
            
        # Decode the generated token
        batch.n_tokens = 1
        batch.token[0] = new_token
        batch.pos[0] = n_past
        batch.n_seq_id[0] = 1
        batch.seq_id[0][0] = 0
        batch.logits[0] = 1
        
        rc = lib.llama_decode(engine.ctx, batch)
        if rc != 0:
            logger.error(f"llama_decode failed during step {step} (rc={rc})")
            break
            
        n_past += 1
        sampler_idx = 0
        
    end_decode = time.perf_counter()
    print() # Newline after generation
    
    decode_time = end_decode - start_decode
    decode_count = len(decode_tokens)
    decode_speed = decode_count / decode_time if decode_time > 0 else 0
    
    logger.info("Benchmarking completed.")
    
    # Get peak RSS (macOS returns bytes, convert to MB)
    peak_rss_bytes = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    peak_rss_mb = peak_rss_bytes / (1024 * 1024)
    
    print("\n" + "="*40)
    print("BENCHMARK RESULTS:")
    print(f"Model: {model_path}")
    print(f"Prefill: {total_prefill} tokens in {prefill_time:.4f}s ({prefill_speed:.2f} t/s)")
    print(f"Decode:  {decode_count} tokens in {decode_time:.4f}s ({decode_speed:.2f} t/s)")
    print(f"Peak RSS: {peak_rss_bytes} bytes ({peak_rss_mb:.2f} MB)")
    print("="*40)
    
    # Clean up
    lib.llama_batch_free(batch)
    engine.shutdown()

if __name__ == "__main__":
    run_benchmark()
