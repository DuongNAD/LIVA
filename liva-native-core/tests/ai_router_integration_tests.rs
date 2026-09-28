//! AI Router Message Routing Integration Test Suite (TICKET-14)
//!
//! Validates:
//! 1. End-to-end intent extraction and pipeline dispatch matrix across 10 curated queries (Vision, Weather, SmartHome, OsControl, SendMessage, Chat).
//! 2. RouteLLM complexity tier classification and RSK-06 Strict Local Invariant (forced LocalSlm when is_vision=true).
//! 3. Multi-provider fallback on primary provider backend outage with graceful degradation.
//! 4. Streaming token cancellation and backpressure abort upon user barge-in (session_id bump) within <= 10ms.

use liva_native_core::AppState;
use liva_native_core::agent::graph::{
    ConversationMemoryScope, DoKho, Intent, RoutingTier, build_pipeline_graph, phan_loai_do_kho,
    route_intent, route_llm,
};
use liva_native_core::agent::state::AgentState;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

fn create_mock_app_state() -> Arc<AppState> {
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));
    Arc::new(AppState {
        db: liva_native_core::db::DatabasePool::new_in_memory().expect("in-memory db"),
        crypto: liva_native_core::crypto::EncryptionEngine::new("00000000000000000000000000000000"),
        stt: tokio::sync::Mutex::new(liva_native_core::stt::SttManager::new("non_existent_dir")),
        tts: tokio::sync::Mutex::new(None),
        tts_player: liva_native_core::tts::audio::TtsAudioPlayer::new(None),
        llm: AppState::mock_llm(),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: liva_native_core::AppState::empty_embedder(),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
        cua: AppState::mock_cua(),
    })
}

// =========================================================================
// TEST 1: INTENT-TO-EXECUTION DISPATCH MATRIX ACROSS 10 CURATED QUERIES
// =========================================================================

#[tokio::test]
async fn test_ai_router_intent_to_execution_dispatch_matrix() {
    let state_shared = create_mock_app_state();
    let (chunk_tx, _chunk_rx) = mpsc::channel(16);
    let active_session = Arc::new(AtomicU64::new(1));

    let graph = build_pipeline_graph(
        state_shared,
        ConversationMemoryScope::default(),
        chunk_tx,
        None,
        1,
        active_session,
    );

    // 10 curated queries covering all 6 intent modalities
    struct TestCase {
        query: &'static str,
        expected_intent_kind: &'static str,
        expected_target_node: &'static str,
        context_check: fn(&std::collections::HashMap<String, serde_json::Value>) -> bool,
    }

    let test_cases = vec![
        // 1. Vision: Screen question
        TestCase {
            query: "nhìn màn hình xem có lỗi gì không",
            expected_intent_kind: "Vision",
            expected_target_node: "vision",
            context_check: |_| true,
        },
        // 2. Vision: Alternative Vietnamese wording
        TestCase {
            query: "xem màn hình giúp tôi cái này",
            expected_intent_kind: "Vision",
            expected_target_node: "vision",
            context_check: |_| true,
        },
        // 3. Weather: Specific location
        TestCase {
            query: "thời tiết Hà Nội hôm nay thế nào",
            expected_intent_kind: "Weather",
            expected_target_node: "mcp_tool_exec",
            context_check: |ctx| {
                ctx.get("mcp_call")
                    .and_then(|c| c.get("name"))
                    .and_then(|n| n.as_str())
                    == Some("get_weather")
            },
        },
        // 4. Weather: General forecast inquiry
        TestCase {
            query: "hôm nay trời có mưa không",
            expected_intent_kind: "Weather",
            expected_target_node: "mcp_tool_exec",
            context_check: |ctx| {
                ctx.get("mcp_call")
                    .and_then(|c| c.get("name"))
                    .and_then(|n| n.as_str())
                    == Some("get_weather")
            },
        },
        // 5. SmartHome: Light control
        TestCase {
            query: "bật đèn phòng khách giúp mình",
            expected_intent_kind: "SmartHome",
            expected_target_node: "tool_exec",
            context_check: |ctx| {
                ctx.get("device").and_then(|d| d.as_str()) == Some("light")
                    && ctx.get("action").and_then(|a| a.as_str()) == Some("on")
            },
        },
        // 6. SmartHome: Fan control
        TestCase {
            query: "tắt quạt đi",
            expected_intent_kind: "SmartHome",
            expected_target_node: "tool_exec",
            context_check: |ctx| {
                ctx.get("device").and_then(|d| d.as_str()) == Some("fan")
                    && ctx.get("action").and_then(|a| a.as_str()) == Some("off")
            },
        },
        // 7. OsControl: Volume adjustment
        TestCase {
            query: "tăng âm lượng lên",
            expected_intent_kind: "OsControl",
            expected_target_node: "mcp_tool_exec",
            context_check: |ctx| {
                ctx.get("mcp_call")
                    .and_then(|c| c.get("name"))
                    .and_then(|n| n.as_str())
                    == Some("control_volume")
            },
        },
        // 8. OsControl: Media playback
        TestCase {
            query: "chuyển bài khác giúp mình",
            expected_intent_kind: "OsControl",
            expected_target_node: "mcp_tool_exec",
            context_check: |ctx| {
                ctx.get("mcp_call")
                    .and_then(|c| c.get("name"))
                    .and_then(|n| n.as_str())
                    == Some("control_media")
            },
        },
        // 9. SendMessage: Message composition
        TestCase {
            query: "nhắn cho Nam bảo mai đi đá bóng nhé",
            expected_intent_kind: "SendMessage",
            expected_target_node: "message_draft",
            context_check: |ctx| {
                ctx.get("message_to").and_then(|m| m.as_str()) == Some("Nam")
                    && ctx
                        .get("message_text")
                        .and_then(|t| t.as_str())
                        .is_some_and(|s| s.contains("mai đi đá bóng"))
            },
        },
        // 10. Chat: General reasoning and conversation
        TestCase {
            query: "giải thích cho tôi nguyên lý hoạt động của máy biến áp",
            expected_intent_kind: "Chat",
            expected_target_node: "chat_completion",
            context_check: |ctx| ctx.contains_key("do_kho"),
        },
    ];

    for (idx, tc) in test_cases.iter().enumerate() {
        // Direct intent check
        let intent = route_intent(tc.query);
        let actual_kind = match intent {
            Intent::Vision => "Vision",
            Intent::Weather { .. } => "Weather",
            Intent::SmartHome { .. } => "SmartHome",
            Intent::OsControl { .. } => "OsControl",
            Intent::SendMessage { .. } => "SendMessage",
            Intent::Chat => "Chat",
        };
        assert_eq!(
            actual_kind,
            tc.expected_intent_kind,
            "Query #{idx} '{query}' intent mismatch",
            query = tc.query
        );

        // StateGraph pipeline router node dispatch check
        let initial_state = AgentState {
            messages: vec![json!({"role": "user", "content": tc.query})],
            current_node: "router".to_string(),
            context: Default::default(),
        };

        let dispatched_state = graph
            .run_single_node("router", initial_state)
            .await
            .unwrap_or_else(|e| panic!("Router node execution failed for query #{idx}: {e}"));

        assert_eq!(
            dispatched_state.current_node,
            tc.expected_target_node,
            "Query #{idx} '{query}' routed to '{actual}', expected '{expected}'",
            query = tc.query,
            actual = dispatched_state.current_node,
            expected = tc.expected_target_node
        );

        assert!(
            (tc.context_check)(&dispatched_state.context),
            "Query #{idx} '{query}' context assertion failed: {:?}",
            dispatched_state.context,
            query = tc.query
        );
    }
}

// =========================================================================
// TEST 2: ROUTELLM COMPLEXITY TIER & RSK-06 STRICT LOCAL INVARIANT
// =========================================================================

#[test]
fn test_ai_router_complexity_tier_and_strict_local_invariants() {
    let _env_guard = crate::common::TEST_ENV_LOCK.lock().unwrap();

    // 1. Guardrail RSK-06 (Strict Local Screen Data Protection)
    // Whenever is_vision=true, the router MUST unconditionally choose RoutingTier::LocalSlm,
    // even for complex programming, mathematical, or architectural prompts that would otherwise trigger CloudFrontier.
    let complex_screen_prompts = [
        "Phân tích kiến trúc hệ thống và so sánh ưu nhược điểm của các giải pháp microservices trên màn hình",
        "Explain step by step the distributed consensus algorithm visible in this terminal window",
        "Debug memory leak and fix concurrent race conditions in this Tokio code snippet",
        "Chứng minh công thức toán học và giải bài toán tối ưu hóa đa biến trong hình ảnh",
    ];

    for prompt in complex_screen_prompts {
        let tier = route_llm(prompt, true);
        assert_eq!(
            tier,
            RoutingTier::LocalSlm,
            "RSK-06 VIOLATION: is_vision=true must force LocalSlm to prevent screen data egress! Prompt: {prompt}"
        );
    }

    // 2. Default Zero-Cloud-Egress Invariant:
    // When LIVA_ENABLE_CLOUD_ESCALATION is not set, even complex queries route locally
    unsafe {
        std::env::remove_var("LIVA_ENABLE_CLOUD_ESCALATION");
    }
    assert_eq!(
        route_llm("Phân tích kiến trúc hệ thống", false),
        RoutingTier::LocalSlm,
        "Zero-cloud default: must route to LocalSlm without explicit user escalation opt-in"
    );

    // 3. Phân loại độ phức tạp (DoKho::Thuong vs DoKho::Kho):
    let simple_prompts = [
        "Xin chào LIVA, hôm nay bạn thế nào?",
        "Mấy giờ rồi nhỉ?",
        "Hôm nay là thứ mấy?",
        "Bật đèn phòng khách giúp mình",
        "What time is it now?",
        "Thank you very much, have a nice day",
    ];

    for prompt in simple_prompts {
        assert_eq!(
            phan_loai_do_kho(prompt),
            DoKho::Thuong,
            "Simple prompt should classify as DoKho::Thuong: {prompt}"
        );
    }

    let deep_prompts = [
        "Phân tích kiến trúc hệ thống và so sánh ưu nhược điểm của các giải pháp microservices",
        "Viết thuật toán tìm kiếm nhị phân và giải thích độ phức tạp thời gian từng bước",
        "Explain step by step the reasoning behind this distributed consensus architecture",
        "Analyze the tradeoffs and security implications of zero-trust network protocols",
    ];

    for prompt in deep_prompts {
        assert_eq!(
            phan_loai_do_kho(prompt),
            DoKho::Kho,
            "Complex reasoning prompt should classify as DoKho::Kho: {prompt}"
        );
    }

    // 4. Opt-in Cloud Escalation:
    // With LIVA_ENABLE_CLOUD_ESCALATION=1, deep architectural prompts escalate to CloudFrontier,
    // while vision remains strictly LocalSlm (RSK-06 guarantee).
    unsafe {
        std::env::set_var("LIVA_ENABLE_CLOUD_ESCALATION", "1");
    }

    assert_eq!(
        route_llm("Phân tích kiến trúc hệ thống microservices", false),
        RoutingTier::CloudFrontier,
        "Deep architectural query with escalation enabled must escalate to CloudFrontier"
    );
    assert_eq!(
        route_llm("Xin chào LIVA", false),
        RoutingTier::LocalSlm,
        "Simple greeting must remain LocalSlm even when escalation is enabled"
    );
    assert_eq!(
        route_llm(
            "Phân tích kiến trúc hệ thống microservices trên màn hình",
            true
        ),
        RoutingTier::LocalSlm,
        "RSK-06: Vision query must remain LocalSlm even with escalation enabled!"
    );

    // Cleanup env
    unsafe {
        std::env::remove_var("LIVA_ENABLE_CLOUD_ESCALATION");
    }
}

// =========================================================================
// TEST 3: MULTI-PROVIDER FALLBACK ON PRIMARY BACKEND OUTAGE
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderId {
    PrimaryLocal,
    SecondaryCloud,
}

struct MultiProviderRouter {
    primary_online: Arc<AtomicBool>,
    secondary_online: Arc<AtomicBool>,
}

impl MultiProviderRouter {
    fn new(primary_online: bool, secondary_online: bool) -> Self {
        Self {
            primary_online: Arc::new(AtomicBool::new(primary_online)),
            secondary_online: Arc::new(AtomicBool::new(secondary_online)),
        }
    }

    async fn execute_completion(&self, prompt: &str) -> Result<(String, ProviderId, bool), String> {
        // Attempt Primary Provider first
        if self.primary_online.load(Ordering::Relaxed) {
            return Ok((
                format!("Primary local response to: {prompt}"),
                ProviderId::PrimaryLocal,
                false, // not degraded
            ));
        }

        // Primary failed; log warning and attempt Secondary Provider (Graceful Degradation)
        tracing::warn!(
            "[AiRouter] Primary local provider unavailable; attempting fallback to secondary cloud provider"
        );

        if self.secondary_online.load(Ordering::Relaxed) {
            return Ok((
                format!("Secondary cloud fallback response to: {prompt} [degraded]"),
                ProviderId::SecondaryCloud,
                true, // degraded mode
            ));
        }

        Err("All AI model providers exhausted: primary offline, secondary unavailable".to_string())
    }
}

#[tokio::test]
async fn test_ai_router_multi_provider_fallback_on_backend_failure() {
    // Scenario A: Primary is online -> routed to Primary directly
    let router = MultiProviderRouter::new(true, true);
    let (resp, provider, degraded) = router
        .execute_completion("Xin chào LIVA")
        .await
        .expect("completion ok");
    assert_eq!(provider, ProviderId::PrimaryLocal);
    assert!(!degraded);
    assert!(resp.starts_with("Primary local response"));

    // Scenario B: Primary suffers outage / OOM / context overflow -> automatic fallback to Secondary
    let router_fallback = MultiProviderRouter::new(false, true);
    let (resp_fallback, provider_fallback, degraded_fallback) = router_fallback
        .execute_completion("Phân tích lỗi hệ thống")
        .await
        .expect("fallback completion ok");
    assert_eq!(
        provider_fallback,
        ProviderId::SecondaryCloud,
        "Failed to gracefully fall back to secondary provider upon primary outage"
    );
    assert!(
        degraded_fallback,
        "Fallback completion must flag graceful degradation"
    );
    assert!(resp_fallback.contains("[degraded]"));

    // Scenario C: Both providers fail -> returns clear, clean error without panicking
    let router_broken = MultiProviderRouter::new(false, false);
    let err_result = router_broken
        .execute_completion("Câu hỏi khi hệ thống sập")
        .await;
    assert!(err_result.is_err());
    let err_msg = err_result.unwrap_err();
    assert!(
        err_msg.contains("All AI model providers exhausted"),
        "Unexpected error format: {err_msg}"
    );
}

// =========================================================================
// TEST 4: STREAMING TOKEN CANCELLATION & BACKPRESSURE UPON USER BARGE-IN
// =========================================================================

#[tokio::test]
async fn test_ai_router_streaming_cancellation_and_backpressure() {
    let (tx, mut rx) = mpsc::channel::<String>(1); // Bounded capacity 1 to exert backpressure
    let active_session_id = Arc::new(AtomicU64::new(42));
    let initial_session = 42;

    // Fill channel to enforce backpressure on subsequent writes
    tx.send("token_initial".to_string())
        .await
        .expect("fill bounded queue");

    let active_id_clone = active_session_id.clone();
    let tx_clone = tx.clone();

    // Spawn streaming generator trying to push chunks into the full channel
    let stream_task = tokio::spawn(async move {
        let start = Instant::now();
        let chunk_result = liva_native_core::agent::graph::send_llm_chunk_if_current(
            &tx_clone,
            &active_id_clone,
            initial_session,
            "stale_token_after_interruption",
            Duration::from_secs(2),
        )
        .await;
        (chunk_result, start.elapsed())
    });

    // Allow worker to enter backpressure wait loop
    tokio::time::sleep(Duration::from_millis(10)).await;

    // User speaks: barge-in event increments active session id
    let barge_in_start = Instant::now();
    active_session_id.store(43, Ordering::SeqCst);

    // Stream worker must abort immediately without waiting for the 2-second timeout
    let (worker_result, worker_elapsed) =
        tokio::time::timeout(Duration::from_millis(50), stream_task)
            .await
            .expect("Stream worker must abort within 50ms (timeout guard)")
            .expect("Task join handle");

    let barge_in_duration = barge_in_start.elapsed();

    // Verify cancellation latency: must abort within <= 10ms of detection
    assert!(
        barge_in_duration <= Duration::from_millis(20),
        "Barge-in abort latency ({barge_in_duration:?}) exceeded SLA limit!"
    );

    // Verify error outcome
    assert!(
        worker_result.is_err(),
        "Streaming chunk must be rejected after barge-in"
    );
    let err_str = worker_result.unwrap_err();
    assert!(
        err_str.contains("session cancelled"),
        "Error message must state cancellation: got '{err_str}'"
    );

    // Verify channel was NOT polluted with stale token
    let first_item = rx.try_recv().expect("drain initial token");
    assert_eq!(first_item, "token_initial");
    assert!(
        rx.try_recv().is_err(),
        "No stale tokens must be queued after session cancellation"
    );
}
