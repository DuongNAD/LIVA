//! Milestone 2 Integration Tests: llama-server Sidecar Supervisor & VRAM Governor (Features F09 & F10).

use liva_native_core::llm::sidecar::{
    FallbackAction, KvPrecision, LlamaSidecarClient, LlamaSidecarSupervisor, SidecarConfig, SlotId,
    VramPressureTier, VramSupervisor, allocate_ephemeral_port, generate_csprng_key,
};
use serde_json::json;

#[test]
fn test_f09_ephemeral_port_allocation() {
    let port1 = allocate_ephemeral_port().expect("allocate ephemeral port 1");
    let port2 = allocate_ephemeral_port().expect("allocate ephemeral port 2");
    assert!(port1 > 1024, "Ephemeral port must be > 1024, got {port1}");
    assert!(port2 > 1024, "Ephemeral port must be > 1024, got {port2}");
}

#[test]
fn test_f09_csprng_key_generation() {
    let key1 = generate_csprng_key();
    let key2 = generate_csprng_key();
    assert_eq!(key1.len(), 64, "Key must be 64 hex characters (256-bit)");
    assert_eq!(key2.len(), 64, "Key must be 64 hex characters (256-bit)");
    assert_ne!(key1, key2, "Keys must be distinct CSPRNG outputs");
    assert!(key1.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_f09_cli_args_security_confinement() {
    let config = SidecarConfig::default();
    let port = 19482;
    let api_key = "test_secret_key_1234567890abcdef1234567890abcdef";
    let args = config.build_cli_args(port, api_key);

    // 1. Mandatory flags checks
    let args_str = args.join(" ");
    assert!(
        args_str.contains("--host 127.0.0.1"),
        "Must bind only to 127.0.0.1"
    );
    assert!(
        args_str.contains("--port 19482"),
        "Must set configured ephemeral port"
    );
    assert!(
        args_str.contains(&format!("--api-key {api_key}")),
        "Must set ephemeral api key"
    );
    assert!(args_str.contains("-np 3"), "Must allocate 3 parallel slots");
    assert!(
        args_str.contains("-c 8192"),
        "Must configure context size per slot"
    );
    assert!(
        args_str.contains("--cache-type-k q8_0"),
        "Must set Q8_0 K cache"
    );
    assert!(
        args_str.contains("--cache-type-v q8_0"),
        "Must set Q8_0 V cache"
    );
    assert!(args_str.contains("-fa"), "Must enable FlashAttention");
    assert!(args_str.contains("--no-webui"), "Must disable web UI");

    // 2. Strict Security Confinement: ZERO tool/agent execution flags!
    assert!(
        !args.iter().any(|a| a == "--tools"),
        "Must NEVER pass --tools flag"
    );
    assert!(
        !args.iter().any(|a| a == "-ag" || a == "--agent"),
        "Must NEVER pass --agent flag"
    );
    assert!(
        !args.iter().any(|a| a.starts_with("--mcp-servers")),
        "Must NEVER pass --mcp-servers-* flags"
    );
}

#[test]
fn test_f10_kv_precision_invariant() {
    // Valid FlashAttention pairs
    assert!(KvPrecision::validate(KvPrecision::Q8_0, KvPrecision::Q8_0).is_ok());
    assert!(KvPrecision::validate(KvPrecision::F16, KvPrecision::F16).is_ok());
    assert!(KvPrecision::validate(KvPrecision::F32, KvPrecision::F32).is_ok());

    // Asymmetric pairs MUST be rejected to preserve FlashAttention kernels
    let err = KvPrecision::validate(KvPrecision::F16, KvPrecision::Q8_0);
    assert!(err.is_err());
    let err_msg = err.unwrap_err().to_string();
    assert!(err_msg.contains("must match V precision"));
}

#[test]
fn test_f10_vram_pressure_fallback_cascade() {
    let total_vram = 16 * 1024 * 1024 * 1024; // 16 GB RTX 5060 Ti
    let supervisor = VramSupervisor::with_total_vram(total_vram);

    // Scenario 1: Nominal load (8 GB used = 50%)
    let assess_nominal = supervisor.evaluate_pressure_custom(total_vram, 8 * 1024 * 1024 * 1024);
    assert_eq!(assess_nominal.tier, VramPressureTier::Nominal);
    assert_eq!(assess_nominal.action_taken, FallbackAction::None);
    assert!(!supervisor.is_reranker_on_cpu());
    assert!(!supervisor.is_worker_sleep_idle());

    // Scenario 2: Elevated load (13.2 GB used = 82.5%)
    let assess_elevated =
        supervisor.evaluate_pressure_custom(total_vram, (13.2 * 1024.0 * 1024.0 * 1024.0) as u64);
    assert_eq!(assess_elevated.tier, VramPressureTier::Elevated);
    assert_eq!(assess_elevated.action_taken, FallbackAction::None);
    assert!(!supervisor.is_reranker_on_cpu());

    // Scenario 3: High pressure (14.0 GB used = 87.5% -> free 2.0 GB)
    let assess_high = supervisor.evaluate_pressure_custom(total_vram, 14 * 1024 * 1024 * 1024);
    assert_eq!(assess_high.tier, VramPressureTier::High);
    assert_eq!(
        assess_high.action_taken,
        FallbackAction::OffloadRerankerToCpu
    );
    assert!(
        supervisor.is_reranker_on_cpu(),
        "Reranker must be offloaded to CPU in High tier"
    );
    assert!(!supervisor.is_worker_sleep_idle());

    // Scenario 4: Critical pressure (15.2 GB used = 95% -> free < 1200 MB)
    let assess_critical =
        supervisor.evaluate_pressure_custom(total_vram, (15.2 * 1024.0 * 1024.0 * 1024.0) as u64);
    assert_eq!(assess_critical.tier, VramPressureTier::Critical);
    assert_eq!(
        assess_critical.action_taken,
        FallbackAction::EvictWorkerAndOffloadReranker
    );
    assert!(supervisor.is_reranker_on_cpu());
    assert!(
        supervisor.is_worker_sleep_idle(),
        "Worker 12B must be sleep-idled in Critical tier"
    );

    // Scenario 5: Recovery back to Nominal (8 GB used, abundant free > 4 GB)
    let assess_recovery = supervisor.evaluate_pressure_custom(total_vram, 8 * 1024 * 1024 * 1024);
    assert_eq!(assess_recovery.tier, VramPressureTier::Nominal);
    assert_eq!(assess_recovery.action_taken, FallbackAction::None);
    assert!(
        !supervisor.is_reranker_on_cpu(),
        "Reranker offload flag resets upon recovery"
    );
    assert!(
        !supervisor.is_worker_sleep_idle(),
        "Worker sleep-idle flag resets upon recovery"
    );
}

#[tokio::test]
async fn test_f09_slot_management_and_kv_save_restore_mock() {
    let supervisor = LlamaSidecarSupervisor::new_mock(SidecarConfig::default());
    let client = &supervisor.client;

    // 1. Health check
    let health = client.health().await;
    assert!(health.is_ok());
    assert!(health.unwrap());

    // 2. Chat completion on pinned slot 0 (foreground)
    let req = json!({
        "messages": [{"role": "user", "content": "Xin chào"}]
    });
    let resp = client.chat_completion(SlotId::Foreground, &req).await;
    assert!(resp.is_ok());
    let val = resp.unwrap();
    assert_eq!(val["slot_id"], 0);

    // 3. Save slot 0 KV cache
    let save_res = client
        .save_slot_kv(SlotId::Foreground, "data/kv_cache/slot0_prefix.bin")
        .await;
    assert!(save_res.is_ok());
    let save_action = save_res.unwrap();
    assert!(save_action.success);
    assert_eq!(
        save_action.filepath.as_deref(),
        Some("data/kv_cache/slot0_prefix.bin")
    );

    // 4. Restore slot 0 KV cache
    let restore_res = client
        .restore_slot_kv(SlotId::Foreground, "data/kv_cache/slot0_prefix.bin")
        .await;
    assert!(restore_res.is_ok());
    let restore_action = restore_res.unwrap();
    assert!(restore_action.success);
    assert_eq!(
        restore_action.filepath.as_deref(),
        Some("data/kv_cache/slot0_prefix.bin")
    );
}
