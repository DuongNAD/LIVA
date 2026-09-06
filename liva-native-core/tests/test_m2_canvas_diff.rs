//! Milestone 2 Integration Tests: Living Canvas & Line-by-Line Diff Reviewer
//!
//! Validates:
//! 1. Unified Diff Parsing (AST, line types, coordinate tracking, multi-file).
//! 2. Offset-Adjusting Patch Reconstruction (selective approval, rejection, user modifications).
//! 3. Thread-Safe Session Management (`DiffReviewRegistry`, state transitions).
//! 4. Human-In-The-Loop Pregel State Graph Suspension & Resumption.
//! 5. Command Layer Dispatch (`diff:*`, `agent:submit_hunk_decision`, `canvas:*`).
//! 6. Security Authorization Matrix (fail-closed enforcement for untrusted principals).

use liva_native_core::agent::graph::{
    ApprovalContext, ApprovalDecision, DiffLineType, DiffReviewRegistry, DiffReviewSession,
    DiffReviewStatus, HunkStatus, create_diff_review_context, evaluate_session_decision,
    parse_unified_diff, reconstruct_approved_patch,
};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{
    AppState, CommandPrincipal, authorize_command, db, handle_command_as, llm, stt, tts,
};
use serde_json::json;
use std::sync::Arc;

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
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

const COMPLEX_MULTI_FILE_DIFF: &str = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,6 +1,8 @@
-use old_lib::init;
+use new_lib::init;
+use new_lib::logger;
 
 fn main() {
+    logger::setup();
     init();
 }
@@ -20,4 +22,5 @@
     let a = 1;
-    let b = 2;
+    let b = 3;
+    let c = 4;
 }
diff --git a/src/config.rs b/src/config.rs
--- a/src/config.rs
+++ b/src/config.rs
@@ -10,3 +10,4 @@
 pub struct Config {
+    pub verbose: bool,
     pub port: u16,
 }
"#;

// ============================================================================
// 1. UNIFIED DIFF PARSING & LINE TRACKING
// ============================================================================

#[test]
fn test_parse_multi_file_unified_diff() {
    let files = parse_unified_diff(COMPLEX_MULTI_FILE_DIFF).expect("parse complex diff");
    assert_eq!(files.len(), 2);

    // File 1: main.rs
    let f1 = &files[0];
    assert_eq!(f1.old_path.as_deref(), Some("src/main.rs"));
    assert_eq!(f1.new_path.as_deref(), Some("src/main.rs"));
    assert_eq!(f1.hunks.len(), 2);

    let h1 = &f1.hunks[0];
    assert_eq!(h1.old_start, 1);
    assert_eq!(h1.old_lines, 6);
    assert_eq!(h1.new_start, 1);
    assert_eq!(h1.new_lines, 8);
    assert_eq!(h1.status, HunkStatus::Pending);

    // Check line annotations in Hunk 1
    assert_eq!(h1.lines[0].line_type, DiffLineType::Deletion);
    assert_eq!(h1.lines[0].old_line_no, Some(1));
    assert_eq!(h1.lines[0].new_line_no, None);

    assert_eq!(h1.lines[1].line_type, DiffLineType::Addition);
    assert_eq!(h1.lines[1].old_line_no, None);
    assert_eq!(h1.lines[1].new_line_no, Some(1));

    // File 2: config.rs
    let f2 = &files[1];
    assert_eq!(f2.old_path.as_deref(), Some("src/config.rs"));
    assert_eq!(f2.new_path.as_deref(), Some("src/config.rs"));
    assert_eq!(f2.hunks.len(), 1);
}

#[test]
fn test_parse_new_and_deleted_files() {
    let new_file_diff = r#"--- /dev/null
+++ b/src/new_module.rs
@@ -0,0 +1,3 @@
+pub fn hello() {
+    println!("hello");
+}
"#;
    let files = parse_unified_diff(new_file_diff).expect("parse new file");
    assert_eq!(files.len(), 1);
    assert!(files[0].is_new);
    assert_eq!(files[0].old_path.as_deref(), Some("/dev/null"));
    assert_eq!(files[0].new_path.as_deref(), Some("src/new_module.rs"));
    assert_eq!(files[0].hunks[0].lines.len(), 3);
}

// ============================================================================
// 2. OFFSET-ADJUSTING PATCH RECONSTRUCTION
// ============================================================================

#[test]
fn test_selective_hunk_approval_recalculation() {
    let mut files = parse_unified_diff(COMPLEX_MULTI_FILE_DIFF).expect("parse diff");

    // File 1, Hunk 1: REJECTED (skips +2 additions, -1 deletion -> net 0 offset contribution)
    files[0].hunks[0].status = HunkStatus::Rejected {
        reason: Some("Keep old imports".into()),
    };

    // File 1, Hunk 2: APPROVED (originally starts at old 20, new 22. With Hunk 1 rejected, new_start becomes 20)
    files[0].hunks[1].status = HunkStatus::Approved;

    // File 2, Hunk 1: APPROVED
    files[1].hunks[0].status = HunkStatus::Approved;

    let patch = reconstruct_approved_patch(&files).expect("reconstruct patch");

    // Verify Hunk 1 was omitted from main.rs
    assert!(!patch.contains("use new_lib::logger;"));

    // Verify Hunk 2 was emitted with adjusted offset: @@ -20,4 +20,5 @@
    assert!(patch.contains("@@ -20,4 +20,5 @@"));
    assert!(patch.contains("+    let c = 4;"));

    // Verify File 2 was included
    assert!(patch.contains("+++ b/src/config.rs"));
    assert!(patch.contains("+    pub verbose: bool,"));
}

#[test]
fn test_user_modified_hunk_reconstruction() {
    let mut files = parse_unified_diff(COMPLEX_MULTI_FILE_DIFF).expect("parse diff");

    // Modify File 1 Hunk 1 with custom replacement lines
    files[0].hunks[0].status = HunkStatus::Modified {
        user_override: "use my_custom_lib::init;\nuse my_custom_lib::logger;\nuse my_custom_lib::telemetry;".into(),
    };
    files[0].hunks[1].status = HunkStatus::Approved;

    let patch = reconstruct_approved_patch(&files).expect("reconstruct patch");
    assert!(patch.contains("+use my_custom_lib::telemetry;"));
    assert!(patch.contains("(modified)"));
}

// ============================================================================
// 3. THREAD-SAFE REGISTRY & DECISION STATE MACHINE
// ============================================================================

#[test]
fn test_diff_review_registry_full_lifecycle() {
    let files = parse_unified_diff(COMPLEX_MULTI_FILE_DIFF).expect("parse diff");
    let session_id = "sess-m2-test-01";
    let session = DiffReviewSession::new(session_id, "thread-42", "act-hitl-99", files);

    let registry = DiffReviewRegistry::new();
    registry.create_session(session);

    assert_eq!(registry.list_pending().len(), 1);

    let s = registry.get_session(session_id).unwrap();
    assert_eq!(s.total_hunks(), 3);
    assert_eq!(s.pending_hunks_count(), 3);
    assert!(!s.is_fully_decided());

    // Submit individual decisions
    let h1_id = s.files[0].hunks[0].hunk_id.clone();
    let h2_id = s.files[0].hunks[1].hunk_id.clone();
    let h3_id = s.files[1].hunks[0].hunk_id.clone();

    registry
        .submit_decision(session_id, &h1_id, HunkStatus::Approved)
        .unwrap();
    registry
        .submit_decision(session_id, &h2_id, HunkStatus::Rejected { reason: None })
        .unwrap();

    let intermediate = registry.get_session(session_id).unwrap();
    assert_eq!(intermediate.pending_hunks_count(), 1);
    assert_eq!(intermediate.status, DiffReviewStatus::Pending);
    assert_eq!(evaluate_session_decision(&intermediate), None);

    // Final decision
    let final_s = registry
        .submit_decision(session_id, &h3_id, HunkStatus::Approved)
        .unwrap();
    assert_eq!(final_s.pending_hunks_count(), 0);
    assert_eq!(final_s.status, DiffReviewStatus::PartiallyApproved);
    assert!(final_s.is_fully_decided());

    let decision = evaluate_session_decision(&final_s).expect("decision evaluated");
    match decision {
        ApprovalDecision::Approved { modified_args } => {
            let args = modified_args.expect("has modified args");
            assert_eq!(args["session_id"], session_id);
            assert_eq!(args["approved_count"], 2);
            assert_eq!(args["rejected_count"], 1);
            let patch = args["approved_patch"].as_str().unwrap();
            assert!(patch.contains("use new_lib::init;"));
        }
        _ => panic!("Expected Approved decision"),
    }
}

// ============================================================================
// 4. PREGEL HITL SUSPENSION INTEGRATION
// ============================================================================

#[test]
fn test_pregel_hitl_approval_context_creation() {
    let files = parse_unified_diff(COMPLEX_MULTI_FILE_DIFF).expect("parse diff");
    let session = DiffReviewSession::new("sess-pregel-01", "thread-101", "act-patch-01", files);

    let ctx: ApprovalContext = create_diff_review_context(&session);
    assert_eq!(ctx.action_id, "act-patch-01");
    assert_eq!(ctx.tool_name, "diff_reviewer");
    assert_eq!(ctx.arguments["session_id"], "sess-pregel-01");
    assert_eq!(ctx.arguments["total_hunks"], 3);
    assert_eq!(ctx.arguments["files_count"], 2);
    assert!(!ctx.is_expired_now());
}

// ============================================================================
// 5. COMMAND DISPATCH LAYER & AUTHORIZATION MATRIX
// ============================================================================

#[tokio::test]
async fn test_diff_commands_via_authorized_principals() {
    let state = test_state();

    // 1. diff:parse_raw_diff
    let parse_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "diff:parse_raw_diff",
        json!({ "raw_diff": COMPLEX_MULTI_FILE_DIFF }),
        None,
        None,
    )
    .await
    .expect("authorized parse");

    assert_eq!(parse_res["total_hunks"], 3);
    assert_eq!(parse_res["files_count"], 2);

    // 2. Create session in global registry for testing
    let files = parse_unified_diff(COMPLEX_MULTI_FILE_DIFF).unwrap();
    let session_id = "sess-ipc-test-1";
    let session = DiffReviewSession::new(session_id, "th-ipc", "act-ipc", files);
    DiffReviewRegistry::global().create_session(session);

    // 3. diff:get_pending_hunks
    let pending_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "diff:get_pending_hunks",
        json!({ "session_id": session_id }),
        None,
        None,
    )
    .await
    .expect("get pending");

    assert_eq!(pending_res["session_id"], session_id);

    // 4. agent:submit_hunk_decision (batch approve all)
    let submit_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "agent:submit_hunk_decision",
        json!({
            "session_id": session_id,
            "batch": "approve_all"
        }),
        None,
        None,
    )
    .await
    .expect("batch submit decision");

    assert_eq!(submit_res["status"], "ok");
    assert_eq!(submit_res["is_fully_decided"], true);
}

#[tokio::test]
async fn test_canvas_streaming_and_state_commands() {
    let state = test_state();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);

    // 1. canvas:stream_widget
    let stream_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "canvas:stream_widget",
        json!({
            "widget_id": "widget-m2-e2e",
            "title": "Interactive Data Visualizer",
            "html": "<div id='app'>Canvas Widget</div>",
            "css": "#app { background: #000; }",
            "js": "console.log('ready');",
            "props": { "points": [10, 20, 30] }
        }),
        Some(tx),
        Some("req-m2-001".to_string()),
    )
    .await
    .expect("stream widget");

    assert_eq!(stream_res["status"], "ok");
    assert_eq!(stream_res["widget"]["widget_id"], "widget-m2-e2e");

    // Receive SSE frame
    let frame = rx.try_recv().expect("received frame over channel");
    assert!(frame.contains("canvas_widget_frame"));
    assert!(frame.contains("widget-m2-e2e"));

    // 2. canvas:get_canvas_state
    let canvas_state = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "canvas:get_canvas_state",
        json!({}),
        None,
        None,
    )
    .await
    .expect("get canvas state");

    assert!(canvas_state["active_widgets"]["widget-m2-e2e"].is_object());

    // 3. canvas:set_layout
    let layout_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "canvas:set_layout",
        json!({
            "split_ratio": 0.65,
            "active_mode": "hybrid"
        }),
        None,
        None,
    )
    .await
    .expect("set layout");

    assert_eq!(layout_res["status"], "ok");
    assert_eq!(layout_res["layout"]["split_ratio"], 0.65);

    // 4. canvas:close_widget
    let close_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "canvas:close_widget",
        json!({ "widget_id": "widget-m2-e2e" }),
        None,
        None,
    )
    .await
    .expect("close widget");

    assert_eq!(close_res["removed"], true);
}

#[test]
fn test_canvas_diff_fail_closed_security_matrix() {
    let unprivileged_principals = [
        CommandPrincipal::Telegram,
        CommandPrincipal::TauriSetup,
        CommandPrincipal::WebSocketRemote,
    ];

    let commands_to_block = [
        "diff:get_pending_hunks",
        "diff:parse_raw_diff",
        "diff:submit_hunk_decision",
        "agent:submit_hunk_decision",
        "canvas:stream_widget",
        "canvas:update_widget_state",
        "canvas:close_widget",
    ];

    for principal in unprivileged_principals {
        for cmd in commands_to_block {
            assert!(
                authorize_command(principal, cmd).is_err(),
                "Principal {:?} MUST NOT be authorized for command '{}'",
                principal,
                cmd
            );
        }
    }
}
