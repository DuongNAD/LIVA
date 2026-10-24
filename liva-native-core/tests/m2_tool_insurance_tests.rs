//! Milestone 2 Integration Tests: 4-Tier Tool Insurance Engine & HMAC Receipts (Feature F12).

use liva_core_types::CommandPrincipal;
use liva_native_core::llm::tool_calling::CatalogTool;
use liva_native_core::llm::tool_insurance::{
    canonical_json_hash, compute_hmac_sha256, verify_hmac_constant_time, EscalationReason,
    HmacReceiptVerifier, InsuranceAction, ToolCallReceipt, ToolInsuranceEngine,
    MAX_SCHEMA_RETRIES, MAX_TOOL_CHAIN_DEPTH,
};
use serde_json::json;

fn create_test_catalog() -> Vec<CatalogTool> {
    vec![
        CatalogTool {
            server: "vault".into(),
            name: "read_markdown".into(),
            description: "Read markdown note".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
            embed_extra: "".into(),
        },
        CatalogTool {
            server: "vault".into(),
            name: "write_markdown".into(),
            description: "Write markdown note to vault".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
            embed_extra: "".into(),
        },
        CatalogTool {
            server: "security".into(),
            name: "dpapi_secrets".into(),
            description: "Access encrypted DPAPI credential store".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" }
                },
                "required": ["key"]
            }),
            embed_extra: "".into(),
        },
    ]
}

#[test]
fn test_f12_tier1_native_tools_formatting_and_principal_filtering() {
    let session_secret = [7u8; 32];
    let engine = ToolInsuranceEngine::new(session_secret);
    let tools = create_test_catalog();

    // 1. LocalCli (trusted root) receives ALL tools
    let cli_tools = engine.format_native_tools(&tools, CommandPrincipal::LocalCli);
    assert_eq!(cli_tools.len(), 3);
    assert_eq!(cli_tools[0]["type"], "function");
    assert_eq!(cli_tools[0]["function"]["name"], "read_markdown");

    // 2. Telegram (external untrusted) has sensitive tools pruned (dpapi, write_markdown)
    let tg_tools = engine.format_native_tools(&tools, CommandPrincipal::Telegram);
    assert_eq!(tg_tools.len(), 1);
    assert_eq!(tg_tools[0]["function"]["name"], "read_markdown");
}

#[test]
fn test_f12_tier2_closed_enum_grammar_schema() {
    let session_secret = [9u8; 32];
    let engine = ToolInsuranceEngine::new(session_secret);
    let tools = create_test_catalog();

    let schema = engine.generate_closed_enum_schema(&tools);
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);

    let enums = schema["properties"]["tool"]["enum"].as_array().unwrap();
    assert_eq!(enums[0], "none");
    assert_eq!(enums[1], "read_markdown");
    assert_eq!(enums[2], "write_markdown");
    assert_eq!(enums[3], "dpapi_secrets");
}

#[test]
fn test_f12_tier3_schema_retry_and_worker_escalation() {
    let session_secret = [11u8; 32];
    let engine = ToolInsuranceEngine::new(session_secret);
    let tools = create_test_catalog();
    let read_tool = &tools[0]; // requires "path"

    // Case 1: Missing required parameter on attempt 1 (retry_count = 0)
    let invalid_args = json!({});
    let action_attempt1 = engine.inspect_and_insure(read_tool, &invalid_args, 0, 1);
    match action_attempt1 {
        InsuranceAction::RetryWithFeedback { feedback_prompt, retry_count } => {
            assert_eq!(retry_count, 1);
            assert!(feedback_prompt.contains("argument validation failed"));
            assert!(feedback_prompt.contains("thiếu tham số bắt buộc: path"));
        }
        other => panic!("Expected RetryWithFeedback, got {:?}", other),
    }

    // Case 2: Missing required parameter on attempt 2 (retry_count = 1) -> Escalate to Worker
    let action_attempt2 = engine.inspect_and_insure(read_tool, &invalid_args, 1, 1);
    match action_attempt2 {
        InsuranceAction::EscalateToWorker { reason } => match reason {
            EscalationReason::SchemaValidationFailedTwice(err) => {
                assert!(err.contains("thiếu tham số bắt buộc: path"));
            }
            other => panic!("Expected SchemaValidationFailedTwice, got {:?}", other),
        },
        other => panic!("Expected EscalateToWorker, got {:?}", other),
    }

    // Case 3: Empty string parameter in required field -> Rejected as hallucination guessing
    let empty_str_args = json!({ "path": "" });
    let action_empty = engine.inspect_and_insure(read_tool, &empty_str_args, 0, 1);
    assert!(matches!(action_empty, InsuranceAction::RetryWithFeedback { .. }));

    // Case 4: Tool chain limit exceeded (chain_step > 2) -> Escalate to Worker
    let valid_args = json!({ "path": "docs/readme.md" });
    let action_deep_chain = engine.inspect_and_insure(read_tool, &valid_args, 0, 3);
    match action_deep_chain {
        InsuranceAction::EscalateToWorker { reason } => {
            assert_eq!(reason, EscalationReason::ToolChainLimitExceeded(3));
        }
        other => panic!("Expected ToolChainLimitExceeded, got {:?}", other),
    }

    // Case 5: Valid arguments within chain limit -> Proceed
    let action_proceed = engine.inspect_and_insure(read_tool, &valid_args, 0, 1);
    assert_eq!(action_proceed, InsuranceAction::Proceed);
}

#[test]
fn test_f12_tier4_hmac_receipt_issuance_and_verification() {
    let session_secret = [42u8; 32];
    let engine = ToolInsuranceEngine::new(session_secret);

    let session_id = "sess_verify_123";
    let call_id = "call_abc_001";
    let tool_name = "write_markdown";
    let args = json!({ "path": "notes/todo.md", "content": "Finish Milestone 2" });
    let result = "File notes/todo.md written successfully (24 bytes)";

    // 1. Read-After-Write Verification check for mutating tool:
    // If raw_confirmed is false, issuance MUST be refused.
    let fail_issue = engine.issue_signed_receipt(
        session_id,
        call_id,
        tool_name,
        &args,
        result,
        true,  // is_mutating
        false, // raw_confirmed = false!
    );
    assert!(fail_issue.is_err(), "Must refuse receipt if RAW check failed");

    // 2. Issuance with raw_confirmed = true succeeds
    let receipt = engine
        .issue_signed_receipt(
            session_id,
            call_id,
            tool_name,
            &args,
            result,
            true, // is_mutating
            true, // raw_confirmed
        )
        .expect("issue signed receipt");

    assert_eq!(receipt.call_id, call_id);
    assert_eq!(receipt.tool_name, tool_name);
    assert!(receipt.read_after_write_verified);
    assert_eq!(receipt.hmac_signature.len(), 64);

    // 3. Verification of legitimate receipt
    let verifier = engine.verifier();
    assert!(verifier.verify_receipt(&receipt).is_ok(), "Legitimate receipt must verify cleanly");

    // 4. Rejection of tampered receipt (e.g. forged result_hash or arguments)
    let mut tampered = receipt.clone();
    tampered.result_hash = "0000000000000000000000000000000000000000000000000000000000000000".into();
    assert!(verifier.verify_receipt(&tampered).is_err(), "Tampered receipt must be rejected");

    let mut tampered_tool = receipt.clone();
    tampered_tool.tool_name = "delete_file".into();
    assert!(verifier.verify_receipt(&tampered_tool).is_err(), "Tampered tool name must be rejected");
}

#[test]
fn test_f12_canonical_json_hash_key_sorting() {
    // Different key ordering in JSON objects must produce the exact same canonical hash
    let json_a = json!({ "z_key": 1, "a_key": 2, "nested": { "beta": 1, "alpha": 2 } });
    let json_b = json!({ "a_key": 2, "nested": { "alpha": 2, "beta": 1 }, "z_key": 1 });

    let hash_a = canonical_json_hash(&json_a);
    let hash_b = canonical_json_hash(&json_b);
    assert_eq!(hash_a, hash_b, "Canonical JSON hash must be key-order invariant");
}

#[test]
fn test_f12_constant_time_verification_mechanics() {
    let key = [0x55u8; 32];
    let msg = b"payload for constant time test";
    let hmac = compute_hmac_sha256(&key, msg);

    assert!(verify_hmac_constant_time(&hmac, &hmac));

    let mut tampered = hmac;
    tampered[15] ^= 0x01;
    assert!(!verify_hmac_constant_time(&hmac, &tampered));

    let wrong_len = [0u8; 16];
    assert!(!verify_hmac_constant_time(&hmac, &wrong_len));
}
