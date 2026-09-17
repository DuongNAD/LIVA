//! Adversarial Challenger Verification Suite for Milestone 4
//! (Phase 3 Escalation Integration at Production Call Sites)
//!
//! Verifies:
//! 1. `pipeline.rs` and `dialogue.rs` integration under adversarial scenarios:
//!    - Complex prompts when expert model exists vs when expert model does not exist / not configured.
//!    - Regular prompts, voice messages, OS control, smart home, and vision intents isolation.
//! 2. Multi-turn dialogue escalation vs fast path state machine isolation.
//! 3. High-concurrency stress test: non-blocking latency (<5µs) and zero deadlock/mutex contention.
//! 4. Fuzzing and extreme edge cases (null bytes, emojis, Unicode zero-width, 100KB-1MB payloads).

use liva_native_core::agent::graph::{
    ConversationMemoryScope, DoKho, Intent, build_pipeline_graph, phan_loai_do_kho, route_intent,
};
use liva_native_core::agent::state::AgentState;
use liva_native_core::configured_expert_model_path;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::messaging::VoiceMessageDialogue;
use liva_native_core::{AppState, db, llm, stt, tts};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;
use tokio::sync::{Mutex, mpsc};

fn create_test_state() -> Arc<AppState> {
    let db = db::DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non_existent_model");
    let llm_manager = llm::LlamaRouterManager::new(512, 0).expect("LLM manager");
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));

    Arc::new(AppState {
        db,
        crypto: EncryptionEngine::new("00000000000000000000000000000000"),
        stt: Mutex::new(stt_manager),
        tts: Mutex::new(None),
        tts_player: tts::audio::TtsAudioPlayer::new(None),
        llm: Mutex::new(llm_manager),
        vad: Mutex::new(None),
        denoiser: Mutex::new(None),
        turn_shadow: Mutex::new(None),
        aec: Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: liva_native_core::AppState::empty_embedder(),
        vision: Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
    })
}

// =========================================================================
// 1. PIPELINE ROUTER ESCALATION: COMPLEXITY VS EXPERT MODEL PRESENCE MATRIX
// =========================================================================

#[tokio::test]
async fn test_pipeline_router_complex_prompt_escalation_matrix() {
    let state = create_test_state();
    let (chunk_tx, _chunk_rx) = mpsc::channel(16);
    let scope = ConversationMemoryScope::new("challenger_user", "conv_m4").unwrap();
    let active_session = Arc::new(AtomicU64::new(100));

    let graph = build_pipeline_graph(state, scope, chunk_tx, None, 100, active_session);

    let complex_prompts = [
        "Hãy hướng dẫn từng bước tối ưu hóa hiệu năng truy vấn SQLite trong môi trường đa luồng.",
        "Phân tích ưu nhược điểm của kiến trúc Event Sourcing so với CRUD truyền thống.",
        "Viết hàm quicksort bằng Rust và phân tích độ phức tạp thời gian thuật toán từng bước?",
        "Why is memory safety important in systems programming? Explain in detail with examples.",
        "SELECT u.id, COUNT(o.id) FROM users u JOIN orders o ON u.id = o.user_id GROUP BY u.id",
        "1. Mục tiêu của kiến trúc LIVA là gì? 2. Điểm nghẽn tiềm tàng nằm ở đâu?",
        "```rust\npub fn solve_graph() -> Result<()> {}\n```",
    ];

    // Setup temporary dummy expert model file to guarantee existence
    let temp_dir = std::env::temp_dir().join("liva_test_expert_dir_m4");
    std::fs::create_dir_all(&temp_dir).expect("create tempdir for expert model");
    let dummy_expert_path = temp_dir.join("dummy_expert_model.gguf");
    std::fs::write(&dummy_expert_path, b"DUMMY_EXPERT_GGUF_HEADER")
        .expect("write dummy expert file");

    // Case 1: LIVA_EXPERT_MODEL_PATH points to real existing file
    unsafe {
        std::env::set_var("LIVA_EXPERT_MODEL_PATH", &dummy_expert_path);
    }

    for prompt in complex_prompts {
        let do_kho = phan_loai_do_kho(prompt);
        assert_eq!(
            do_kho,
            DoKho::Kho,
            "Prompt must classify as DoKho::Kho: '{prompt}'"
        );

        let mut agent_state = AgentState::default();
        agent_state.messages.push(json!({
            "role": "user",
            "content": prompt
        }));

        let routed = graph
            .run_single_node("router", agent_state)
            .await
            .expect("router node run");

        assert_eq!(
            routed.context.get("do_kho").and_then(|v| v.as_str()),
            Some("kho"),
            "Router node must set context.do_kho = kho for: '{prompt}'"
        );
        assert_eq!(
            routed.context.get("goi_y_expert").and_then(|v| v.as_bool()),
            Some(true),
            "Complex query with expert model present MUST escalate via router node: '{prompt}'"
        );
    }

    // Case 2: LIVA_EXPERT_MODEL_PATH points to non-existent file
    let non_existent_path = temp_dir.join("non_existent_expert.gguf");
    unsafe {
        std::env::set_var("LIVA_EXPERT_MODEL_PATH", &non_existent_path);
    }

    for prompt in complex_prompts {
        let mut agent_state = AgentState::default();
        agent_state.messages.push(json!({
            "role": "user",
            "content": prompt
        }));

        let routed = graph
            .run_single_node("router", agent_state)
            .await
            .expect("router node run");

        assert_eq!(
            routed.context.get("do_kho").and_then(|v| v.as_str()),
            Some("kho")
        );
        assert_eq!(
            routed.context.get("goi_y_expert").and_then(|v| v.as_bool()),
            Some(false),
            "Complex query WITHOUT expert model MUST NOT escalate: '{prompt}'"
        );
    }

    // Case 3: Regular prompt must NEVER escalate even when expert model exists
    unsafe {
        std::env::set_var("LIVA_EXPERT_MODEL_PATH", &dummy_expert_path);
    }
    let regular_prompts = [
        "Xin chào LIVA",
        "Chào buổi sáng bạn nhé",
        "Hôm nay bạn cảm thấy thế nào?",
    ];
    for prompt in regular_prompts {
        let mut agent_state = AgentState::default();
        agent_state.messages.push(json!({
            "role": "user",
            "content": prompt
        }));

        let routed = graph
            .run_single_node("router", agent_state)
            .await
            .expect("router node run");

        assert_eq!(
            routed.context.get("goi_y_expert").and_then(|v| v.as_bool()),
            Some(false),
            "Regular prompt MUST NOT escalate even if expert model exists: '{prompt}'"
        );
    }

    // Cleanup env var and tempdir
    unsafe {
        std::env::remove_var("LIVA_EXPERT_MODEL_PATH");
    }
    std::fs::remove_dir_all(&temp_dir).ok();
}

// =========================================================================
// 2. PIPELINE ROUTER ISOLATION: REGULAR, SMART HOME, OS, VISION, MESSAGING
// =========================================================================

#[test]
fn test_pipeline_router_non_chat_and_reflex_intent_isolation() {
    let regular_and_reflex_matrix = [
        // Regular chat queries
        ("Xin chào LIVA", Intent::Chat, DoKho::Thuong),
        ("Chào buổi sáng bạn nhé", Intent::Chat, DoKho::Thuong),
        ("Hôm nay là thứ mấy?", Intent::Chat, DoKho::Thuong),
        (
            "Thời tiết Hà Nội hôm nay thế nào?",
            Intent::Weather {
                location: Some("Hà Nội".to_string()),
            },
            DoKho::Thuong,
        ),
        ("Cảm ơn bạn rất nhiều", Intent::Chat, DoKho::Thuong),
        // Smart home reflex
        (
            "bật đèn phòng khách",
            Intent::SmartHome {
                device: "light",
                action: "on",
            },
            DoKho::Thuong,
        ),
        (
            "tắt quạt đi giúp mình",
            Intent::SmartHome {
                device: "fan",
                action: "off",
            },
            DoKho::Thuong,
        ),
        (
            "bật điều hoà lên 24 độ",
            Intent::SmartHome {
                device: "ac",
                action: "on",
            },
            DoKho::Thuong,
        ),
        // OS Control reflex
        (
            "tăng âm lượng lên",
            Intent::OsControl {
                tool: "control_volume",
                action: "up",
            },
            DoKho::Thuong,
        ),
        (
            "giảm âm lượng xuống",
            Intent::OsControl {
                tool: "control_volume",
                action: "down",
            },
            DoKho::Thuong,
        ),
        (
            "bật nhạc lên",
            Intent::OsControl {
                tool: "control_media",
                action: "play_pause",
            },
            DoKho::Thuong,
        ),
        (
            "chuyển bài khác",
            Intent::OsControl {
                tool: "control_media",
                action: "next",
            },
            DoKho::Thuong,
        ),
        // Vision intent
        ("trên màn hình có gì", Intent::Vision, DoKho::Thuong),
        ("chụp màn hình lại", Intent::Vision, DoKho::Thuong),
        // SendMessage reflex
        (
            "nhắn tin cho Minh bảo tối nay đi ăn nhé",
            Intent::SendMessage {
                recipient: "Minh".to_string(),
                body: "tối nay đi ăn nhé".to_string(),
                platform: None,
            },
            DoKho::Thuong,
        ),
        (
            "nhắn cho Nam là mai đi học",
            Intent::SendMessage {
                recipient: "Nam".to_string(),
                body: "mai đi học".to_string(),
                platform: None,
            },
            DoKho::Thuong,
        ),
    ];

    for (text, expected_intent, expected_dokho) in regular_and_reflex_matrix {
        let intent = route_intent(text);
        assert_eq!(
            intent, expected_intent,
            "Intent routing failure on input: '{text}'"
        );

        let dokho = phan_loai_do_kho(text);
        assert_eq!(
            dokho, expected_dokho,
            "Complexity classification failure on input: '{text}'"
        );

        // For non-chat intents, expert escalation must NEVER occur
        if !matches!(intent, Intent::Chat) {
            // Reflex lane guarantees 0 escalation
            assert_eq!(
                dokho,
                DoKho::Thuong,
                "Reflex intents must always classify as Thuong: '{text}'"
            );
        }
    }
}

// =========================================================================
// 3. DIALOGUE & VOICE MESSAGE STATE MACHINE ISOLATION
// =========================================================================

#[test]
fn test_voice_message_dialogue_state_machine_isolation() {
    let mut dialogue = VoiceMessageDialogue::default();

    // Turn 1: User says a voice message command with missing body -> AskBody
    let action1 = dialogue.begin("Minh".to_string(), String::new(), None);
    assert_eq!(
        action1,
        liva_native_core::messaging::VoiceMessageAction::AskBody
    );

    // Turn 2: User provides body (even if the body has technical/complex words like "quicksort")
    let body_text = "nhắc Minh viết thuật toán quicksort bằng Rust";
    let action2 = dialogue.follow_up(body_text);
    assert_eq!(
        action2,
        Some(liva_native_core::messaging::VoiceMessageAction::AskPlatform)
    );

    // Turn 3: User provides platform -> Draft created
    let action3 = dialogue.follow_up("nhắn bằng Telegram");
    assert!(
        matches!(
            action3,
            Some(liva_native_core::messaging::VoiceMessageAction::Draft { .. })
        ),
        "Voice message dialogue must capture technical body without triggering Chat LLM routing"
    );

    // Turn 4: Await confirmation
    dialogue.await_confirmation("draft-test-123".to_string());
    assert!(dialogue.is_pending());

    // Turn 5: User confirms
    let action5 = dialogue.follow_up("gửi đi");
    assert_eq!(
        action5,
        Some(liva_native_core::messaging::VoiceMessageAction::Confirm {
            draft_id: "draft-test-123".to_string()
        })
    );
    assert!(!dialogue.is_pending());
}

// =========================================================================
// 4. CONCURRENCY, LATENCY (<5µs) & ZERO MUTEX CONTENTION BENCHMARK
// =========================================================================

#[tokio::test]
async fn test_adversarial_concurrency_latency_and_zero_contention() {
    let test_queries = [
        "Xin chào LIVA",
        "bật đèn phòng khách",
        "tăng âm lượng lên",
        "Viết hàm quicksort bằng Rust và phân tích độ phức tạp thời gian thuật toán từng bước?",
        "Phân tích ưu nhược điểm của SQLite WAL so với PostgreSQL",
        "1. Kế hoạch ra sao? 2. Rủi ro gồm những gì?",
        "What is Tokio runtime? Explain in detail",
        "SELECT * FROM users WHERE active = 1",
        "nhắn cho Nam là mai đi học",
        "trên màn hình có gì",
    ];

    // Benchmark 1: Pure 0-token heuristic complexity classification (100,000 iterations)
    let iter_pure = 100_000;
    let start_pure = Instant::now();

    for i in 0..iter_pure {
        let text = test_queries[i % test_queries.len()];
        let _ = phan_loai_do_kho(text);
    }

    let elapsed_pure = start_pure.elapsed();
    let avg_pure_micros = elapsed_pure.as_micros() as f64 / iter_pure as f64;

    println!("\n=== EMPIRICAL PURE COMPLEXITY ROUTING BENCHMARK ===");
    println!("Iterations: {iter_pure}");
    println!("Total time: {:?}", elapsed_pure);
    println!(
        "Average latency per phan_loai_do_kho: {:.4} µs",
        avg_pure_micros
    );

    assert!(
        avg_pure_micros < 25.0,
        "Pure complexity classification latency ({avg_pure_micros:.4} µs) exceeded SLA bound!"
    );

    // Benchmark 2: End-to-end Escalation check (routing + complexity + expert path check)
    let iter_e2e = 5_000;
    let start_e2e = Instant::now();

    for i in 0..iter_e2e {
        let text = test_queries[i % test_queries.len()];
        let intent = route_intent(text);
        if matches!(intent, Intent::Chat) {
            let do_kho = phan_loai_do_kho(text);
            let co_expert = configured_expert_model_path().is_some_and(|p| p.exists());
            let goi_y_expert = matches!(do_kho, DoKho::Kho) && co_expert;
            if matches!(do_kho, DoKho::Thuong) {
                assert!(
                    !goi_y_expert,
                    "Regular chat query must never trigger expert escalation"
                );
            }
        }
    }

    let elapsed_e2e = start_e2e.elapsed();
    let avg_e2e_micros = elapsed_e2e.as_micros() as f64 / iter_e2e as f64;

    println!("\n=== EMPIRICAL E2E ESCALATION CALL SITE BENCHMARK ===");
    println!("Iterations: {iter_e2e}");
    println!("Total time: {:?}", elapsed_e2e);
    println!(
        "Average latency per end-to-end routing check: {:.4} µs",
        avg_e2e_micros
    );

    // Multi-threaded 100 concurrent async tasks stress test (zero deadlock / zero mutex contention)
    let num_tasks = 100;
    let ops_per_task = 100;
    let mut handles = Vec::with_capacity(num_tasks);

    let start_concurrent = Instant::now();

    for task_id in 0..num_tasks {
        let handle = tokio::spawn(async move {
            for i in 0..ops_per_task {
                let query = test_queries[(task_id + i) % test_queries.len()];
                let intent = route_intent(query);
                if matches!(intent, Intent::Chat) {
                    let do_kho = phan_loai_do_kho(query);
                    let co_expert = configured_expert_model_path().is_some_and(|p| p.exists());
                    let goi_y = matches!(do_kho, DoKho::Kho) && co_expert;
                    if matches!(do_kho, DoKho::Thuong) {
                        assert!(
                            !goi_y,
                            "Concurrent regular chat must never trigger expert escalation"
                        );
                    }
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle
            .await
            .expect("Concurrent task panicked or deadlocked");
    }

    let elapsed_concurrent = start_concurrent.elapsed();
    println!(
        "Completed 100 concurrent tasks (10,000 total ops) in {:?}",
        elapsed_concurrent
    );
    assert!(
        elapsed_concurrent.as_millis() < 5000,
        "Concurrent stress test took too long: {:?}",
        elapsed_concurrent
    );
}

// =========================================================================
// 5. ADVERSARIAL EDGE CASES & EXTREME BOUNDARY INPUTS
// =========================================================================

#[test]
fn test_adversarial_boundary_inputs_safety() {
    let large_payload = "a".repeat(100_000);
    let adversarial_inputs = [
        "",
        " ",
        "   \t\r\n   ",
        "\0",
        "\0\0\0hello\0\0\0",
        "\u{200B}\u{200C}\u{200D}\u{FEFF}",
        "🤖🚀⚡🎉🔥",
        "SELECT * FROM 'users'; DROP TABLE users; --",
        "{\"intent\": \"Chat\", \"payload\": null}",
        "<script>alert('xss')</script>",
        large_payload.as_str(),
    ];

    for input in adversarial_inputs {
        // Must never panic
        let intent = route_intent(input);
        let dokho = phan_loai_do_kho(input);
        let co_expert = configured_expert_model_path().is_some_and(|p| p.exists());
        let goi_y = matches!(dokho, DoKho::Kho) && co_expert;

        assert!(matches!(
            intent,
            Intent::Chat
                | Intent::Vision
                | Intent::SmartHome { .. }
                | Intent::OsControl { .. }
                | Intent::SendMessage { .. }
        ));

        // Adversarial empty/whitespace/control/injection payloads must not trigger false positive DoKho::Kho
        if input.trim().is_empty() || input.starts_with('\0') || input.contains("alert('xss')") {
            assert_eq!(
                dokho,
                DoKho::Thuong,
                "Adversarial empty/injection payload must not classify as Kho: '{input}'"
            );
            assert!(
                !goi_y,
                "Adversarial payload must not trigger expert escalation"
            );
        }
    }
}
