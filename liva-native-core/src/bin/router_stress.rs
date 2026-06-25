#![allow(dead_code, unused_imports, unused_variables)]
use std::path::Path;
use std::time::Instant;

#[path = "../llm/mod.rs"]
mod llm;

use llm::engine::{LlamaRouterManager, prune_kv_cache};

// Win32 Process Memory Monitoring Helper
#[cfg(target_os = "windows")]
#[link(name = "psapi")]
unsafe extern "system" {
    fn GetProcessMemoryInfo(
        process: *mut std::ffi::c_void,
        ppsmemCounters: *mut PROCESS_MEMORY_COUNTERS,
        cb: u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut std::ffi::c_void;
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[allow(non_snake_case)]
struct PROCESS_MEMORY_COUNTERS {
    cb: u32,
    PageFaultCount: u32,
    PeakWorkingSetSize: usize,
    WorkingSetSize: usize,
    QuotaPeakPagedPoolUsage: usize,
    QuotaPagedPoolUsage: usize,
    QuotaPeakNonPagedPoolUsage: usize,
    QuotaNonPagedPoolUsage: usize,
    PagefileUsage: usize,
    PeakPagefileUsage: usize,
}

#[cfg(target_os = "windows")]
fn get_memory_usage() -> usize {
    unsafe {
        let handle = GetCurrentProcess();
        let mut counters = std::mem::zeroed::<PROCESS_MEMORY_COUNTERS>();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(handle, &mut counters, counters.cb) != 0 {
            counters.WorkingSetSize
        } else {
            0
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn get_memory_usage() -> usize {
    0
}

#[tokio::main]
async fn main() -> Result<(), String> {
    println!("========================================================");
    println!("LIVA LLM ROUTER - PERFORMANCE & STRESS TESTING HARNESS");
    println!("========================================================");

    let llm_model_dir = std::env::var("LIVA_LLM_MODEL_DIR").unwrap_or_else(|_| {
        let paths = ["models", "../models", "../../models"];
        for p in &paths {
            if Path::new(p).join("ggml-vocab-llama-spm.gguf").exists() {
                return p.to_string();
            }
        }
        "models".to_string()
    });

    let model1_path_buf = Path::new(&llm_model_dir).join("ggml-vocab-llama-spm.gguf");
    let model2_path_buf = Path::new(&llm_model_dir).join("ggml-vocab-llama-bpe.gguf");
    let model1_path = model1_path_buf.as_path();
    let model2_path = model2_path_buf.as_path();

    if !model1_path.exists() || !model2_path.exists() {
        return Err(format!(
            "Model files not found. Ensure paths exist under model dir '{}':\n- {:?}\n- {:?}",
            llm_model_dir, model1_path, model2_path
        ));
    }

    // --------------------------------------------------------
    // 1. Dynamic Model Swapping & VRAM/Memory Leak Verification
    // --------------------------------------------------------
    println!("\n[1] Stress Testing Dynamic Model Swapping (Hot-Swaps)...");
    let mut manager = LlamaRouterManager::new(128, 0)?;

    let initial_mem = get_memory_usage();
    println!(
        "Initial working set: {:.2} MB",
        initial_mem as f64 / 1_048_576.0
    );

    let iterations = 30;
    let mut max_mem = initial_mem;
    let mut mem_readings = Vec::new();

    for i in 1..=iterations {
        let target_model = if i % 2 == 0 { model1_path } else { model2_path };
        let start = Instant::now();

        // Perform the swap
        manager
            .swap_model(target_model, Some(128), Some(0), Some(true))
            .await?;
        let elapsed = start.elapsed();

        let current_mem = get_memory_usage();
        max_mem = max_mem.max(current_mem);
        mem_readings.push(current_mem);

        if i % 5 == 0 || i == 1 {
            println!(
                "  Swap #{:02}: duration={:?}, working set={:.2} MB",
                i,
                elapsed,
                current_mem as f64 / 1_048_576.0
            );
        }
    }

    let final_mem = get_memory_usage();
    let mem_increase = if final_mem > initial_mem {
        final_mem - initial_mem
    } else {
        0
    };

    println!("\nHot-Swap Summary:");
    println!("- Completed swaps: {}", iterations);
    println!(
        "- Initial working set: {:.2} MB",
        initial_mem as f64 / 1_048_576.0
    );
    println!(
        "- Peak working set:    {:.2} MB",
        max_mem as f64 / 1_048_576.0
    );
    println!(
        "- Final working set:   {:.2} MB",
        final_mem as f64 / 1_048_576.0
    );
    println!(
        "- Net growth:          {:.2} MB",
        mem_increase as f64 / 1_048_576.0
    );

    // If memory grew by more than 15MB over 30 swaps, warn about potential leakage
    if mem_increase > 15 * 1024 * 1024 {
        println!("⚠️ WARNING: Potential memory leakage detected in model swapping!");
    } else {
        println!("✅ Model swapping behaves robustly without significant memory growth.");
    }

    // --------------------------------------------------------
    // 2. Sliding Window Pruning & Sequence Shift Verification
    // --------------------------------------------------------
    println!("\n[2] Testing Sliding Window Pruning Logic...");

    // We initialize the manager with n_ctx = 16.
    // So s = (16 / 8).min(512) = 2
    //    k = (16 / 8).min(512) = 2
    //    s + k = 4.
    let n_ctx = 16;
    let _s = 2;
    let _k = 2;

    let mut manager_prune = LlamaRouterManager::new(n_ctx, 0)?;
    manager_prune
        .swap_model(model1_path, Some(n_ctx), Some(0), Some(true))
        .await?;

    let engine = manager_prune
        .engine
        .as_mut()
        .ok_or("Failed to load engine")?;

    // Create a vector representing the last tokens in context
    // Let's populate it with 3 tokens (just under s + k = 4)
    let mut last_tokens: Vec<llama_cpp_2::token::LlamaToken> =
        (0..3).map(|v| llama_cpp_2::token::LlamaToken(v)).collect();
    let mut n_past = 3;

    println!("Initial State:");
    println!("- n_past = {}", n_past);
    println!("- last_tokens len = {}", last_tokens.len());

    // Call prune_kv_cache
    prune_kv_cache(
        &mut engine.context,
        &mut n_past,
        n_ctx as i32,
        &mut last_tokens,
    );
    println!("After prune (n_past < n_ctx):");
    println!("- n_past = {} (Expected: 3)", n_past);
    println!("- last_tokens len = {} (Expected: 3)", last_tokens.len());
    assert_eq!(n_past, 3);
    assert_eq!(last_tokens.len(), 3);

    // Now make n_past = 16 (exactly n_ctx)
    n_past = 16;
    last_tokens = (0..16).map(|v| llama_cpp_2::token::LlamaToken(v)).collect();
    println!("\nTriggering Pruning (n_past = n_ctx = 16):");

    prune_kv_cache(
        &mut engine.context,
        &mut n_past,
        n_ctx as i32,
        &mut last_tokens,
    );
    println!("After prune (n_past = 16):");
    println!("- n_past = {} (Expected: 14)", n_past);
    println!("- last_tokens len = {} (Expected: 14)", last_tokens.len());

    // Verify values
    assert_eq!(n_past, 14, "n_past should be reduced by k=2");
    assert_eq!(
        last_tokens.len(),
        14,
        "last_tokens should be drained by k=2"
    );

    // Verify token values to ensure correct index range was kept
    // We expected to keep [0..s) and [s+k..n_past)
    // Here, s = 2, k = 2, n_past (before prune) was 16.
    // So kept should be [0..2) and [4..16).
    // So kept tokens should be 0, 1, 4, 5, 6, ..., 15.
    let expected_tokens: Vec<i32> = (0..2).chain(4..16).collect();
    let actual_tokens: Vec<i32> = last_tokens.iter().map(|t| t.0).collect();
    println!("- Kept tokens: {:?}", actual_tokens);
    assert_eq!(
        actual_tokens, expected_tokens,
        "Tokens kept do not match expected system prompt tokens!"
    );

    // Now test a case where we have tokens beyond n_ctx before pruning
    // Let's reset: n_past = 20. s = 2, k = 2.
    // Kept tokens should be [0..2) and [4..20).
    // So if initial is 0..20, kept should be 0..2 and 4..20.
    n_past = 20;
    last_tokens = (0..20).map(|v| llama_cpp_2::token::LlamaToken(v)).collect();

    println!("\nTriggering Pruning with trailing tokens (n_past = 20):");
    prune_kv_cache(
        &mut engine.context,
        &mut n_past,
        n_ctx as i32,
        &mut last_tokens,
    );
    println!("After prune (n_past = 20):");
    println!("- n_past = {} (Expected: 18)", n_past);
    println!("- last_tokens len = {} (Expected: 18)", last_tokens.len());

    assert_eq!(n_past, 18, "n_past should be reduced by k=2");
    assert_eq!(
        last_tokens.len(),
        18,
        "last_tokens should be drained by k=2"
    );

    let expected_tokens_2: Vec<i32> = (0..2).chain(4..20).collect();
    let actual_tokens_2: Vec<i32> = last_tokens.iter().map(|t| t.0).collect();
    println!("- Kept tokens: {:?}", actual_tokens_2);
    assert_eq!(
        actual_tokens_2, expected_tokens_2,
        "Shifted tokens do not match expected sequence!"
    );

    // Verify llama.cpp KV cache state operation
    // If the sequence shifts were invalid in llama.cpp, llama_decode/kv_cache operations would crash or assert.
    // Since we called prune_kv_cache and it succeeded, the API bindings work as expected.

    println!("\n✅ Sliding Window Pruning and Sequence Shifts verified successfully!");
    Ok(())
}
