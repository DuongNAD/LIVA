//! Integration tests for Scoped Tool Registry, Guarded Execution Pipeline,
//! and Session Event Stream (Features F4, F5, F6).
//!
//! Authoritative references:
//! - PROJECT.md (Features F4, F5, F6)
//! - TEST_INFRA.md (Tier 1..3 test specifications)
//! - deepseek_harness_integration.md (RFC 1 & RFC 2 specifications)

use liva_native_core::CommandPrincipal;
use liva_native_core::cognitive::{
    ActionProposal, EventSensitivity, IdempotencyCheckResult, IdempotencyManager, PolicyDecision,
    PolicyEngine, RedactedAuditLedger, RiskTier, SessionEvent, SessionEventStream, ToolObservation,
};
use liva_native_core::db::DatabasePool;
use liva_native_core::llm::{CatalogTool, ScopedToolRegistry, ToolError, ToolExecError, ToolScope};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// TIER 1: FEATURE COVERAGE (F4: NATIVE SCOPED TOOL REGISTRY)
// ---------------------------------------------------------------------------

#[test]
fn test_root_scope_registration() {
    let registry = ScopedToolRegistry::new();
    let root_scope = ToolScope::new("scope:root", CommandPrincipal::LocalCli);
    registry.register_scope(root_scope);

    let tool = CatalogTool {
        server: "native".into(),
        name: "search_vault".into(),
        description: "Search notes in vault".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            }
        }),
        embed_extra: "".into(),
    };

    let _guard = registry
        .register_scoped("scope:root", tool)
        .expect("registered");
    let tools = registry.resolve_tools_for_scope("scope:root");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "search_vault");
}

#[test]
fn test_child_scope_inheritance() {
    let registry = ScopedToolRegistry::new();
    let root = ToolScope::new("scope:root", CommandPrincipal::LocalCli);
    let child = ToolScope::new("scope:session_1", CommandPrincipal::TauriDashboard)
        .with_parent("scope:root");

    registry.register_scope(root);
    registry.register_scope(child);

    let root_tool = CatalogTool {
        server: "native".into(),
        name: "get_system_status".into(),
        description: "Get system stats".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let ephemeral_tool = CatalogTool {
        server: "session".into(),
        name: "session_scratchpad".into(),
        description: "Ephemeral scratchpad".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let _g1 = registry.register_scoped("scope:root", root_tool).unwrap();
    let _g2 = registry
        .register_scoped("scope:session_1", ephemeral_tool)
        .unwrap();

    let child_tools = registry.resolve_tools_for_scope("scope:session_1");
    assert_eq!(child_tools.len(), 2);
    let names: HashSet<_> = child_tools.into_iter().map(|t| t.name).collect();
    assert!(names.contains("get_system_status"));
    assert!(names.contains("session_scratchpad"));
}

#[test]
fn test_peer_scope_isolation() {
    let registry = ScopedToolRegistry::new();
    let scope_a = ToolScope::new("scope:A", CommandPrincipal::TauriWidget);
    let scope_b = ToolScope::new("scope:B", CommandPrincipal::TauriWidget);

    registry.register_scope(scope_a);
    registry.register_scope(scope_b);

    let tool_a = CatalogTool {
        server: "native".into(),
        name: "tool_a".into(),
        description: "Tool for scope A".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let tool_b = CatalogTool {
        server: "native".into(),
        name: "tool_b".into(),
        description: "Tool for scope B".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let _g1 = registry.register_scoped("scope:A", tool_a).unwrap();
    let _g2 = registry.register_scoped("scope:B", tool_b).unwrap();

    let tools_a = registry.resolve_tools_for_scope("scope:A");
    let tools_b = registry.resolve_tools_for_scope("scope:B");

    assert_eq!(tools_a.len(), 1);
    assert_eq!(tools_a[0].name, "tool_a");

    assert_eq!(tools_b.len(), 1);
    assert_eq!(tools_b[0].name, "tool_b");
}

#[test]
fn test_raii_scope_guard_disposal() {
    let registry = ScopedToolRegistry::new();
    let scope = ToolScope::new("scope:temp", CommandPrincipal::LocalCli);
    registry.register_scope(scope);

    let tool = CatalogTool {
        server: "native".into(),
        name: "temporary_calculator".into(),
        description: "Temp calculator".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    {
        let guard = registry.register_scoped("scope:temp", tool).unwrap();
        let tools = registry.resolve_tools_for_scope("scope:temp");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "temporary_calculator");
        drop(guard); // Explicit fiber disposal
    }

    let tools_after = registry.resolve_tools_for_scope("scope:temp");
    assert!(
        tools_after.is_empty(),
        "Tool should be unregistered after RAII drop"
    );
}

#[test]
fn test_scoped_tool_selection_top_k() {
    let registry = ScopedToolRegistry::new();
    let scope = ToolScope::new("scope:calc_session", CommandPrincipal::TauriDashboard)
        .allow_tool("calculator")
        .allow_tool("search_vault");
    registry.register_scope(scope.clone());

    let calc_tool = CatalogTool {
        server: "native".into(),
        name: "calculator".into(),
        description: "Thực hiện phép tính toán số học".into(),
        input_schema: json!({
            "type": "object",
            "required": ["expr"],
            "properties": { "expr": { "type": "string" } }
        }),
        embed_extra: "tính toán cộng trừ nhân chia".into(),
    };

    let search_tool = CatalogTool {
        server: "native".into(),
        name: "search_vault".into(),
        description: "Tìm kiếm ghi chú trong vault".into(),
        input_schema: json!({
            "type": "object",
            "required": ["query"],
            "properties": { "query": { "type": "string" } }
        }),
        embed_extra: "".into(),
    };

    let weather_tool = CatalogTool {
        server: "native".into(),
        name: "get_weather".into(),
        description: "Xem dự báo thời tiết".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let _g1 = registry
        .register_scoped("scope:calc_session", calc_tool)
        .unwrap();
    let _g2 = registry
        .register_scoped("scope:calc_session", search_tool)
        .unwrap();
    let _g3 = registry
        .register_scoped("scope:calc_session", weather_tool)
        .unwrap();

    let selected = registry.select_tools_for_scope(&scope, "tính toán", 2);
    assert!(selected.len() <= 2);
    assert!(!selected.is_empty());
    // get_weather is not in allowed_tools so it should never be returned
    assert!(selected.iter().all(|t| t.name != "get_weather"));
}

// ---------------------------------------------------------------------------
// TIER 1: FEATURE COVERAGE (F5: GUARDED TOOL EXECUTION PIPELINE)
// ---------------------------------------------------------------------------

#[test]
fn test_principal_allowed_tool_exec() {
    let registry = ScopedToolRegistry::new();
    let scope =
        ToolScope::new("scope:dash", CommandPrincipal::TauriDashboard).allow_tool("search_vault");
    registry.register_scope(scope.clone());

    let tool = CatalogTool {
        server: "native".into(),
        name: "search_vault".into(),
        description: "Search notes".into(),
        input_schema: json!({
            "type": "object",
            "required": ["query"],
            "properties": { "query": { "type": "string" } }
        }),
        embed_extra: "".into(),
    };

    let _g = registry.register_scoped("scope:dash", tool).unwrap();

    let res = registry.execute_guarded(&scope, "search_vault", &json!({"query": "rust"}));
    assert!(res.is_ok());
    let val = res.unwrap();
    assert_eq!(val["status"], "executed");
    assert_eq!(val["tool"], "search_vault");
}

#[test]
fn test_principal_unauthorized_tool_exec() {
    let registry = ScopedToolRegistry::new();
    let scope = ToolScope::new("scope:telegram", CommandPrincipal::Telegram).allow_tool("ping");
    registry.register_scope(scope.clone());

    let tool = CatalogTool {
        server: "native".into(),
        name: "update_config".into(),
        description: "Admin update".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let _g = registry.register_scoped("scope:telegram", tool).unwrap();

    let res = registry.execute_guarded(&scope, "update_config", &json!({}));
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("not authorized"));
}

#[test]
fn test_risk_tier_confirmation_gating() {
    let proposal = ActionProposal::new(
        "delete_all_data",
        "native/delete_all_data",
        json!({"all": true}),
        RiskTier::PhysicalOrIrreversible,
        "User asked to clear everything",
    );

    let decision: PolicyDecision = PolicyEngine::evaluate_proposal(&proposal);
    assert!(decision.requires_hitl);
    assert_eq!(decision.risk_tier, RiskTier::PhysicalOrIrreversible);
    assert!(decision.confirmation_token.is_some());
}

#[test]
fn test_action_audit_ledger_recording() {
    let pool = DatabasePool::new_in_memory().expect("in-memory database");
    let conn = pool.writer.get().expect("db conn");

    let registry = ScopedToolRegistry::new();
    let scope =
        ToolScope::new("scope:audit_test", CommandPrincipal::LocalCli).allow_tool("read_markdown");
    registry.register_scope(scope.clone());

    let tool = CatalogTool {
        server: "native".into(),
        name: "read_markdown".into(),
        description: "Read note".into(),
        input_schema: json!({
            "type": "object",
            "required": ["path"],
            "properties": { "path": { "type": "string" } }
        }),
        embed_extra: "".into(),
    };

    let _g = registry.register_scoped("scope:audit_test", tool).unwrap();

    let res = registry.execute_guarded_with_audit(
        Some(&conn),
        &scope,
        "read_markdown",
        &json!({"path": "secret_vault/token.md", "secret": "sk-test-secret"}),
    );
    assert!(res.is_ok());

    let recent = RedactedAuditLedger::query_recent(&conn, 10).expect("query audit");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].tool_id, "native/read_markdown");
    assert_eq!(recent[0].status, "success");
    // Secrets should be scrubbed in audit records
    assert!(!recent[0].redacted_params.contains("sk-test-secret"));
}

#[test]
fn test_tool_call_idempotency() {
    let pool = DatabasePool::new_in_memory().expect("in-memory database");
    let conn = pool.writer.get().expect("db conn");

    let manager = IdempotencyManager::new();
    let key = "idemp_test_turn_001";
    let action_id = "act_123";
    let tool_id = "native/read_markdown";
    let ttl_ms = 60_000;

    let state = manager
        .check_or_start(key, action_id, tool_id, ttl_ms, Some(&conn))
        .unwrap();
    assert_eq!(state, IdempotencyCheckResult::New);

    let state_in_progress = manager
        .check_or_start(key, action_id, tool_id, ttl_ms, Some(&conn))
        .unwrap();
    assert_eq!(state_in_progress, IdempotencyCheckResult::InProgress);

    let obs = ToolObservation::success(action_id, tool_id, "done", 50);
    manager.complete(key, &obs, Some(&conn)).unwrap();

    let state_completed = manager
        .check_or_start(key, action_id, tool_id, ttl_ms, Some(&conn))
        .unwrap();
    match state_completed {
        IdempotencyCheckResult::Completed(cached) => {
            let cached_obs = cached.expect("cached observation");
            assert!(cached_obs.success);
            assert_eq!(cached_obs.output_sanitized, "done");
        }
        _ => panic!("Expected completed idempotency state"),
    }
}

// ---------------------------------------------------------------------------
// TIER 1: FEATURE COVERAGE (F6: SESSION EVENT STREAM)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_session_event_broadcast() {
    let stream = SessionEventStream::new(32);
    let mut rx1 = stream.subscribe();
    let mut rx2 = stream.subscribe();

    let event1 = SessionEvent::prompt_injected("session_1", 150);
    let event2 = SessionEvent::reasoning_chunk("session_1", "Suy nghĩ...");

    stream.publish(event1.clone()).await.unwrap();
    stream.publish(event2.clone()).await.unwrap();

    let recv1_1 = rx1.recv().await.unwrap();
    let recv1_2 = rx1.recv().await.unwrap();
    let recv2_1 = rx2.recv().await.unwrap();
    let recv2_2 = rx2.recv().await.unwrap();

    assert_eq!(recv1_1, event1);
    assert_eq!(recv1_2, event2);
    assert_eq!(recv2_1, event1);
    assert_eq!(recv2_2, event2);
}

#[tokio::test]
async fn test_session_event_replay() {
    let stream = SessionEventStream::new(64);

    for i in 0..10 {
        let evt = SessionEvent::reasoning_chunk("session_replay", format!("chunk_{i}"));
        stream.publish(evt).await.unwrap();
    }

    let replayed = stream.replay("session_replay").await;
    assert_eq!(replayed.len(), 10);
    for (i, evt) in replayed.iter().enumerate() {
        if let SessionEvent::ReasoningChunk { token, .. } = evt {
            assert_eq!(token, &format!("chunk_{i}"));
        } else {
            panic!("Unexpected event type");
        }
    }
}

#[tokio::test]
async fn test_concurrent_session_subscribers() {
    let stream = Arc::new(SessionEventStream::new(128));
    let mut handles = Vec::new();

    for _ in 0..5 {
        let mut rx = stream.subscribe();
        let handle = tokio::spawn(async move {
            let mut count = 0;
            while let Ok(_evt) = rx.recv().await {
                count += 1;
                if count == 20 {
                    break;
                }
            }
            count
        });
        handles.push(handle);
    }

    for i in 0..20 {
        let evt = SessionEvent::content_chunk("session_concurrent", format!("word_{i}"));
        stream.publish(evt).await.unwrap();
    }

    for handle in handles {
        let count = handle.await.unwrap();
        assert_eq!(count, 20);
    }
}

#[test]
fn test_event_stream_redaction() {
    let secret_evt = SessionEvent::custom(
        "session_secret",
        "api_key_configured",
        json!({ "token": "sk-1234567890abcdef1234567890abcdef" }),
        EventSensitivity::Secret,
    );

    let redacted = secret_evt.redact();
    if let SessionEvent::Custom { data, .. } = redacted {
        assert!(data["token"].as_str().unwrap().contains("[REDACTED"));
    } else {
        panic!("Expected custom event");
    }
}

#[test]
fn test_session_finished_event() {
    let evt = SessionEvent::session_finished("session_fin", 520, 1420);
    assert_eq!(evt.session_id(), "session_fin");

    let serialized = serde_json::to_string(&evt).unwrap();
    let deserialized: SessionEvent = serde_json::from_str(&serialized).unwrap();
    assert_eq!(evt, deserialized);
}

// ---------------------------------------------------------------------------
// TIER 2: BOUNDARY & CORNER CASES (F4, F5, F6)
// ---------------------------------------------------------------------------

#[test]
fn test_empty_scope_boundary() {
    let registry = ScopedToolRegistry::new();
    let scope = ToolScope::new("scope:empty", CommandPrincipal::TauriWidget);
    registry.register_scope(scope.clone());

    let tools = registry.resolve_tools_for_scope("scope:empty");
    assert!(tools.is_empty());

    let res = registry.execute_guarded(&scope, "non_existent_tool", &json!({}));
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err(),
        ToolExecError::ToolNotFound("non_existent_tool".to_string())
    );
}

#[test]
fn test_duplicate_tool_registration() {
    let registry = ScopedToolRegistry::new();
    let scope = ToolScope::new("scope:dupe", CommandPrincipal::LocalCli);
    registry.register_scope(scope);

    let tool1 = CatalogTool {
        server: "native".into(),
        name: "duplicate_name".into(),
        description: "Version 1".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let tool2 = CatalogTool {
        server: "native".into(),
        name: "duplicate_name".into(),
        description: "Version 2".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let _g1 = registry
        .register_scoped("scope:dupe", tool1)
        .expect("first succeeds");
    let res2 = registry.register_scoped("scope:dupe", tool2);
    assert!(res2.is_err());
    assert_eq!(
        res2.unwrap_err(),
        ToolError::DuplicateTool("duplicate_name".to_string(), "scope:dupe".to_string())
    );
}

#[test]
fn test_deep_scope_recursion() {
    let registry = ScopedToolRegistry::new();
    let depth = 32;

    for i in 0..depth {
        let scope_id = format!("scope:level_{i}");
        let parent = if i > 0 {
            Some(format!("scope:level_{}", i - 1))
        } else {
            None
        };

        let mut scope = ToolScope::new(&scope_id, CommandPrincipal::LocalCli);
        scope.parent_scope = parent;
        registry.register_scope(scope);
    }

    let root_tool = CatalogTool {
        server: "native".into(),
        name: "deep_root_tool".into(),
        description: "Tool defined at root".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let _g = registry
        .register_scoped("scope:level_0", root_tool)
        .unwrap();

    let deepest_scope = format!("scope:level_{}", depth - 1);
    let resolved = registry.resolve_tools_for_scope(&deepest_scope);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].name, "deep_root_tool");
}

#[test]
fn test_malformed_json_tool_execution() {
    let registry = ScopedToolRegistry::new();
    let scope =
        ToolScope::new("scope:validation", CommandPrincipal::LocalCli).allow_tool("read_markdown");
    registry.register_scope(scope.clone());

    let tool = CatalogTool {
        server: "native".into(),
        name: "read_markdown".into(),
        description: "Read note".into(),
        input_schema: json!({
            "type": "object",
            "required": ["path"],
            "properties": { "path": { "type": "string" } }
        }),
        embed_extra: "".into(),
    };

    let _g = registry.register_scoped("scope:validation", tool).unwrap();

    // 1. Missing required field
    let res1 = registry.execute_guarded(&scope, "read_markdown", &json!({}));
    assert!(res1.is_err());
    assert!(matches!(
        res1.unwrap_err(),
        ToolExecError::InvalidArguments(_)
    ));

    // 2. Wrong type for required field
    let res2 = registry.execute_guarded(&scope, "read_markdown", &json!({"path": 12345}));
    assert!(res2.is_err());
    assert!(matches!(
        res2.unwrap_err(),
        ToolExecError::InvalidArguments(_)
    ));
}

#[tokio::test]
async fn test_broadcast_backpressure_lag() {
    let stream = SessionEventStream::new(16);
    let mut rx = stream.subscribe();

    for i in 0..100 {
        let evt = SessionEvent::content_chunk("lag_test", format!("token_{i}"));
        let _ = stream.publish(evt).await;
    }

    // Since channel capacity is 16 and we sent 100, receiver should report Lagged
    let recv_res = rx.recv().await;
    assert!(recv_res.is_err());
    assert!(matches!(
        recv_res.unwrap_err(),
        broadcast::error::RecvError::Lagged(_)
    ));
}

// ---------------------------------------------------------------------------
// TIER 3: CROSS-FEATURE INTEGRATION
// ---------------------------------------------------------------------------

#[test]
fn test_guarded_tool_pipeline_policy_and_audit() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let conn = pool.writer.get().expect("db conn");

    let registry = ScopedToolRegistry::new();
    let scope =
        ToolScope::new("scope:t3", CommandPrincipal::TauriDashboard).allow_tool("control_volume");
    registry.register_scope(scope.clone());

    let tool = CatalogTool {
        server: "native".into(),
        name: "control_volume".into(),
        description: "Điều chỉnh âm lượng".into(),
        input_schema: json!({
            "type": "object",
            "required": ["level"],
            "properties": { "level": { "type": "integer" } }
        }),
        embed_extra: "âm lượng".into(),
    };

    let _g = registry.register_scoped("scope:t3", tool).unwrap();

    let res = registry.execute_guarded_with_audit(
        Some(&conn),
        &scope,
        "control_volume",
        &json!({"level": 50}),
    );
    assert!(res.is_ok());

    let recent = RedactedAuditLedger::query_recent(&conn, 5).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].risk_tier, "reversible");
}

#[test]
fn test_raii_scope_drop_lifecycle() {
    let registry = ScopedToolRegistry::new();
    let root = ToolScope::new("scope:root", CommandPrincipal::LocalCli);
    let task_scope =
        ToolScope::new("scope:task_42", CommandPrincipal::TauriDashboard).with_parent("scope:root");

    registry.register_scope(root);
    registry.register_scope(task_scope);

    let t1 = CatalogTool {
        server: "native".into(),
        name: "persistent_tool".into(),
        description: "Always available".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let t2 = CatalogTool {
        server: "ephemeral".into(),
        name: "task_exclusive_tool".into(),
        description: "Only during task".into(),
        input_schema: json!({}),
        embed_extra: "".into(),
    };

    let _g_root = registry.register_scoped("scope:root", t1).unwrap();

    {
        let guard = registry.register_scoped("scope:task_42", t2).unwrap();
        let tools = registry.resolve_tools_for_scope("scope:task_42");
        assert_eq!(tools.len(), 2);
        drop(guard);
    }

    let tools_after = registry.resolve_tools_for_scope("scope:task_42");
    assert_eq!(tools_after.len(), 1);
    assert_eq!(tools_after[0].name, "persistent_tool");
}
