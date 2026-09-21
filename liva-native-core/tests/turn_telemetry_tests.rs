use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::{
    DatabasePool, TurnTelemetryRecord, get_telemetry_summary, record_turn_telemetry,
};
use liva_native_core::{AppState, CommandPrincipal, handle_command_as, llm, stt, tts};
use serde_json::json;
use std::sync::Arc;

fn test_state() -> Arc<AppState> {
    let db = DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
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
    })
}

#[tokio::test]
async fn test_telemetry_summary_empty_database() {
    let pool = DatabasePool::new_in_memory().expect("create in-memory db");
    let summary = pool
        .spawn_reader(|conn| get_telemetry_summary(conn, None))
        .await
        .expect("query summary");

    assert_eq!(summary.total_turns, 0);
    assert_eq!(summary.ok_turns, 0);
    assert_eq!(summary.err_turns, 0);
    assert_eq!(summary.avg_latency_ms, 0.0);
    assert_eq!(summary.p50_latency_ms, 0);
    assert_eq!(summary.p95_latency_ms, 0);
    assert_eq!(summary.total_prompt_tokens, 0);
    assert_eq!(summary.total_completion_tokens, 0);
    assert!(summary.models.is_empty());
    assert!(summary.entry_paths.is_empty());
}

#[tokio::test]
async fn test_telemetry_recording_and_aggregation_math() {
    let pool = DatabasePool::new_in_memory().expect("create in-memory db");

    // Ghi 10 bản ghi với các độ trễ: 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000 ms
    let records = vec![
        TurnTelemetryRecord {
            id: None,
            event_id: Some("evt-1".to_string()),
            ts: 1000,
            entry_path: "chat".to_string(),
            model_id: "qwen-router".to_string(),
            prompt_tokens: 50,
            completion_tokens: 30,
            latency_ms: 100,
            outcome: "ok".to_string(),
            err_kind: None,
        },
        TurnTelemetryRecord {
            id: None,
            event_id: Some("evt-2".to_string()),
            ts: 1010,
            entry_path: "chat".to_string(),
            model_id: "qwen-router".to_string(),
            prompt_tokens: 60,
            completion_tokens: 40,
            latency_ms: 200,
            outcome: "ok".to_string(),
            err_kind: None,
        },
        TurnTelemetryRecord {
            id: None,
            event_id: Some("evt-3".to_string()),
            ts: 1020,
            entry_path: "voice".to_string(),
            model_id: "qwen-router".to_string(),
            prompt_tokens: 40,
            completion_tokens: 20,
            latency_ms: 300,
            outcome: "ok".to_string(),
            err_kind: None,
        },
        TurnTelemetryRecord {
            id: None,
            event_id: Some("evt-4".to_string()),
            ts: 1030,
            entry_path: "voice".to_string(),
            model_id: "qwen-router".to_string(),
            prompt_tokens: 45,
            completion_tokens: 25,
            latency_ms: 400,
            outcome: "ok".to_string(),
            err_kind: None,
        },
        TurnTelemetryRecord {
            id: None,
            event_id: Some("evt-5".to_string()),
            ts: 1040,
            entry_path: "graph".to_string(),
            model_id: "qwen-expert".to_string(),
            prompt_tokens: 120,
            completion_tokens: 80,
            latency_ms: 500,
            outcome: "ok".to_string(),
            err_kind: None,
        },
        TurnTelemetryRecord {
            id: None,
            event_id: Some("evt-6".to_string()),
            ts: 1050,
            entry_path: "graph".to_string(),
            model_id: "qwen-expert".to_string(),
            prompt_tokens: 130,
            completion_tokens: 90,
            latency_ms: 600,
            outcome: "ok".to_string(),
            err_kind: None,
        },
        TurnTelemetryRecord {
            id: None,
            event_id: Some("evt-7".to_string()),
            ts: 1060,
            entry_path: "vision".to_string(),
            model_id: "qwen-vl".to_string(),
            prompt_tokens: 200,
            completion_tokens: 50,
            latency_ms: 700,
            outcome: "ok".to_string(),
            err_kind: None,
        },
        TurnTelemetryRecord {
            id: None,
            event_id: Some("evt-8".to_string()),
            ts: 1070,
            entry_path: "vision".to_string(),
            model_id: "qwen-vl".to_string(),
            prompt_tokens: 210,
            completion_tokens: 60,
            latency_ms: 800,
            outcome: "ok".to_string(),
            err_kind: None,
        },
        TurnTelemetryRecord {
            id: None,
            event_id: Some("evt-9".to_string()),
            ts: 1080,
            entry_path: "chat".to_string(),
            model_id: "qwen-router".to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
            latency_ms: 900,
            outcome: "err".to_string(),
            err_kind: Some("timeout".to_string()),
        },
        TurnTelemetryRecord {
            id: None,
            event_id: Some("evt-10".to_string()),
            ts: 1090,
            entry_path: "voice".to_string(),
            model_id: "qwen-router".to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
            latency_ms: 1000,
            outcome: "err".to_string(),
            err_kind: Some("llm_engine_error".to_string()),
        },
    ];

    for rec in records {
        let r = rec.clone();
        pool.spawn_writer(move |conn| record_turn_telemetry(conn, &r))
            .await
            .expect("record turn");
    }

    let summary = pool
        .spawn_reader(|conn| get_telemetry_summary(conn, None))
        .await
        .expect("get summary");

    assert_eq!(summary.total_turns, 10);
    assert_eq!(summary.ok_turns, 8);
    assert_eq!(summary.err_turns, 2);
    // (100 + 200 + ... + 1000) / 10 = 5500 / 10 = 550.0
    assert_eq!(summary.avg_latency_ms, 550.0);
    // 10 items sorted: [100, 200, 300, 400, 500, 600, 700, 800, 900, 1000]
    // p50 index = floor(10 * 0.5) = 5 -> item 600
    assert_eq!(summary.p50_latency_ms, 600);
    // p95 index = floor(10 * 0.95) = 9 -> item 1000
    assert_eq!(summary.p95_latency_ms, 1000);

    // Token totals: 50+60+40+45+120+130+200+210 = 855 prompt tokens
    assert_eq!(summary.total_prompt_tokens, 855);
    // 30+40+20+25+80+90+50+60 = 395 completion tokens
    assert_eq!(summary.total_completion_tokens, 395);

    // Grouping by model
    assert_eq!(summary.models.get("qwen-router"), Some(&6));
    assert_eq!(summary.models.get("qwen-expert"), Some(&2));
    assert_eq!(summary.models.get("qwen-vl"), Some(&2));

    // Grouping by entry path
    assert_eq!(summary.entry_paths.get("chat"), Some(&3));
    assert_eq!(summary.entry_paths.get("voice"), Some(&3));
    assert_eq!(summary.entry_paths.get("graph"), Some(&2));
    assert_eq!(summary.entry_paths.get("vision"), Some(&2));
}

#[tokio::test]
async fn test_telemetry_summary_since_ts_filter() {
    let pool = DatabasePool::new_in_memory().expect("create in-memory db");

    let rec1 = TurnTelemetryRecord {
        id: None,
        event_id: None,
        ts: 100,
        entry_path: "chat".to_string(),
        model_id: "m1".to_string(),
        prompt_tokens: 10,
        completion_tokens: 10,
        latency_ms: 100,
        outcome: "ok".to_string(),
        err_kind: None,
    };
    let rec2 = TurnTelemetryRecord {
        id: None,
        event_id: None,
        ts: 200,
        entry_path: "chat".to_string(),
        model_id: "m1".to_string(),
        prompt_tokens: 20,
        completion_tokens: 20,
        latency_ms: 200,
        outcome: "ok".to_string(),
        err_kind: None,
    };
    let rec3 = TurnTelemetryRecord {
        id: None,
        event_id: None,
        ts: 300,
        entry_path: "chat".to_string(),
        model_id: "m2".to_string(),
        prompt_tokens: 30,
        completion_tokens: 30,
        latency_ms: 300,
        outcome: "ok".to_string(),
        err_kind: None,
    };

    for r in [rec1, rec2, rec3] {
        pool.spawn_writer(move |conn| record_turn_telemetry(conn, &r))
            .await
            .expect("insert record");
    }

    // Filter since ts = 200: should match rec2 and rec3 (ts 200 and 300)
    let filtered = pool
        .spawn_reader(|conn| get_telemetry_summary(conn, Some(200)))
        .await
        .expect("filtered summary");

    assert_eq!(filtered.total_turns, 2);
    assert_eq!(filtered.total_prompt_tokens, 50);
    assert_eq!(filtered.total_completion_tokens, 50);
    assert_eq!(filtered.avg_latency_ms, 250.0);
    assert_eq!(filtered.models.get("m1"), Some(&1));
    assert_eq!(filtered.models.get("m2"), Some(&1));
}

#[tokio::test]
async fn test_telemetry_summary_command_authorization_and_invocation() {
    let state = test_state();

    // 1. Ghi một bản ghi mẫu vào DB của state
    let rec = TurnTelemetryRecord {
        id: None,
        event_id: Some("e2e-evt".to_string()),
        ts: 5000,
        entry_path: "chat".to_string(),
        model_id: "model-e2e".to_string(),
        prompt_tokens: 15,
        completion_tokens: 25,
        latency_ms: 120,
        outcome: "ok".to_string(),
        err_kind: None,
    };
    state
        .db
        .spawn_writer(move |conn| record_turn_telemetry(conn, &rec))
        .await
        .expect("seed telemetry record");

    // 2. TauriDashboard gọi thành công
    let dashboard_resp = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "telemetry:summary",
        json!({}),
        None,
        None,
    )
    .await
    .expect("dashboard query telemetry summary");
    assert_eq!(dashboard_resp["total_turns"], 1);
    assert_eq!(dashboard_resp["ok_turns"], 1);
    assert_eq!(dashboard_resp["err_turns"], 0);
    assert_eq!(dashboard_resp["total_prompt_tokens"], 15);
    assert_eq!(dashboard_resp["total_completion_tokens"], 25);
    assert_eq!(dashboard_resp["models"]["model-e2e"], 1);

    // 3. TauriWidget gọi thành công
    let widget_resp = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "telemetry:summary",
        json!({ "since_ts": 4000 }),
        None,
        None,
    )
    .await
    .expect("widget query telemetry summary");
    assert_eq!(widget_resp["total_turns"], 1);

    // 4. TauriSetup bị từ chối
    let setup_err = handle_command_as(
        CommandPrincipal::TauriSetup,
        state,
        "telemetry:summary",
        json!({}),
        None,
        None,
    )
    .await
    .expect_err("setup must be rejected");
    assert!(setup_err.contains("TauriSetup"));
    assert!(setup_err.contains("telemetry:summary"));
}
