//! Integration tests for SOTA Agentic Harness RFCs (RFC 1 - RFC 4).
//!
//! Authoritative references:
//! - RFC 1: Scoped Tool Registry & Guarded Execution (NativeScopedToolRegistry, ToolScope, ToolExecPolicy, ToolExecutionContext, ScopedTool async trait)
//! - RFC 2: Real-Time Dual-Channel CoT Streaming (ReasoningStreamSplitter, isThought: true IPC event streaming, UTF-8 boundary safety)
//! - RFC 3: Priority-Sliced Dynamic Prompt Assembly (DynamicPromptAssembler with P0-P4 priority slices wired into handle_chat_completion_scoped)
//! - RFC 4: Append-Only Session Event Stream with SQLite WAL Persistence (session_events table, DbWriteCommand::AppendSessionEvent, deterministic replay)

use liva_native_core::CommandPrincipal;
use liva_native_core::cognitive::{SessionEvent, SessionEventStream};
use liva_native_core::db::DatabasePool;
use liva_native_core::llm::prompt::dynamic_prompt::{ContextTokenBudget, PromptSelection};
use liva_native_core::llm::{
    CallToolResult, ChatMessage, DynamicPromptAssembler, NativeScopedToolRegistry, PromptSlice,
    ReasoningStreamSplitter, ScopedTool, SlicePriority, StreamChunk, Tool, ToolContent,
    ToolExecError, ToolExecPolicy, ToolExecutionContext, ToolScope, async_trait, compile_prompt,
};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// RFC 1: SCOPED TOOL REGISTRY & GUARDED EXECUTION TESTS
// ---------------------------------------------------------------------------

#[derive(JsonSchema, Deserialize)]
struct MathMultiplyArgs {
    a: f64,
    b: f64,
}

struct MathMultiplyTool;

#[async_trait]
impl ScopedTool for MathMultiplyTool {
    fn definition(&self) -> Tool {
        Tool {
            name: "math_multiply".into(),
            description: "Multiply two numbers".into(),
            input_schema: schema_for!(MathMultiplyArgs),
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
        arguments: Value,
        _context: ToolExecutionContext,
    ) -> Result<CallToolResult, String> {
        let args: MathMultiplyArgs =
            serde_json::from_value(arguments).map_err(|e| e.to_string())?;
        let result = args.a * args.b;
        Ok(CallToolResult::success(format!("{result}")))
    }
}

#[derive(JsonSchema, Deserialize)]
struct SystemRebootArgs {
    #[allow(dead_code)]
    force: Option<bool>,
}

struct ProposeOnlyRebootTool;

#[async_trait]
impl ScopedTool for ProposeOnlyRebootTool {
    fn definition(&self) -> Tool {
        Tool {
            name: "system_reboot".into(),
            description: "Reboot system (destructive)".into(),
            input_schema: schema_for!(SystemRebootArgs),
        }
    }

    fn default_policy(&self) -> ToolExecPolicy {
        ToolExecPolicy::ProposeOnly
    }

    fn required_principal(&self) -> CommandPrincipal {
        CommandPrincipal::TauriDashboard
    }

    async fn execute(
        &self,
        _arguments: Value,
        _context: ToolExecutionContext,
    ) -> Result<CallToolResult, String> {
        Ok(CallToolResult::success("Reboot scheduled"))
    }
}

#[tokio::test]
async fn test_rfc1_scoped_tool_registry_and_raii_guard() {
    let registry = NativeScopedToolRegistry::new();
    let session_scope = ToolScope::session("sess-100", CommandPrincipal::LocalCli);
    registry.register_scope(session_scope.clone());

    let multiply_tool = Arc::new(MathMultiplyTool);
    let guard = registry
        .register_scoped_tool(&session_scope.scope_id, multiply_tool)
        .expect("Tool registration failed");

    let context =
        ToolExecutionContext::new("sess-100", CommandPrincipal::LocalCli, PathBuf::from("."));

    // Dispatch valid execution
    let result = registry
        .dispatch(
            &session_scope,
            "math_multiply",
            json!({ "a": 6.0, "b": 7.0 }),
            context.clone(),
        )
        .await
        .expect("Dispatch failed");

    assert_eq!(
        result.content,
        vec![ToolContent::Text { text: "42".into() }]
    );
    assert!(!result.is_error);

    // Drop guard and verify tool is removed
    drop(guard);

    let after_drop = registry
        .dispatch(
            &session_scope,
            "math_multiply",
            json!({ "a": 6.0, "b": 7.0 }),
            context,
        )
        .await;

    assert!(after_drop.is_err());
    match after_drop.unwrap_err() {
        ToolExecError::ToolNotFound(name) => assert_eq!(name, "math_multiply"),
        other => panic!("Expected ToolNotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn test_rfc1_policy_enforcement_propose_only() {
    let registry = NativeScopedToolRegistry::new();
    let session_scope = ToolScope::session("sess-101", CommandPrincipal::LocalCli);
    registry.register_scope(session_scope.clone());

    let reboot_tool = Arc::new(ProposeOnlyRebootTool);
    let _guard = registry
        .register_scoped_tool(&session_scope.scope_id, reboot_tool)
        .expect("Tool registration failed");

    let context = ToolExecutionContext::new(
        "sess-101",
        CommandPrincipal::TauriDashboard,
        PathBuf::from("."),
    );

    let result = registry
        .dispatch(
            &session_scope,
            "system_reboot",
            json!({ "force": true }),
            context,
        )
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ToolExecError::ConfirmationRequired { risk, .. } => {
            assert!(risk.contains("ProposeOnly"));
        }
        other => panic!("Expected ConfirmationRequired, got {other:?}"),
    }
}

struct ReadOnlyStatusTool;

#[async_trait]
impl ScopedTool for ReadOnlyStatusTool {
    fn definition(&self) -> Tool {
        Tool {
            name: "read_status".into(),
            description: "Read system status (requires TauriWidget)".into(),
            input_schema: schema_for!(serde_json::Value),
        }
    }

    fn default_policy(&self) -> ToolExecPolicy {
        ToolExecPolicy::Auto
    }

    fn required_principal(&self) -> CommandPrincipal {
        CommandPrincipal::TauriWidget
    }

    async fn execute(
        &self,
        _arguments: Value,
        _context: ToolExecutionContext,
    ) -> Result<CallToolResult, String> {
        Ok(CallToolResult::success("status_ok"))
    }
}

/// Negative Test: Exhaustive rejection matrix verifying that lower-privilege callers
/// (Telegram, TauriSetup, TauriWidget, TauriDashboard) are rejected when attempting
/// to execute a LocalCli-required tool, while Test and LocalCli succeed.
#[tokio::test]
async fn test_rfc1_principal_authorization_insufficient_privilege_rejected() {
    let registry = NativeScopedToolRegistry::new();
    let session_scope = ToolScope::session("sess-auth-matrix", CommandPrincipal::LocalCli);
    registry.register_scope(session_scope.clone());

    let multiply_tool = Arc::new(MathMultiplyTool);
    let _guard = registry
        .register_scoped_tool(&session_scope.scope_id, multiply_tool)
        .expect("Registration must succeed");

    let unauthorized_principals = [
        (CommandPrincipal::Telegram, "Telegram"),
        (CommandPrincipal::TauriSetup, "TauriSetup"),
        (CommandPrincipal::TauriWidget, "TauriWidget"),
        (CommandPrincipal::TauriDashboard, "TauriDashboard"),
    ];

    for (principal, name) in unauthorized_principals {
        let ctx = ToolExecutionContext::new("sess-auth-matrix", principal, PathBuf::from("."));
        let result = registry
            .dispatch(
                &session_scope,
                "math_multiply",
                json!({ "a": 3.0, "b": 4.0 }),
                ctx,
            )
            .await;

        assert!(
            result.is_err(),
            "Principal {name} MUST be rejected when invoking LocalCli-required tool"
        );
        match result.unwrap_err() {
            ToolExecError::UnauthorizedPrincipal(msg) => {
                assert!(
                    msg.contains(name),
                    "Error message must specify rejected caller '{name}': {msg}"
                );
                assert!(
                    msg.contains("LocalCli"),
                    "Error message must specify required level 'LocalCli': {msg}"
                );
            }
            other => panic!("Expected UnauthorizedPrincipal for {name}, got {other:?}"),
        }
    }

    // Authorized callers: Test and LocalCli
    let authorized_principals = [CommandPrincipal::Test, CommandPrincipal::LocalCli];
    for principal in authorized_principals {
        let ctx = ToolExecutionContext::new("sess-auth-matrix", principal, PathBuf::from("."));
        let result = registry
            .dispatch(
                &session_scope,
                "math_multiply",
                json!({ "a": 3.0, "b": 4.0 }),
                ctx,
            )
            .await;

        assert!(
            result.is_ok(),
            "Principal {principal:?} MUST be authorized to execute LocalCli-level tool"
        );
        let content = result.unwrap().content;
        assert_eq!(content, vec![ToolContent::Text { text: "12".into() }]);
    }
}

/// Tiered Boundary Test: A tool requiring intermediate privilege (TauriWidget = 3)
/// must reject Telegram (1) and TauriSetup (2), but allow TauriWidget (3),
/// TauriDashboard (4), Test (5), and LocalCli (5).
#[tokio::test]
async fn test_rfc1_principal_authorization_tiered_boundaries() {
    let registry = NativeScopedToolRegistry::new();
    let session_scope = ToolScope::session("sess-tiered-test", CommandPrincipal::LocalCli);
    registry.register_scope(session_scope.clone());

    let status_tool = Arc::new(ReadOnlyStatusTool);
    let _guard = registry
        .register_scoped_tool(&session_scope.scope_id, status_tool)
        .expect("Registration must succeed");

    // Rejected tiers (level < 3)
    let rejected = [CommandPrincipal::Telegram, CommandPrincipal::TauriSetup];
    for principal in rejected {
        let ctx = ToolExecutionContext::new("sess-tiered-test", principal, PathBuf::from("."));
        let res = registry
            .dispatch(&session_scope, "read_status", json!({}), ctx)
            .await;
        assert!(
            res.is_err(),
            "Principal {principal:?} (tier < 3) must be rejected"
        );
        assert!(matches!(
            res.unwrap_err(),
            ToolExecError::UnauthorizedPrincipal(_)
        ));
    }

    // Allowed tiers (level >= 3)
    let allowed = [
        CommandPrincipal::TauriWidget,
        CommandPrincipal::TauriDashboard,
        CommandPrincipal::Test,
        CommandPrincipal::LocalCli,
    ];
    for principal in allowed {
        let ctx = ToolExecutionContext::new("sess-tiered-test", principal, PathBuf::from("."));
        let res = registry
            .dispatch(&session_scope, "read_status", json!({}), ctx)
            .await;
        assert!(
            res.is_ok(),
            "Principal {principal:?} (tier >= 3) must be authorized"
        );
        assert_eq!(
            res.unwrap().content,
            vec![ToolContent::Text {
                text: "status_ok".into()
            }]
        );
    }
}

/// Hierarchical Scope Inheritance Test: Verify that a session scope correctly
/// inherits tools registered at global scope via parent_scope linkage.
#[tokio::test]
async fn test_rfc1_hierarchical_session_global_scope_inheritance() {
    let registry = NativeScopedToolRegistry::new();

    let global_scope = ToolScope::global(CommandPrincipal::LocalCli);
    registry.register_scope(global_scope.clone());

    let session_scope = ToolScope::session("sess-inherit-verify", CommandPrincipal::LocalCli);
    registry.register_scope(session_scope.clone());

    // Register tool at global scope
    let tool = Arc::new(MathMultiplyTool);
    let _guard = registry
        .register_scoped_tool(&global_scope.scope_id, tool)
        .expect("Global tool registration succeeded");

    // Dispatch from session scope: must resolve via parent scope hierarchy
    let ctx = ToolExecutionContext::new(
        "sess-inherit-verify",
        CommandPrincipal::LocalCli,
        PathBuf::from("."),
    );
    let res = registry
        .dispatch(
            &session_scope,
            "math_multiply",
            json!({ "a": 5.0, "b": 5.0 }),
            ctx,
        )
        .await;

    assert!(
        res.is_ok(),
        "Session scope must inherit and execute tools registered in global scope"
    );
    assert_eq!(
        res.unwrap().content,
        vec![ToolContent::Text { text: "25".into() }]
    );
}

// ---------------------------------------------------------------------------
// RFC 2: REAL-TIME DUAL-CHANNEL COT STREAMING TESTS
// ---------------------------------------------------------------------------

#[test]
fn test_rfc2_dual_channel_demuxing_with_utf8_split_safety() {
    let mut splitter = ReasoningStreamSplitter::new();

    // Streaming tokens splitting a Vietnamese multibyte character across chunks
    let vietnamese_bytes = "Kế hoạch phân tích: ".as_bytes();
    let split_pos = 2; // splits inside UTF-8 multibyte boundary

    let chunk1 = splitter.process_bytes(&vietnamese_bytes[..split_pos]);
    let chunk2 = splitter.process_bytes(&vietnamese_bytes[split_pos..]);

    let mut combined_visible = String::new();
    for c in chunk1.into_iter().chain(chunk2) {
        if let StreamChunk::VisibleText(text) = c {
            combined_visible.push_str(&text);
        }
    }
    assert_eq!(combined_visible, "Kế hoạch phân tích: ");

    // Demux thinking channel
    let mut thought_splitter = ReasoningStreamSplitter::new();
    let tokens = [
        "Trước hết, ",
        "<think>",
        "bước 1: tính tích ",
        "6 x 7 = 42",
        "</think>",
        "kết quả là 42.",
    ];

    let mut reasoning_accum = String::new();
    let mut visible_accum = String::new();

    for token in tokens {
        for chunk in thought_splitter.process_token(token) {
            match chunk {
                StreamChunk::Reasoning(thought) => reasoning_accum.push_str(&thought),
                StreamChunk::VisibleText(visible) => visible_accum.push_str(&visible),
                StreamChunk::Heartbeat => {}
            }
        }
    }
    for chunk in thought_splitter.flush() {
        match chunk {
            StreamChunk::Reasoning(thought) => reasoning_accum.push_str(&thought),
            StreamChunk::VisibleText(visible) => visible_accum.push_str(&visible),
            StreamChunk::Heartbeat => {}
        }
    }

    assert_eq!(reasoning_accum, "bước 1: tính tích 6 x 7 = 42");
    assert_eq!(visible_accum, "Trước hết, kết quả là 42.");
}

// ---------------------------------------------------------------------------
// RFC 3: PRIORITY-SLICED DYNAMIC PROMPT ASSEMBLY TESTS
// ---------------------------------------------------------------------------

#[test]
fn test_rfc3_priority_sliced_assembly_and_budget_eviction() {
    let mut slices = Vec::new();
    let mut selection = PromptSelection::default();

    // P0: Core Persona (Mandatory)
    slices.push(PromptSlice::new(
        "p0_persona",
        SlicePriority::P0_SystemCore,
        0,
        ChatMessage {
            role: "system".into(),
            content: "You are LIVA AI Assistant.".into(),
        },
    ));
    selection.mandatory_ids.push("p0_persona".into());

    // P1: Active Tool Schemas
    slices.push(PromptSlice::new(
        "p1_tools",
        SlicePriority::P1_BaseCapabilities,
        1,
        ChatMessage {
            role: "system".into(),
            content: "Available Tools: search_vault(query*), math_multiply(a*, b*)".into(),
        },
    ));

    // P3: Recalled Long-Term Facts
    slices.push(PromptSlice::new(
        "p3_facts",
        SlicePriority::P3_DomainSkills,
        2,
        ChatMessage {
            role: "system".into(),
            content: "User prefers concise answers in Vietnamese.".into(),
        },
    ));

    // P4: Old History Turn (Grouped atomically)
    slices.push(PromptSlice::new(
        "turn_0_user",
        SlicePriority::P4_DynamicContext,
        3,
        ChatMessage {
            role: "user".into(),
            content: "Tell me a very long story about ancient algorithms...".into(),
        },
    ));
    slices.push(PromptSlice::new(
        "turn_0_assistant",
        SlicePriority::P4_DynamicContext,
        4,
        ChatMessage {
            role: "assistant".into(),
            content: "Once upon a time in ancient Alexandria...".into(),
        },
    ));
    selection
        .atomic_groups
        .push(vec!["turn_0_user".into(), "turn_0_assistant".into()]);

    // P2: Current User Turn (Mandatory)
    slices.push(PromptSlice::new(
        "turn_1_user",
        SlicePriority::P0_SystemCore,
        5,
        ChatMessage {
            role: "user".into(),
            content: "What is 6 * 7?".into(),
        },
    ));
    selection.mandatory_ids.push("turn_1_user".into());

    // Compile with generous budget (everything fits)
    let generous_budget = ContextTokenBudget::new(2048, 256).expect("Valid budget");
    let generous_compiled = DynamicPromptAssembler::compile_exact_budgeted_prompt(
        &slices,
        &selection,
        &generous_budget,
        compile_prompt,
        |prompt| Ok(prompt.len() / 4), // deterministic linear mock tokenizer
    )
    .expect("Generous compile succeeds");

    assert!(generous_compiled.dropped_slice_ids.is_empty());
    assert!(generous_compiled.prompt.contains("LIVA AI Assistant"));
    assert!(generous_compiled.prompt.contains("ancient algorithms"));
    assert!(generous_compiled.prompt.contains("What is 6 * 7?"));

    // Compile with tight budget (forces eviction of P4 history)
    let tight_budget = ContextTokenBudget::new(300, 256).expect("Valid budget");
    let tight_compiled = DynamicPromptAssembler::compile_exact_budgeted_prompt(
        &slices,
        &selection,
        &tight_budget,
        compile_prompt,
        |prompt| Ok(prompt.len() / 4),
    )
    .expect("Tight compile succeeds");

    // Verify P4 history slices were dropped
    assert!(
        tight_compiled
            .dropped_slice_ids
            .contains(&"turn_0_user".to_string())
    );
    assert!(
        tight_compiled
            .dropped_slice_ids
            .contains(&"turn_0_assistant".to_string())
    );
    // Verify P0 and latest user turn remain intact
    assert!(tight_compiled.prompt.contains("LIVA AI Assistant"));
    assert!(tight_compiled.prompt.contains("What is 6 * 7?"));
    assert!(!tight_compiled.prompt.contains("ancient algorithms"));
}

// ---------------------------------------------------------------------------
// RFC 4: APPEND-ONLY SESSION EVENT STREAM & SQLITE WAL PERSISTENCE TESTS
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rfc4_session_events_wal_persistence_and_deterministic_replay() {
    let pool = Arc::new(DatabasePool::new_in_memory().expect("In-memory DB creation failed"));

    let event_stream = SessionEventStream::with_db_pool(128, pool.clone());

    let session_id = "sess-wal-400";

    // Publish multiple session events
    let event1 = SessionEvent::PromptInjected {
        session_id: session_id.into(),
        prompt_tokens: 128,
    };
    let event2 = SessionEvent::ReasoningChunk {
        session_id: session_id.into(),
        token: "Analyzing query parameters".into(),
    };
    let event3 = SessionEvent::ContentChunk {
        session_id: session_id.into(),
        token: "Here is your calculation result".into(),
    };
    let event4 = SessionEvent::ToolCallProposed {
        session_id: session_id.into(),
        tool: "math_multiply".into(),
        args: json!({ "a": 6, "b": 7 }),
    };
    let event5 = SessionEvent::SessionFinished {
        session_id: session_id.into(),
        total_tokens: 180,
        latency_ms: 25,
    };

    event_stream.publish(event1).await.expect("Publish failed");
    event_stream.publish(event2).await.expect("Publish failed");
    event_stream.publish(event3).await.expect("Publish failed");
    event_stream.publish(event4).await.expect("Publish failed");
    event_stream.publish(event5).await.expect("Publish failed");

    // Flush WAL writer to ensure all micro-batches are committed to SQLite
    pool.writer_actor.flush().await.expect("Flush failed");

    // Deterministic replay directly from SQLite WAL disk table
    let replayed = event_stream
        .replay_from_db(session_id)
        .expect("Replay from DB failed");

    assert_eq!(replayed.len(), 5);

    match replayed[0] {
        SessionEvent::PromptInjected { prompt_tokens, .. } => {
            assert_eq!(prompt_tokens, 128);
        }
        ref other => panic!("Expected PromptInjected, got {other:?}"),
    }

    match &replayed[1] {
        SessionEvent::ReasoningChunk { token, .. } => {
            assert_eq!(token, "Analyzing query parameters");
        }
        other => panic!("Expected ReasoningChunk, got {other:?}"),
    }

    match &replayed[2] {
        SessionEvent::ContentChunk { token, .. } => {
            assert_eq!(token, "Here is your calculation result");
        }
        other => panic!("Expected ContentChunk, got {other:?}"),
    }

    match &replayed[3] {
        SessionEvent::ToolCallProposed { tool, .. } => {
            assert_eq!(tool, "math_multiply");
        }
        other => panic!("Expected ToolCallProposed, got {other:?}"),
    }

    match replayed[4] {
        SessionEvent::SessionFinished {
            total_tokens,
            latency_ms,
            ..
        } => {
            assert_eq!(total_tokens, 180);
            assert_eq!(latency_ms, 25);
        }
        ref other => panic!("Expected SessionFinished, got {other:?}"),
    }

    // Test time-travel branched checkpointing
    event_stream
        .save_branched_checkpoint(
            "cp-001",
            session_id,
            None,
            1,
            "digest-turn-1",
            r#"{"turn": 1, "status": "ok"}"#,
        )
        .await
        .expect("Checkpoint save failed");

    event_stream
        .save_branched_checkpoint(
            "cp-002",
            session_id,
            Some("cp-001".to_string()),
            2,
            "digest-turn-2",
            r#"{"turn": 2, "status": "branched"}"#,
        )
        .await
        .expect("Branched checkpoint save failed");

    let checkpoints = event_stream
        .replay_checkpoints_from_db(session_id)
        .expect("Checkpoint replay failed");

    assert_eq!(checkpoints.len(), 2);
    assert_eq!(checkpoints[0].0, "cp-001");
    assert_eq!(checkpoints[0].1, None);
    assert_eq!(checkpoints[1].0, "cp-002");
    assert_eq!(checkpoints[1].1.as_deref(), Some("cp-001"));
}
