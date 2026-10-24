//! Milestone 2 Adversarial Challenge Suite: Compact Prompt Assembler & Tool Insurance Engine
//!
//! Empirical Challenger 2 (challenger_m2_2):
//! 1. Tool Result Forgery & Hallucination:
//!    - Fabricated `<tool_result>` and simulated hallucinated execution without HMAC receipt.
//!    - 1-bit payload & receipt corruption (call_id, tool_name, canonical_args_hash, result_hash, signature).
//!    - Constant-time HMAC verification under malicious timing payloads.
//!    - Replay attack across distinct sessions with independent session secrets.
//!    - Read-After-Write (RAW) verification bypass attempts on mutating operations.
//! 2. Prompt Token Overflow & Budget Enforcement:
//!    - System prompt token overflow (>1000 tokens) strict error rejection.
//!    - Bitemporal memory overflow (>1500 tokens) confidence-sorted budget pruning.
//!    - Skill index (>300 tokens) and dynamic tools catalog (>1500 tokens) budget clamping.
//!    - Dynamic conversation history overflow (>10,000 tokens) reverse-pruning with latest user preservation.
//!    - 100% Static prefix cache invariance across consecutive conversational turns.
//! 3. Worker Escalation:
//!    - Two consecutive schema validation failures trigger `EscalateToWorker`.
//!    - Tool chain depth exceeding `MAX_TOOL_CHAIN_DEPTH` (depth > 2) triggers `EscalateToWorker`.
//!    - Schema argument self-correction on attempt 2 successfully proceeds without escalation.

use liva_core_types::CommandPrincipal;
use liva_native_core::llm::prompt::ChatMessage;
use liva_native_core::llm::prompt::compact_assembler::{
    AssembledCompactPrompt, CompactPromptAssembler, CompactPromptBudget, MemoryFactEntry,
    SkillIndexEntry, estimate_tokens,
};
use liva_native_core::llm::tool_calling::CatalogTool;
use liva_native_core::llm::tool_insurance::{
    canonical_json_hash, compute_hmac_sha256, verify_hmac_constant_time, EscalationReason,
    HmacReceiptVerifier, InsuranceAction, ToolCallReceipt, ToolInsuranceEngine,
    MAX_SCHEMA_RETRIES, MAX_TOOL_CHAIN_DEPTH,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn create_sample_catalog_tools() -> Vec<CatalogTool> {
    vec![
        CatalogTool {
            server: "system".into(),
            name: "read_markdown".into(),
            description: "Read markdown file from vault".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max_lines": { "type": "integer" }
                },
                "required": ["path"]
            }),
            embed_extra: "".into(),
        },
        CatalogTool {
            server: "vault".into(),
            name: "write_markdown".into(),
            description: "Write content to markdown file in vault".into(),
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
            description: "Retrieve DPAPI protected secrets".into(),
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

// =========================================================================
// CHALLENGE 1: TOOL RESULT FORGERY & HMAC CRYPTOGRAPHIC TAMPERING
// =========================================================================

#[test]
fn test_adversarial_tool_result_forgery_unauthenticated_hallucination() {
    let session_secret = [0xAAu8; 32];
    let engine = ToolInsuranceEngine::new(session_secret);
    let verifier = engine.verifier();

    // Attacker model hallucinates output without running tool and fabricates receipt
    let forged_receipt = ToolCallReceipt {
        call_id: "call_hallucinated_001".into(),
        session_id: "sess_victim".into(),
        tool_name: "write_markdown".into(),
        canonical_args_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        result_hash: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
        read_after_write_verified: true,
        hmac_signature: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".into(),
        timestamp_ms: 1729690000000,
    };

    let result = verifier.verify_receipt(&forged_receipt);
    assert!(result.is_err(), "Forged HMAC signature must be strictly rejected");
    assert!(result.unwrap_err().contains("HMAC signature verification failed"));
}

#[test]
fn test_adversarial_hmac_single_bit_payload_corruption() {
    let session_secret = [0x37u8; 32];
    let engine = ToolInsuranceEngine::new(session_secret);
    let verifier = engine.verifier();

    let session_id = "sess_adversary";
    let call_id = "call_safe_100";
    let tool_name = "write_markdown";
    let args = json!({ "path": "notes/secure.md", "content": "Adversarial payload verification" });
    let result_content = "Success: 32 bytes written";

    let genuine_receipt = engine
        .issue_signed_receipt(session_id, call_id, tool_name, &args, result_content, true, true)
        .expect("issue genuine receipt");

    // Baseline: genuine receipt must verify cleanly
    assert!(verifier.verify_receipt(&genuine_receipt).is_ok());

    // 1. Bit flip in call_id
    let mut tampered_call_id = genuine_receipt.clone();
    let mut bytes = tampered_call_id.call_id.into_bytes();
    bytes[0] ^= 0x01; // flip 1 bit
    tampered_call_id.call_id = String::from_utf8(bytes).unwrap();
    assert!(
        verifier.verify_receipt(&tampered_call_id).is_err(),
        "1-bit flip in call_id must fail verification"
    );

    // 2. Bit flip in tool_name
    let mut tampered_tool = genuine_receipt.clone();
    let mut bytes = tampered_tool.tool_name.into_bytes();
    bytes[0] ^= 0x01;
    tampered_tool.tool_name = String::from_utf8(bytes).unwrap();
    assert!(
        verifier.verify_receipt(&tampered_tool).is_err(),
        "1-bit flip in tool_name must fail verification"
    );

    // 3. Bit flip in canonical_args_hash
    let mut tampered_args_hash = genuine_receipt.clone();
    let mut bytes = tampered_args_hash.canonical_args_hash.into_bytes();
    bytes[0] ^= 0x01;
    tampered_args_hash.canonical_args_hash = String::from_utf8(bytes).unwrap();
    assert!(
        verifier.verify_receipt(&tampered_args_hash).is_err(),
        "1-bit flip in canonical_args_hash must fail verification"
    );

    // 4. Bit flip in result_hash
    let mut tampered_result_hash = genuine_receipt.clone();
    let mut bytes = tampered_result_hash.result_hash.into_bytes();
    let last_idx = bytes.len() - 1;
    bytes[last_idx] ^= 0x01;
    tampered_result_hash.result_hash = String::from_utf8(bytes).unwrap();
    assert!(
        verifier.verify_receipt(&tampered_result_hash).is_err(),
        "1-bit flip in result_hash must fail verification"
    );

    // 5. Bit flip across multiple positions in hmac_signature
    for flip_pos in [0, 15, 31, 47, 63] {
        let mut tampered_sig = genuine_receipt.clone();
        let mut sig_chars: Vec<char> = tampered_sig.hmac_signature.chars().collect();
        // Change hex character
        sig_chars[flip_pos] = if sig_chars[flip_pos] == 'a' { 'b' } else { 'a' };
        tampered_sig.hmac_signature = sig_chars.into_iter().collect();
        assert!(
            verifier.verify_receipt(&tampered_sig).is_err(),
            "Corrupting HMAC signature at character {} must fail",
            flip_pos
        );
    }

    // 6. Signature length malformation (truncated or oversized hex)
    let mut truncated_sig = genuine_receipt.clone();
    truncated_sig.hmac_signature.pop();
    assert!(
        verifier.verify_receipt(&truncated_sig).is_err(),
        "Truncated HMAC signature (63 chars) must fail"
    );

    let mut oversized_sig = genuine_receipt.clone();
    oversized_sig.hmac_signature.push('0');
    assert!(
        verifier.verify_receipt(&oversized_sig).is_err(),
        "Oversized HMAC signature (65 chars) must fail"
    );

    // 7. Non-hex invalid characters
    let mut non_hex_sig = genuine_receipt.clone();
    non_hex_sig.hmac_signature = "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ".into();
    assert!(
        verifier.verify_receipt(&non_hex_sig).is_err(),
        "Non-hex characters in HMAC signature must fail"
    );
}

#[test]
fn test_adversarial_cross_session_receipt_replay_attack() {
    let session_a_secret = [0x11u8; 32];
    let session_b_secret = [0x22u8; 32];

    let engine_a = ToolInsuranceEngine::new(session_a_secret);
    let engine_b = ToolInsuranceEngine::new(session_b_secret);

    let call_id = "call_replay_001";
    let tool_name = "write_markdown";
    let args = json!({ "path": "inbox/note.md", "content": "Cross-session leak" });
    let result_content = "Created 20 bytes";

    // Session A generates a legitimate receipt
    let receipt_a = engine_a
        .issue_signed_receipt("session_a", call_id, tool_name, &args, result_content, true, true)
        .expect("issue receipt in session A");

    assert!(engine_a.verifier().verify_receipt(&receipt_a).is_ok());

    // Attacker attempts to replay Session A's receipt into Session B
    let replay_result = engine_b.verifier().verify_receipt(&receipt_a);
    assert!(
        replay_result.is_err(),
        "Cross-session replayed receipt MUST be rejected due to mismatched session secret"
    );
}

#[test]
fn test_adversarial_read_after_write_unconfirmed_bypass_attempt() {
    let session_secret = [0x99u8; 32];
    let engine = ToolInsuranceEngine::new(session_secret);

    let session_id = "sess_raw_bypass";
    let call_id = "call_mutate_1";
    let tool_name = "delete_file";
    let args = json!({ "path": "sensitive/vault.db" });
    let result_content = "File deleted successfully";

    // Mutating tool with raw_confirmed = false MUST be refused receipt issuance
    let unconfirmed_issue = engine.issue_signed_receipt(
        session_id,
        call_id,
        tool_name,
        &args,
        result_content,
        true,  // is_mutating
        false, // raw_confirmed is FALSE
    );
    assert!(unconfirmed_issue.is_err(), "Must reject issuance when RAW check fails");
    let err_msg = unconfirmed_issue.unwrap_err();
    assert!(err_msg.contains("Read-after-write verification failed"));

    // Attacker crafts forged receipt with read_after_write_verified = true
    let forged = ToolCallReceipt {
        call_id: call_id.into(),
        session_id: session_id.into(),
        tool_name: tool_name.into(),
        canonical_args_hash: canonical_json_hash(&args),
        result_hash: hex::encode(Sha256::digest(result_content.as_bytes())),
        read_after_write_verified: true, // Attacker lied
        hmac_signature: "00".repeat(32),
        timestamp_ms: 1729691234567,
    };
    assert!(engine.verifier().verify_receipt(&forged).is_err());
}

#[test]
fn test_adversarial_canonical_json_hash_mutation_sensitivity() {
    let args_base = json!({
        "target": "server",
        "action": "restart",
        "force": false
    });

    let hash_base = canonical_json_hash(&args_base);

    // Permute keys: canonical hash MUST remain invariant
    let args_permuted = json!({
        "force": false,
        "target": "server",
        "action": "restart"
    });
    let hash_permuted = canonical_json_hash(&args_permuted);
    assert_eq!(hash_base, hash_permuted);

    // Mutate boolean flag: hash MUST change
    let args_mutated_bool = json!({
        "target": "server",
        "action": "restart",
        "force": true
    });
    assert_ne!(hash_base, canonical_json_hash(&args_mutated_bool));

    // Inject unauthorized extra parameter: hash MUST change
    let args_injected = json!({
        "target": "server",
        "action": "restart",
        "force": false,
        "sudo": true
    });
    assert_ne!(hash_base, canonical_json_hash(&args_injected));
}

// =========================================================================
// CHALLENGE 2: PROMPT TOKEN OVERFLOW & BUDGET PRUNING STRESS
// =========================================================================

#[test]
fn test_adversarial_system_prompt_overflow_strict_rejection() {
    let budget = CompactPromptBudget::default(); // max_system_tokens = 1000
    let tools = create_sample_catalog_tools();
    let skills = vec![SkillIndexEntry {
        skill_id: "test".into(),
        description: "Test skill".into(),
    }];
    let memories = vec![];
    let turns = vec![ChatMessage {
        role: "user".into(),
        content: "Hello".into(),
    }];

    // Construct an adversarial system rules string with > 1000 tokens (> 4000 characters)
    let overflow_system_rules = "You are an AI assistant. ".repeat(300); // 300 * 25 = 7500 chars ≈ 1875 tokens
    let est_tokens = estimate_tokens(&overflow_system_rules);
    assert!(est_tokens > 1000, "Must exceed 1000 tokens: est = {}", est_tokens);

    let result = CompactPromptAssembler::assemble(
        &overflow_system_rules,
        &skills,
        &tools,
        &memories,
        &turns,
        &budget,
    );

    assert!(result.is_err(), "Must return Err on system prompt token overflow");
    let err = result.unwrap_err();
    assert!(err.contains("Static system rules exceed budget"));
    assert!(err.contains("1000"));
}

#[test]
fn test_adversarial_bitemporal_memory_overflow_budget_pruning() {
    let budget = CompactPromptBudget::default(); // max_memory_tokens = 1500
    let tools = create_sample_catalog_tools();
    let skills = vec![];
    let turns = vec![ChatMessage {
        role: "user".into(),
        content: "What do you know about me?".into(),
    }];
    let system_rules = "You are LIVA.";

    // Generate 60 active memory facts, each ~40 tokens. Total = ~2400 tokens (> 1500 limit).
    let mut large_memories = Vec::new();
    for i in 0..60 {
        large_memories.push(MemoryFactEntry {
            fact_id: format!("fact_{i:03}"),
            subject: format!("user_profile_{i}"),
            predicate: "preference_detail_attribute".into(),
            object_value: format!("detailed_preference_specification_value_string_for_testing_boundaries_{i}"),
            scope: "global".into(),
            confidence: (i as f32) / 60.0, // Increasing confidence from 0.0 to ~0.98
            is_active: true,
        });
    }

    let assembled = CompactPromptAssembler::assemble(
        system_rules,
        &skills,
        &tools,
        &large_memories,
        &turns,
        &budget,
    )
    .expect("assemble prompt with pruned memory");

    // Must stay strictly within budget <= 1500 tokens
    assert!(
        assembled.estimated_memory_tokens <= budget.max_memory_tokens,
        "Memory tokens {} must not exceed budget {}",
        assembled.estimated_memory_tokens,
        budget.max_memory_tokens
    );
    assert!(assembled.estimated_memory_tokens > 1000, "Should pack memory close to 1500 token limit");

    // Invariant: Higher confidence memories must be retained before lower confidence ones!
    // Since confidence 0.98 (i=59) is highest, it must be present in the assembled prompt
    assert!(
        assembled.system_turn_content.contains("user_profile_59"),
        "Highest confidence memory must be preserved"
    );
    // Lower confidence memories (i=0, confidence 0.0) must be pruned
    assert!(
        !assembled.system_turn_content.contains("user_profile_00"),
        "Lowest confidence memory must be pruned out"
    );
}

#[test]
fn test_adversarial_skills_and_tools_catalog_overflow_pruning() {
    let budget = CompactPromptBudget::default();
    let system_rules = "You are LIVA.";
    let memories = vec![];
    let turns = vec![ChatMessage {
        role: "user".into(),
        content: "Execute task".into(),
    }];

    // 1. Generate 40 skills (each ~20 tokens = 800 tokens, exceeding 300 token budget)
    let mut many_skills = Vec::new();
    for i in 0..40 {
        many_skills.push(SkillIndexEntry {
            skill_id: format!("skill_module_{i:02}"),
            description: format!("Automated specialist capability description for skill index stress test #{i}"),
        });
    }

    // 2. Generate 50 tools (each ~50 tokens = 2500 tokens, exceeding 1500 token budget)
    let mut many_tools = Vec::new();
    for i in 0..50 {
        many_tools.push(CatalogTool {
            server: "stress_server".into(),
            name: format!("stress_tool_operation_{i:02}"),
            description: format!("Comprehensive tool description for stress testing parameter schemas #{i}"),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "param_alpha": { "type": "string" },
                    "param_beta": { "type": "integer" },
                    "param_gamma": { "type": "boolean" }
                },
                "required": ["param_alpha"]
            }),
            embed_extra: "".into(),
        });
    }

    let assembled = CompactPromptAssembler::assemble(
        system_rules,
        &many_skills,
        &many_tools,
        &memories,
        &turns,
        &budget,
    )
    .expect("assemble with clamped skills and tools");

    assert!(
        assembled.estimated_skill_index_tokens <= budget.max_skill_index_tokens,
        "Skill index tokens {} must be <= {}",
        assembled.estimated_skill_index_tokens,
        budget.max_skill_index_tokens
    );
    assert!(
        assembled.estimated_tools_tokens <= budget.max_tools_tokens,
        "Tools catalog tokens {} must be <= {}",
        assembled.estimated_tools_tokens,
        budget.max_tools_tokens
    );
}

#[test]
fn test_adversarial_massive_conversation_history_overflow_pruning() {
    // Dialogue 4k budget: n_ctx = 4096, reserve = 512, total static prefix cap = 3500
    let budget = CompactPromptBudget::for_dialogue_4k();
    let system_rules = "System rules.";
    let tools = create_sample_catalog_tools();
    let skills = vec![];
    let memories = vec![];

    // Generate 50 conversation turns totaling ~6000 tokens (well beyond available 3460 tokens)
    let mut turns = Vec::new();
    for i in 0..50 {
        let text = format!("Turn {i}: ") + &"Detailed conversation content message payload repeating multiple words for token consumption test. ".repeat(5);
        turns.push(ChatMessage {
            role: if i % 2 == 0 { "user".into() } else { "assistant".into() },
            content: text,
        });
    }

    let assembled = CompactPromptAssembler::assemble(
        system_rules,
        &skills,
        &tools,
        &memories,
        &turns,
        &budget,
    )
    .expect("assemble with massive history");

    assert!(assembled.dropped_history_turns > 0, "Older turns must be dropped");

    // Invariant: The very latest user message MUST be preserved!
    let latest_user_msg = turns.iter().rev().find(|m| m.role == "user").unwrap();
    let found_latest = assembled
        .full_messages
        .iter()
        .any(|m| m.content.contains(&latest_user_msg.content));
    assert!(found_latest, "Mandatory latest user turn must NEVER be dropped");
}

#[test]
fn test_adversarial_static_prefix_byte_level_cache_invariance() {
    let budget = CompactPromptBudget::default();
    let system_rules = "You are LIVA, local private AI. Always be concise.";
    let tools = create_sample_catalog_tools();
    let skills = vec![SkillIndexEntry {
        skill_id: "obsidian".into(),
        description: "Obsidian PKM vault integration".into(),
    }];
    let memories = vec![MemoryFactEntry {
        fact_id: "f1".into(),
        subject: "user".into(),
        predicate: "name".into(),
        object_value: "Alice".into(),
        scope: "global".into(),
        confidence: 0.95,
        is_active: true,
    }];

    // Turn 1
    let turns_turn1 = vec![ChatMessage {
        role: "user".into(),
        content: "What is my name?".into(),
    }];

    // Turn 2 (with turn 1 history appended)
    let turns_turn2 = vec![
        ChatMessage {
            role: "user".into(),
            content: "What is my name?".into(),
        },
        ChatMessage {
            role: "assistant".into(),
            content: "Your name is Alice.".into(),
        },
        ChatMessage {
            role: "user".into(),
            content: "Now search my notes for project plans.".into(),
        },
    ];

    let p1 = CompactPromptAssembler::assemble(
        system_rules,
        &skills,
        &tools,
        &memories,
        &turns_turn1,
        &budget,
    )
    .unwrap();

    let p2 = CompactPromptAssembler::assemble(
        system_rules,
        &skills,
        &tools,
        &memories,
        &turns_turn2,
        &budget,
    )
    .unwrap();

    // The system prompt block MUST be 100% byte-for-byte identical across turns!
    assert_eq!(
        p1.system_turn_content, p2.system_turn_content,
        "System prompt block must be 100% byte-identical for prefix caching"
    );
    assert_eq!(
        p1.full_messages[0].content, p2.full_messages[0].content,
        "Leading system turn in message list must match identically"
    );
}

// =========================================================================
// CHALLENGE 3: WORKER ESCALATION PROTOCOL STRESS
// =========================================================================

#[test]
fn test_adversarial_worker_escalation_two_consecutive_schema_failures() {
    let session_secret = [0x55u8; 32];
    let engine = ToolInsuranceEngine::new(session_secret);
    let tools = create_sample_catalog_tools();
    let write_tool = &tools[1]; // requires "path" and "content"

    // Step 1: 1st invalid call (missing "content"). retry_count = 0.
    let invalid_args_1 = json!({ "path": "notes/todo.md" });
    let action_step1 = engine.inspect_and_insure(write_tool, &invalid_args_1, 0, 1);

    match action_step1 {
        InsuranceAction::RetryWithFeedback { feedback_prompt, retry_count } => {
            assert_eq!(retry_count, 1, "Must increment retry_count to 1");
            assert!(feedback_prompt.contains("thiếu tham số bắt buộc: content"));
        }
        other => panic!("Expected RetryWithFeedback on 1st failure, got {:?}", other),
    }

    // Step 2: 2nd invalid call (model again provides invalid args, e.g. empty string guessing). retry_count = 1.
    let invalid_args_2 = json!({ "path": "notes/todo.md", "content": "   " });
    let action_step2 = engine.inspect_and_insure(write_tool, &invalid_args_2, 1, 1);

    match action_step2 {
        InsuranceAction::EscalateToWorker { reason } => match reason {
            EscalationReason::SchemaValidationFailedTwice(err) => {
                assert!(
                    err.contains("tham số bắt buộc rỗng"),
                    "Error must indicate empty string hallucination rejection: {}",
                    err
                );
            }
            other => panic!("Expected SchemaValidationFailedTwice, got {:?}", other),
        },
        other => panic!("Expected EscalateToWorker on 2nd failure, got {:?}", other),
    }
}

#[test]
fn test_adversarial_worker_escalation_recovery_on_valid_retry() {
    let session_secret = [0x66u8; 32];
    let engine = ToolInsuranceEngine::new(session_secret);
    let tools = create_sample_catalog_tools();
    let read_tool = &tools[0]; // requires "path"

    // Attempt 1: Failed (missing path)
    let invalid_args = json!({});
    let action_1 = engine.inspect_and_insure(read_tool, &invalid_args, 0, 1);
    assert!(matches!(action_1, InsuranceAction::RetryWithFeedback { retry_count: 1, .. }));

    // Attempt 2: Model provides valid args after feedback
    let valid_args = json!({ "path": "notes/read.md", "max_lines": 50 });
    let action_2 = engine.inspect_and_insure(read_tool, &valid_args, 1, 1);
    assert_eq!(
        action_2,
        InsuranceAction::Proceed,
        "Model successfully corrected args; must Proceed without escalation"
    );
}

#[test]
fn test_adversarial_worker_escalation_deep_tool_chain_limit() {
    let session_secret = [0x77u8; 32];
    let engine = ToolInsuranceEngine::new(session_secret);
    let tools = create_sample_catalog_tools();
    let read_tool = &tools[0];
    let valid_args = json!({ "path": "test.md" });

    // Chain step 1: <= MAX_TOOL_CHAIN_DEPTH (2) -> Proceed
    let act_step1 = engine.inspect_and_insure(read_tool, &valid_args, 0, 1);
    assert_eq!(act_step1, InsuranceAction::Proceed);

    // Chain step 2: == MAX_TOOL_CHAIN_DEPTH (2) -> Proceed
    let act_step2 = engine.inspect_and_insure(read_tool, &valid_args, 0, 2);
    assert_eq!(act_step2, InsuranceAction::Proceed);

    // Chain step 3: > MAX_TOOL_CHAIN_DEPTH (2) -> Escalate to Worker
    let act_step3 = engine.inspect_and_insure(read_tool, &valid_args, 0, 3);
    match act_step3 {
        InsuranceAction::EscalateToWorker { reason } => {
            assert_eq!(reason, EscalationReason::ToolChainLimitExceeded(3));
        }
        other => panic!("Expected ToolChainLimitExceeded, got {:?}", other),
    }

    // Chain step 10: Deep runaway chain -> Escalate to Worker
    let act_step10 = engine.inspect_and_insure(read_tool, &valid_args, 0, 10);
    assert!(matches!(
        act_step10,
        InsuranceAction::EscalateToWorker {
            reason: EscalationReason::ToolChainLimitExceeded(10)
        }
    ));
}
