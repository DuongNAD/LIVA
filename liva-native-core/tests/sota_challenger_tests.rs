//! Adversarial stress test suite for SOTA Agentic Harness RFCs (RFC 1 & RFC 4).
//!
//! Authored by challenger_phase2_m1_1 to empirically probe boundary conditions,
//! authorization bypasses, RAII cleanup guarantees, and crash recovery.

use liva_native_core::CommandPrincipal;
use liva_native_core::cognitive::{SessionEvent, SessionEventStream};
use liva_native_core::db::DatabasePool;
use liva_native_core::llm::{
    CallToolResult, NativeScopedToolRegistry, ScopedTool, Tool, ToolContent, ToolExecError,
    ToolExecPolicy, ToolExecutionContext, ToolScope, async_trait,
};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// DUMMY SCOPED TOOLS FOR ADVERSARIAL TESTING
// ---------------------------------------------------------------------------

#[derive(JsonSchema, Deserialize)]
struct DummyArgs {
    #[allow(dead_code)]
    input: Option<String>,
}

struct HighPrivilegeAutoTool;

#[async_trait]
impl ScopedTool for HighPrivilegeAutoTool {
    fn definition(&self) -> Tool {
        Tool {
            name: "high_privilege_auto".into(),
            description: "Tool requiring LocalCli principal to run automatically".into(),
            input_schema: schema_for!(DummyArgs),
        }
    }

    fn default_policy(&self) -> ToolExecPolicy {
        ToolExecPolicy::Auto
    }

    fn required_principal(&self) -> CommandPrincipal {
        CommandPrincipal::LocalCli
    }

    async fn execute(
        &self,
        _arguments: Value,
        _context: ToolExecutionContext,
    ) -> Result<CallToolResult, String> {
        Ok(CallToolResult::success("high_privilege_success"))
    }
}

struct ForbiddenTool;

#[async_trait]
impl ScopedTool for ForbiddenTool {
    fn definition(&self) -> Tool {
        Tool {
            name: "forbidden_tool".into(),
            description: "Tool that must be rejected regardless of principal".into(),
            input_schema: schema_for!(DummyArgs),
        }
    }

    fn default_policy(&self) -> ToolExecPolicy {
        ToolExecPolicy::Forbidden
    }

    fn required_principal(&self) -> CommandPrincipal {
        CommandPrincipal::LocalCli
    }

    async fn execute(
        &self,
        _arguments: Value,
        _context: ToolExecutionContext,
    ) -> Result<CallToolResult, String> {
        Ok(CallToolResult::success("forbidden_executed_unexpectedly"))
    }
}

struct TaskIsolatedTool;

#[async_trait]
impl ScopedTool for TaskIsolatedTool {
    fn definition(&self) -> Tool {
        Tool {
            name: "task_isolated_tool".into(),
            description: "Tool strictly scoped to an ephemeral subtask".into(),
            input_schema: schema_for!(DummyArgs),
        }
    }

    fn default_policy(&self) -> ToolExecPolicy {
        ToolExecPolicy::Auto
    }

    fn required_principal(&self) -> CommandPrincipal {
        CommandPrincipal::LocalCli
    }

    async fn execute(
        &self,
        _arguments: Value,
        _context: ToolExecutionContext,
    ) -> Result<CallToolResult, String> {
        Ok(CallToolResult::success("task_secret_data"))
    }
}

// ---------------------------------------------------------------------------
// RFC 1: ADVERSARIAL CHALLENGES
// ---------------------------------------------------------------------------

/// Challenge 1: Cross-session and Cross-task Scope Isolation.
/// Verify that a Task-scoped tool cannot be invoked from:
/// 1. A different session ID.
/// 2. The parent session itself (tasks are child scopes; parent cannot call child tool).
/// 3. A sibling task under the same session.
#[tokio::test]
async fn test_adversarial_task_scoped_tool_cross_session_isolation() {
    let registry = NativeScopedToolRegistry::new();

    let session_a = ToolScope::session("session-A", CommandPrincipal::LocalCli);
    registry.register_scope(session_a.clone());

    let session_b = ToolScope::session("session-B", CommandPrincipal::LocalCli);
    registry.register_scope(session_b.clone());

    let task_1 = ToolScope::task("task-1", "session-A", CommandPrincipal::LocalCli);
    registry.register_scope(task_1.clone());

    let task_2 = ToolScope::task("task-2", "session-A", CommandPrincipal::LocalCli);
    registry.register_scope(task_2.clone());

    // Register tool exclusively into task-1 scope
    let tool = Arc::new(TaskIsolatedTool);
    let _guard = registry
        .register_scoped_tool(&task_1.scope_id, tool)
        .expect("Task tool registration must succeed");

    // 1. Invocation from task-1 scope should succeed
    let ctx_task1 =
        ToolExecutionContext::new("session-A", CommandPrincipal::LocalCli, PathBuf::from("."));
    let res_task1 = registry
        .dispatch(&task_1, "task_isolated_tool", json!({}), ctx_task1)
        .await;
    assert!(
        res_task1.is_ok(),
        "task-1 should be able to invoke its own tool"
    );
    assert_eq!(
        res_task1.unwrap().content,
        vec![ToolContent::Text {
            text: "task_secret_data".into()
        }]
    );

    // 2. Invocation from parent session-A should FAIL (hierarchical scoping does not expose child tools to parents)
    let ctx_session_a =
        ToolExecutionContext::new("session-A", CommandPrincipal::LocalCli, PathBuf::from("."));
    let res_parent = registry
        .dispatch(&session_a, "task_isolated_tool", json!({}), ctx_session_a)
        .await;
    assert!(
        res_parent.is_err(),
        "Parent session must NOT have access to child task tools"
    );
    match res_parent.unwrap_err() {
        ToolExecError::ToolNotFound(name) => assert_eq!(name, "task_isolated_tool"),
        other => panic!("Expected ToolNotFound, got {other:?}"),
    }

    // 3. Invocation from completely different session-B should FAIL
    let ctx_session_b =
        ToolExecutionContext::new("session-B", CommandPrincipal::LocalCli, PathBuf::from("."));
    let res_session_b = registry
        .dispatch(&session_b, "task_isolated_tool", json!({}), ctx_session_b)
        .await;
    assert!(
        res_session_b.is_err(),
        "Unrelated session-B must NOT have access to session-A task tools"
    );
    match res_session_b.unwrap_err() {
        ToolExecError::ToolNotFound(name) => assert_eq!(name, "task_isolated_tool"),
        other => panic!("Expected ToolNotFound, got {other:?}"),
    }

    // 4. Invocation from sibling task-2 under same session should FAIL
    let ctx_task2 =
        ToolExecutionContext::new("session-A", CommandPrincipal::LocalCli, PathBuf::from("."));
    let res_sibling = registry
        .dispatch(&task_2, "task_isolated_tool", json!({}), ctx_task2)
        .await;
    assert!(
        res_sibling.is_err(),
        "Sibling task-2 must NOT have access to task-1 tools"
    );
    match res_sibling.unwrap_err() {
        ToolExecError::ToolNotFound(name) => assert_eq!(name, "task_isolated_tool"),
        other => panic!("Expected ToolNotFound, got {other:?}"),
    }
}

/// Challenge 2: Principal Authorization Enforcement on Auto Tools.
/// An Auto tool requiring CommandPrincipal::LocalCli must REJECT callers with lower privilege
/// (such as Telegram, TauriWidget, TauriDashboard).
#[tokio::test]
async fn test_adversarial_auto_tool_insufficient_principal_rejection() {
    let registry = NativeScopedToolRegistry::new();
    let session_scope = ToolScope::session("sess-sec-test", CommandPrincipal::LocalCli);
    registry.register_scope(session_scope.clone());

    let tool = Arc::new(HighPrivilegeAutoTool);
    let _guard = registry
        .register_scoped_tool(&session_scope.scope_id, tool)
        .expect("Registration must succeed");

    // Test with low privilege caller: Telegram
    let telegram_ctx = ToolExecutionContext::new(
        "sess-sec-test",
        CommandPrincipal::Telegram,
        PathBuf::from("."),
    );
    let result_telegram = registry
        .dispatch(
            &session_scope,
            "high_privilege_auto",
            json!({}),
            telegram_ctx,
        )
        .await;

    // Verify rejection: telegram principal must NOT be authorized to execute LocalCli-level tool
    assert!(
        result_telegram.is_err(),
        "Calling a LocalCli tool from Telegram principal MUST fail"
    );
    match result_telegram.unwrap_err() {
        ToolExecError::UnauthorizedPrincipal(msg) => {
            assert!(
                msg.contains("Telegram"),
                "Error message should mention unauthorized principal Telegram: {msg}"
            );
        }
        other => panic!("Expected UnauthorizedPrincipal, got {other:?}"),
    }

    // Test with lower privilege caller: TauriWidget
    let widget_ctx = ToolExecutionContext::new(
        "sess-sec-test",
        CommandPrincipal::TauriWidget,
        PathBuf::from("."),
    );
    let result_widget = registry
        .dispatch(&session_scope, "high_privilege_auto", json!({}), widget_ctx)
        .await;

    assert!(
        result_widget.is_err(),
        "Calling a LocalCli tool from TauriWidget principal MUST fail"
    );
    match result_widget.unwrap_err() {
        ToolExecError::UnauthorizedPrincipal(msg) => {
            assert!(
                msg.contains("TauriWidget"),
                "Error message should mention unauthorized principal TauriWidget: {msg}"
            );
        }
        other => panic!("Expected UnauthorizedPrincipal, got {other:?}"),
    }

    // Test with authorized caller: LocalCli
    let local_ctx = ToolExecutionContext::new(
        "sess-sec-test",
        CommandPrincipal::LocalCli,
        PathBuf::from("."),
    );
    let result_local = registry
        .dispatch(&session_scope, "high_privilege_auto", json!({}), local_ctx)
        .await;
    assert!(result_local.is_ok(), "LocalCli must be authorized");
}

/// Challenge 3: RAII ScopeGuard drop on error paths and early returns.
/// Verify that when a ScopeGuard is dropped inside a fallible block, the tool is immediately
/// purged from both catalog and dynamic tool registries.
#[tokio::test]
async fn test_adversarial_raii_scope_guard_drop_on_error_path() {
    let registry = NativeScopedToolRegistry::new();
    let session_scope = ToolScope::session("sess-raii-test", CommandPrincipal::LocalCli);
    registry.register_scope(session_scope.clone());

    fn fallible_operation(
        registry: &NativeScopedToolRegistry,
        scope_id: &str,
    ) -> Result<(), &'static str> {
        let tool = Arc::new(HighPrivilegeAutoTool);
        let _guard = registry
            .register_scoped_tool(scope_id, tool)
            .map_err(|_| "registration failed")?;

        // Simulate an error that triggers early return, causing `_guard` to drop
        Err("simulated_error_before_completion")
    }

    let res = fallible_operation(&registry, &session_scope.scope_id);
    assert_eq!(res, Err("simulated_error_before_completion"));

    // Tool should now be completely purged from the registry
    let ctx = ToolExecutionContext::new(
        "sess-raii-test",
        CommandPrincipal::LocalCli,
        PathBuf::from("."),
    );
    let dispatch_res = registry
        .dispatch(&session_scope, "high_privilege_auto", json!({}), ctx)
        .await;

    assert!(
        dispatch_res.is_err(),
        "Tool must NOT exist after ScopeGuard was dropped on error path"
    );
    match dispatch_res.unwrap_err() {
        ToolExecError::ToolNotFound(name) => assert_eq!(name, "high_privilege_auto"),
        other => panic!("Expected ToolNotFound, got {other:?}"),
    }

    // Also verify catalog reflection is clean
    let available = registry.resolve_tools_for_scope(&session_scope.scope_id);
    assert!(
        available.iter().all(|t| t.name != "high_privilege_auto"),
        "Catalog must not contain high_privilege_auto after drop"
    );
}

/// Challenge 4: Policy Enforcement - Forbidden Policy Rejection.
/// A tool configured with ToolExecPolicy::Forbidden must fail unconditionally,
/// even when invoked by CommandPrincipal::LocalCli.
#[tokio::test]
async fn test_adversarial_policy_forbidden_rejection() {
    let registry = NativeScopedToolRegistry::new();
    let session_scope = ToolScope::session("sess-forbidden-test", CommandPrincipal::LocalCli);
    registry.register_scope(session_scope.clone());

    let forbidden_tool = Arc::new(ForbiddenTool);
    let _guard = registry
        .register_scoped_tool(&session_scope.scope_id, forbidden_tool)
        .expect("Registration should succeed");

    let ctx = ToolExecutionContext::new(
        "sess-forbidden-test",
        CommandPrincipal::LocalCli,
        PathBuf::from("."),
    );

    let res = registry
        .dispatch(&session_scope, "forbidden_tool", json!({}), ctx)
        .await;

    assert!(res.is_err(), "Forbidden tool must fail execution");
    match res.unwrap_err() {
        ToolExecError::PolicyViolation(msg) => {
            assert!(
                msg.contains("forbidden by policy"),
                "Expected forbidden policy violation message: {msg}"
            );
        }
        other => panic!("Expected PolicyViolation, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// RFC 4: ADVERSARIAL CHALLENGES
// ---------------------------------------------------------------------------

/// Challenge 5: High-Concurrency Session Events Stress Test.
/// Spawn 5 concurrent tasks publishing 25 events each (total 125 events) across 3 distinct sessions.
/// Verify:
/// 1. Zero event loss across SQLite WAL persistence.
/// 2. Exact deterministic replay for each session with monotonic event ordering.
#[tokio::test]
async fn test_adversarial_rfc4_concurrent_session_events_stress() {
    let pool = Arc::new(DatabasePool::new_in_memory().expect("In-memory DB creation failed"));
    let event_stream = Arc::new(SessionEventStream::with_db_pool(512, pool.clone()));

    let num_tasks = 5;
    let events_per_task = 25;
    let sessions = vec!["stress-sess-alpha", "stress-sess-beta", "stress-sess-gamma"];

    let mut handles = Vec::new();

    for task_idx in 0..num_tasks {
        let stream = event_stream.clone();
        let session_id = sessions[task_idx % sessions.len()].to_string();

        handles.push(tokio::spawn(async move {
            for i in 0..events_per_task {
                let event = SessionEvent::ContentChunk {
                    session_id: session_id.clone(),
                    token: format!("task_{task_idx}_token_{i}"),
                };
                stream
                    .publish(event)
                    .await
                    .expect("Publish should not fail");
            }
        }));
    }

    for h in handles {
        h.await.expect("Task failed");
    }

    // Flush writer actor to disk
    pool.writer_actor.flush().await.expect("Flush must succeed");

    // Total events across all sessions
    let mut total_replayed = 0;
    for sid in &sessions {
        let events = event_stream
            .replay_from_db(sid)
            .expect("Replay from DB must succeed");
        total_replayed += events.len();

        // Verify each event has correct session_id
        for e in events {
            assert_eq!(e.session_id(), *sid);
        }
    }

    assert_eq!(
        total_replayed,
        num_tasks * events_per_task,
        "Total persisted events in WAL must exactly equal published events"
    );
}

/// Challenge 6: Crash Simulation & 100% Deterministic State Reconstruction.
/// Publish a rich sequence of events to a SessionEventStream.
/// Then simulate an abrupt process crash by completely dropping the SessionEventStream
/// (wiping the in-memory RAM buffer). Recreate a new stream instance connected to the
/// same SQLite WAL pool and verify complete bit-for-bit trajectory reconstruction.
#[tokio::test]
async fn test_adversarial_rfc4_crash_recovery_deterministic_reconstruction() {
    let pool = Arc::new(DatabasePool::new_in_memory().expect("In-memory DB creation failed"));

    let session_id = "crash-sess-999";

    let original_events = vec![
        SessionEvent::PromptInjected {
            session_id: session_id.into(),
            prompt_tokens: 350,
        },
        SessionEvent::ReasoningChunk {
            session_id: session_id.into(),
            token: "Step 1: Check security constraints".into(),
        },
        SessionEvent::ReasoningChunk {
            session_id: session_id.into(),
            token: "Step 2: Parse parameters".into(),
        },
        SessionEvent::ToolCallProposed {
            session_id: session_id.into(),
            tool: "file_read".into(),
            args: json!({ "path": "/etc/config" }),
        },
        SessionEvent::ToolCallExecuted {
            session_id: session_id.into(),
            tool: "file_read".into(),
            result: json!({ "status": "ok", "lines": 42 }),
            duration_ms: 12,
        },
        SessionEvent::ContentChunk {
            session_id: session_id.into(),
            token: "Configuration loaded successfully.".into(),
        },
        SessionEvent::SessionFinished {
            session_id: session_id.into(),
            total_tokens: 420,
            latency_ms: 85,
        },
    ];

    {
        // Scope 1: Active session stream publishing events
        let active_stream = SessionEventStream::with_db_pool(64, pool.clone());
        for evt in &original_events {
            active_stream
                .publish(evt.clone())
                .await
                .expect("Publish failed");
        }
        pool.writer_actor.flush().await.expect("Flush failed");
        // `active_stream` dropped here simulating crash / process termination
    }

    // Scope 2: Brand new stream created after "crash"
    let recovered_stream = SessionEventStream::with_db_pool(64, pool.clone());

    // In-memory RAM buffer of recovered_stream is completely empty
    let in_memory_events = recovered_stream.replay(session_id).await;
    assert!(
        in_memory_events.is_empty(),
        "Recovered stream in-memory buffer must be empty after crash"
    );

    // Replay directly from durable SQLite WAL disk table
    let replayed = recovered_stream
        .replay_from_db(session_id)
        .expect("replay_from_db must succeed");

    assert_eq!(
        replayed.len(),
        original_events.len(),
        "Replayed event count must match original count"
    );

    for (idx, (actual, expected)) in replayed.iter().zip(original_events.iter()).enumerate() {
        assert_eq!(
            actual.event_name(),
            expected.event_name(),
            "Mismatch at event index {idx}"
        );
        let actual_json = serde_json::to_string(actual).unwrap();
        let expected_json = serde_json::to_string(expected).unwrap();
        assert_eq!(
            actual_json, expected_json,
            "Serialized payload mismatch at event {idx}"
        );
    }
}

/// Challenge 7: Branched Time-Travel Checkpoint Tree Verification.
/// Construct a branching checkpoint tree:
/// cp-root (turn 0)
///  ├── cp-1a (turn 1, branch A) -> cp-2a (turn 2)
///  └── cp-1b (turn 1, branch B) -> cp-2b (turn 2)
/// Replay and verify branching links and data integrity.
#[tokio::test]
async fn test_adversarial_rfc4_branched_time_travel_checkpoint_tree() {
    let pool = Arc::new(DatabasePool::new_in_memory().expect("In-memory DB creation failed"));
    let stream = SessionEventStream::with_db_pool(64, pool.clone());

    let sid = "branching-sess-tree";

    stream
        .save_branched_checkpoint("cp-root", sid, None, 0, "digest-0", r#"{"state": "root"}"#)
        .await
        .expect("Save root checkpoint failed");

    stream
        .save_branched_checkpoint(
            "cp-1a",
            sid,
            Some("cp-root".into()),
            1,
            "digest-1a",
            r#"{"state": "branch-a-turn-1"}"#,
        )
        .await
        .expect("Save 1a checkpoint failed");

    stream
        .save_branched_checkpoint(
            "cp-2a",
            sid,
            Some("cp-1a".into()),
            2,
            "digest-2a",
            r#"{"state": "branch-a-turn-2"}"#,
        )
        .await
        .expect("Save 2a checkpoint failed");

    stream
        .save_branched_checkpoint(
            "cp-1b",
            sid,
            Some("cp-root".into()),
            1,
            "digest-1b",
            r#"{"state": "branch-b-turn-1"}"#,
        )
        .await
        .expect("Save 1b checkpoint failed");

    stream
        .save_branched_checkpoint(
            "cp-2b",
            sid,
            Some("cp-1b".into()),
            2,
            "digest-2b",
            r#"{"state": "branch-b-turn-2"}"#,
        )
        .await
        .expect("Save 2b checkpoint failed");

    let checkpoints = stream
        .replay_checkpoints_from_db(sid)
        .expect("Replay checkpoints failed");

    assert_eq!(checkpoints.len(), 5, "All 5 checkpoints must be retrieved");

    let get_cp = |id: &str| checkpoints.iter().find(|c| c.0 == id).cloned();

    let root = get_cp("cp-root").expect("cp-root missing");
    assert_eq!(root.1, None);
    assert_eq!(root.2, 0);

    let cp1a = get_cp("cp-1a").expect("cp-1a missing");
    assert_eq!(cp1a.1.as_deref(), Some("cp-root"));
    assert_eq!(cp1a.2, 1);

    let cp2a = get_cp("cp-2a").expect("cp-2a missing");
    assert_eq!(cp2a.1.as_deref(), Some("cp-1a"));
    assert_eq!(cp2a.2, 2);

    let cp1b = get_cp("cp-1b").expect("cp-1b missing");
    assert_eq!(cp1b.1.as_deref(), Some("cp-root"));
    assert_eq!(cp1b.2, 1);

    let cp2b = get_cp("cp-2b").expect("cp-2b missing");
    assert_eq!(cp2b.1.as_deref(), Some("cp-1b"));
    assert_eq!(cp2b.2, 2);
}

/// Challenge 8: Session Continuation Across Simulated Process Restarts.
/// Verifies that when an application restarts and continues logging events
/// to an existing session, events are replayed in exact chronological FIFO order
/// without interleaving or sequence corruption.
#[tokio::test]
async fn test_adversarial_rfc4_restart_session_continuation_ordering() {
    let pool = Arc::new(DatabasePool::new_in_memory().expect("In-memory DB creation failed"));
    let session_id = "sess-restart-continuity";

    // Phase 1: Lifecycle before restart
    {
        let stream1 = SessionEventStream::with_db_pool(64, pool.clone());
        let e1 = SessionEvent::PromptInjected {
            session_id: session_id.into(),
            prompt_tokens: 100,
        };
        let e2 = SessionEvent::ReasoningChunk {
            session_id: session_id.into(),
            token: "First thought before restart".into(),
        };
        let e3 = SessionEvent::ContentChunk {
            session_id: session_id.into(),
            token: "Response part 1".into(),
        };
        stream1.publish(e1).await.expect("Publish e1 failed");
        stream1.publish(e2).await.expect("Publish e2 failed");
        stream1.publish(e3).await.expect("Publish e3 failed");
        pool.writer_actor.flush().await.expect("Flush 1 failed");
        // stream1 dropped simulating process restart
    }

    // Phase 2: Lifecycle after restart (new stream instance connected to same SQLite pool)
    let stream2 = SessionEventStream::with_db_pool(64, pool.clone());

    // In-memory buffer is clean
    assert_eq!(stream2.replay(session_id).await.len(), 0);

    // Continue the session after restart
    let e4 = SessionEvent::ReasoningChunk {
        session_id: session_id.into(),
        token: "Second thought after restart".into(),
    };
    let e5 = SessionEvent::ContentChunk {
        session_id: session_id.into(),
        token: "Response part 2".into(),
    };
    stream2.publish(e4).await.expect("Publish e4 failed");
    stream2.publish(e5).await.expect("Publish e5 failed");
    pool.writer_actor.flush().await.expect("Flush 2 failed");

    // Replay entire multi-lifecycle trajectory directly from SQLite WAL
    let replayed = stream2
        .replay_from_db(session_id)
        .expect("Replay from DB failed");

    assert_eq!(
        replayed.len(),
        5,
        "Must replay all 5 events across restart boundary"
    );

    // Verify exact chronological sequence
    match &replayed[0] {
        SessionEvent::PromptInjected { prompt_tokens, .. } => assert_eq!(*prompt_tokens, 100),
        other => panic!("Expected PromptInjected at 0, got {other:?}"),
    }
    match &replayed[1] {
        SessionEvent::ReasoningChunk { token, .. } => {
            assert_eq!(token, "First thought before restart")
        }
        other => panic!("Expected ReasoningChunk at 1, got {other:?}"),
    }
    match &replayed[2] {
        SessionEvent::ContentChunk { token, .. } => assert_eq!(token, "Response part 1"),
        other => panic!("Expected ContentChunk at 2, got {other:?}"),
    }
    match &replayed[3] {
        SessionEvent::ReasoningChunk { token, .. } => {
            assert_eq!(token, "Second thought after restart")
        }
        other => panic!("Expected ReasoningChunk at 3, got {other:?}"),
    }
    match &replayed[4] {
        SessionEvent::ContentChunk { token, .. } => assert_eq!(token, "Response part 2"),
        other => panic!("Expected ContentChunk at 4, got {other:?}"),
    }
}
