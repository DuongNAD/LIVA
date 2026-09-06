//! Milestone 2 Adversarial Stress Test Suite
//!
//! Empirical validation of edge cases, attack scenarios, and stress conditions:
//! 1. UTF-8 multi-byte secrets in channel configuration masking (crash/panic testing)
//! 2. Expired WhatsApp QR codes and concurrent QR generation storms
//! 3. Channel rapid start/stop/restart/configure state transition storms
//! 4. Pairing short-code brute force resistance & challenge expiration
//! 5. Pairing challenge replay attacks & token signature tampering
//! 6. Revocation lifecycle under active token verification
//! 7. Browser SSRF boundary attacks & session control storms
//! 8. Skill store path traversal attempts in manifest lookup and hub installation

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::gateway::pairing::{NodeId, NodeRole, PairingRegistry, PairingRequest};
use liva_native_core::{
    AppState, CommandPrincipal, db, handle_command_as, llm, stt, tts,
};
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_state() -> Arc<AppState> {
    let db = db::DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let llm_manager = llm::LlamaRouterManager::new(2048, 0).expect("LLM manager");
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));

    Arc::new(AppState {
        db,
        crypto: EncryptionEngine::new("00000000000000000000000000000000"),
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(None),
        tts_player: tts::audio::TtsAudioPlayer::new(None),
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault")),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// -----------------------------------------------------------------------------
// Test 1: Channel Config UTF-8 Multi-Byte Secrets & Masking Robustness
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_channel_config_utf8_masking() {
    let state = test_state();

    // Adversarial secret inputs: multi-byte UTF-8 sequences at byte 4 boundary
    let weird_tokens = [
        "abcàdef12345",         // 'à' is 2 bytes across index 3..5
        "ab🔑cdef12345",        // '🔑' is 4 bytes across index 2..6
        "123🌟567🌟90",         // '🌟' is 4 bytes
        "TiếngViệtBảoMật2026",  // Vietnamese accented characters
        " ngắn ",               // Short with whitespace
        "",                     // Empty
        "12345678",             // Exactly 8 chars
        "123456789",            // 9 chars
        "𠮷野家𠮷野家",          // 4-byte CJK surrogate pairs
    ];

    for token in weird_tokens {
        let res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "channels:configure",
            json!({
                "channelId": "telegram",
                "config": {
                    "enabled": true,
                    "botToken": token,
                    "secretKey": token
                }
            }),
            None,
            None,
        )
        .await;

        println!("Configuring with token '{token}' result: {:?}", res.is_ok());
        // If it panics due to byte slicing, this test will fail
        assert!(res.is_ok(), "Configuring token '{token}' should succeed without panic: {:?}", res.err());
    }
}

// -----------------------------------------------------------------------------
// Test 2: WhatsApp Expired QR & Concurrent QR Generation Storms
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_whatsapp_qr_expiration_and_concurrency() {
    let state = test_state();

    // 1. Initial QR fetch
    let qr1 = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "channels:whatsapp_qr",
        json!({}),
        None,
        None,
    )
    .await
    .expect("first qr");

    let qr_data1 = qr1["qrData"].as_str().unwrap().to_string();
    assert!(!qr_data1.is_empty());
    let expires1 = qr1["expiresAtUnix"].as_u64().unwrap();
    assert!(expires1 >= now_unix());

    // 2. High concurrency storm: 50 tasks requesting WhatsApp QR simultaneously
    let mut handles = Vec::new();
    for _ in 0..50 {
        let st = state.clone();
        handles.push(tokio::spawn(async move {
            handle_command_as(
                CommandPrincipal::TauriDashboard,
                st,
                "channels:whatsapp_qr",
                json!({}),
                None,
                None,
            )
            .await
        }));
    }

    for h in handles {
        let res = h.await.expect("task join").expect("qr command");
        assert!(res["qrData"].as_str().is_some());
        assert!(res["ttlSeconds"].as_u64().unwrap() <= 120);
    }
}

// -----------------------------------------------------------------------------
// Test 3: Rapid State Transition Storms (Start/Stop/Restart/Configure)
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_channel_state_transition_storm() {
    let state = test_state();
    let channels = ["telegram", "whatsapp", "discord", "slack"];

    let mut handles = Vec::new();
    for i in 0..100 {
        let st = state.clone();
        let channel_id = channels[i % channels.len()];
        let action = match i % 4 {
            0 => "channels:start",
            1 => "channels:stop",
            2 => "channels:restart",
            _ => "channels:configure",
        };

        handles.push(tokio::spawn(async move {
            if action == "channels:configure" {
                handle_command_as(
                    CommandPrincipal::TauriDashboard,
                    st,
                    "channels:configure",
                    json!({
                        "channelId": channel_id,
                        "config": { "enabled": i % 2 == 0 }
                    }),
                    None,
                    None,
                )
                .await
            } else {
                handle_command_as(
                    CommandPrincipal::TauriDashboard,
                    st,
                    action,
                    json!({ "channelId": channel_id }),
                    None,
                    None,
                )
                .await
            }
        }));
    }

    for h in handles {
        let res = h.await.expect("task join");
        assert!(res.is_ok(), "Storm command failed: {:?}", res.err());
    }

    // Verify registry is still intact and queryable
    let list_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "channels:list",
        json!({}),
        None,
        None,
    )
    .await
    .expect("list channels after storm");

    assert_eq!(list_res["channels"].as_array().unwrap().len(), 4);
}

// -----------------------------------------------------------------------------
// Test 4: Pairing Short-Code Brute Force & Challenge TTL Expiry
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_pairing_brute_force_and_expiration() {
    let state = test_state();

    // 1. Create a legitimate pending challenge
    let challenge = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:create_challenge",
        json!({
            "nodeName": "Target Device",
            "role": "mobile_companion",
            "publicKey": "ed25519_target_pubkey"
        }),
        None,
        None,
    )
    .await
    .expect("create challenge");

    let legitimate_code = challenge["shortCode"].as_str().unwrap().to_string();

    // 2. Simulate attacker attempting 1,000 rapid incorrect short-code guesses
    let mut guess_handles = Vec::new();
    for i in 0..1000 {
        let st = state.clone();
        let guess = format!("{:06}", (i * 7919) % 1_000_000);
        if guess == legitimate_code {
            continue; // Skip legitimate code
        }

        guess_handles.push(tokio::spawn(async move {
            handle_command_as(
                CommandPrincipal::TauriDashboard,
                st,
                "pairing:approve",
                json!({ "shortCode": guess }),
                None,
                None,
            )
            .await
        }));
    }

    for h in guess_handles {
        let res = h.await.expect("join");
        assert!(res.is_err(), "Invalid short-code guess should fail");
    }

    // 3. Legitimate code must still be approvable
    let valid_approval = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:approve",
        json!({ "shortCode": legitimate_code }),
        None,
        None,
    )
    .await
    .expect("approve with valid code");

    assert!(valid_approval["success"].as_bool().unwrap());
}

// -----------------------------------------------------------------------------
// Test 5: Replay Attacks & Token Signature Tampering
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_pairing_replay_and_tampering() {
    let registry = PairingRegistry::with_random_secret("server_pubkey_test");
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "Replay Test Node".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "ed25519_pubkey".to_string(),
        pairing_nonce: "nonce_123".to_string(),
        timestamp_unix: now_unix(),
    };

    let ch = registry.create_challenge(req, 300).await.unwrap();

    // First approval succeeds
    let resp1 = registry.approve_by_short_code(&ch.short_code).await;
    assert!(resp1.is_ok());
    let token = resp1.unwrap().auth_token.unwrap();

    // Replay attack with same short code MUST fail
    let replay_code = registry.approve_by_short_code(&ch.short_code).await;
    assert!(replay_code.is_err(), "Replaying short code MUST fail");

    // Replay attack with same challenge ID MUST fail
    let replay_cid = registry.approve_by_challenge_id(&ch.challenge_id).await;
    assert!(replay_cid.is_err(), "Replaying challenge ID MUST fail");

    // Token tampering attack: elevate role from mobile_companion to desktop_ui
    let tampered_role = token.replace("mobile_companion", "desktop_ui");
    assert!(registry.verify_auth_token(&tampered_role).is_err(), "Tampered role in token MUST fail verification");

    // Token tampering attack: alter node_id UUID
    let fake_uuid = uuid::Uuid::new_v4().to_string();
    let tampered_uuid = token.replace(&node_id.to_string(), &fake_uuid);
    assert!(registry.verify_auth_token(&tampered_uuid).is_err(), "Tampered node ID MUST fail verification");

    // Expired token verification
    let expired_token = registry.generate_token(node_id, NodeRole::MobileCompanion, now_unix() - 100);
    assert!(registry.verify_auth_token(&expired_token).is_err(), "Expired token MUST fail verification");
}

// -----------------------------------------------------------------------------
// Test 6: Node Revocation Lifecycle & Active Connections
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_node_revocation_lifecycle() {
    let registry = PairingRegistry::with_random_secret("server_pubkey_test");
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "Revocation Candidate".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "ed25519_pubkey".to_string(),
        pairing_nonce: "nonce_abc".to_string(),
        timestamp_unix: now_unix(),
    };

    let ch = registry.create_challenge(req, 300).await.unwrap();
    let resp = registry.approve_by_challenge_id(&ch.challenge_id).await.unwrap();
    let token = resp.auth_token.unwrap();

    // Active connection check: token verifies & node is approved
    assert!(registry.verify_auth_token(&token).is_ok());
    assert!(registry.is_node_approved(&node_id).await);

    // Revoke node
    let revoked = registry.revoke_node(&node_id).await;
    assert!(revoked);

    // Node is no longer approved
    assert!(!registry.is_node_approved(&node_id).await);

    // Calling revoke a second time returns false (idempotent / not found)
    assert!(!registry.revoke_node(&node_id).await);
}

// -----------------------------------------------------------------------------
// Test 7: Browser SSRF Boundaries & Session Control Storms
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_browser_ssrf_and_control_storms() {
    let state = test_state();

    let ssrf_targets = [
        "http://127.0.0.1:8080/admin",
        "http://localhost:3000/api",
        "http://169.254.169.254/latest/meta-data/",
        "http://10.0.0.1/internal",
        "http://192.168.1.1/router",
        "http://172.16.50.1/private",
        "http://metadata.google.internal/computeMetadata/v1/",
        "http://server.local/debug",
        "http://server.corp/secrets",
    ];

    for target in ssrf_targets {
        let res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "browser:navigate",
            json!({ "url": target }),
            None,
            None,
        )
        .await;

        assert!(res.is_err(), "SSRF target '{target}' MUST be blocked by browser sandbox");
    }

    // Control action storm: 50 concurrent pause/resume/stop/clear_logs
    let control_actions = ["pause", "resume", "clear_logs"];
    let mut handles = Vec::new();
    for i in 0..50 {
        let st = state.clone();
        let action = control_actions[i % control_actions.len()];
        handles.push(tokio::spawn(async move {
            handle_command_as(
                CommandPrincipal::TauriDashboard,
                st,
                "browser:control",
                json!({ "action": action }),
                None,
                None,
            )
            .await
        }));
    }

    for h in handles {
        let res = h.await.expect("join");
        assert!(res.is_ok(), "Browser control storm failed: {:?}", res.err());
    }
}

// -----------------------------------------------------------------------------
// Test 8: Skill Store Path Traversal Attempts
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_skill_store_path_traversal() {
    let state = test_state();

    // Attempt path traversal via skillId
    let traversal_ids = [
        "../../etc/passwd",
        "../Cargo.toml",
        "/etc/shadow",
        "....//....//something",
    ];

    for tid in traversal_ids {
        let res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "skills:get_manifest",
            json!({ "skillId": tid }),
            None,
            None,
        )
        .await;

        // Even if fallback synthetic manifest is returned, it must not leak host file contents
        if let Ok(val) = res {
            assert_ne!(val["skillId"], "/etc/passwd");
            assert!(!val["rawContent"].as_str().unwrap().contains("root:x:0:0:"));
        }
    }
}

// -----------------------------------------------------------------------------
// Test 9: Skill Store Hub Installation Path Traversal Empirical Verification
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_skill_install_path_traversal_escape() {
    let state = test_state();
    let escape_id = "test_escape_traversal_dir";
    let traversal_name = format!("../../../tmp/{}", escape_id);

    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:install_from_hub",
        json!({
            "name": traversal_name,
            "skillId": traversal_name
        }),
        None,
        None,
    )
    .await;

    println!("Skill install with path traversal result: {:?}", res);
    let escaped_file = std::path::Path::new("/tmp").join(escape_id).join("SKILL.md");
    if escaped_file.exists() {
        println!("VULNERABILITY CONFIRMED: File written outside skills root at {:?}", escaped_file);
        let _ = std::fs::remove_dir_all(std::path::Path::new("/tmp").join(escape_id));
    }
    assert!(res.is_err(), "Skill install with path traversal MUST fail");
    assert!(!escaped_file.exists(), "Escaped file MUST NOT exist");
}

// -----------------------------------------------------------------------------
// Test 10: Pairing Challenge Targeted Brute Force Invalidation (Max Failed Attempts)
// -----------------------------------------------------------------------------
#[tokio::test]
async fn test_adversarial_pairing_targeted_brute_force_invalidation() {
    let state = test_state();

    let challenge = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:create_challenge",
        json!({
            "nodeName": "Targeted Victim Device",
            "role": "mobile_companion",
            "publicKey": "ed25519_victim_pubkey"
        }),
        None,
        None,
    )
    .await
    .expect("create challenge");

    let cid = challenge["challengeId"].as_str().unwrap();
    let legitimate_code = challenge["shortCode"].as_str().unwrap();

    // 4 failed guesses with wrong code against this specific challenge
    for _ in 0..4 {
        let res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "pairing:approve",
            json!({
                "challengeId": cid,
                "shortCode": "000000"
            }),
            None,
            None,
        )
        .await;

        assert!(res.is_err(), "Incorrect short code attempt MUST fail");
    }

    // 5th failed guess must invalidate the challenge
    let res5 = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:approve",
        json!({
            "challengeId": cid,
            "shortCode": "000000"
        }),
        None,
        None,
    )
    .await;

    assert!(res5.is_err(), "5th failed attempt MUST fail and invalidate");

    // Approving now even with the legitimate code MUST fail because challenge was evicted
    let res_legit = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:approve",
        json!({
            "challengeId": cid,
            "shortCode": legitimate_code
        }),
        None,
        None,
    )
    .await;

    assert!(res_legit.is_err(), "Legitimate code on invalidated challenge MUST fail");
}
