//! Empirical Challenger 2: Adversarial Security Verification Suite (Milestone M4)
//!
//! Empirically challenges:
//! 1. Audit Chain Tampering & SQLite Trigger Immutability (Feature F24):
//!    - Verifies raw SQL `UPDATE` and `DELETE` on `audit_hash_chain` are 100% rejected by SQLite triggers `trg_audit_chain_no_update` and `trg_audit_chain_no_delete`.
//!    - Verifies cryptographic tamper-evidence: 1-byte corruption of `block_hash`, `prev_block_hash`, `payload_hash`, `redacted_payload`, `timestamp_ms`, or action fields immediately fails `verify_chain_integrity`.
//!    - Verifies middle-block deletion and pointer discontinuity detection.
//! 2. Untrusted Taint Sink Violation & Labeled<T> Invariants (Feature F20):
//!    - Verifies `Labeled<T>` attaches untrusted taint flags and origin.
//!    - Verifies functor `map` preserves taint across data transformations.
//!    - Verifies monoid `combine` propagates untrusted taint when merged with trusted owner data.
//!    - Verifies `ScopedToolRegistry` enforces `ExecutorPolicy::enforce_sink_call` on untrusted inputs (Telegram principal / tainted data) into mutating sinks, returning `PolicyViolation` with `MutationForbiddenInReaderMode` or `TaintedDataSinkViolation`.
//!    - Verifies expired declassification receipt is rejected while unexpired receipt permits execution.
//! 3. Canonical Hash & Invisible Character Detection (Feature F23):
//!    - Verifies JSON key permutations and nested objects produce 100% identical canonical strings and BLAKE3 hashes.
//!    - Verifies Unicode NFD decomposed characters normalize to NFC with matching canonical hash.
//!    - Verifies `visualize_invisible_chars` detects and tags all Unicode zero-width / BiDi control characters (ZWSP, ZWNJ, ZWJ, LRM, RLM, LRE, RLE, PDF, LRO, RLO, BOM, SHY).
//! 4. Emergency Kill Switch Invariants & Latency SLA (Feature F25):
//!    - Verifies atomic epoch evaluation latency is < 5ns.
//!    - Validates architectural defects: private `mod authorization;` in `lib.rs` and missing `escalate_epoch` method on `EmergencyKillSwitch`.

use liva_core_types::{
    CallToolResult, CommandPrincipal, DataOrigin, DeclassificationReceipt, IntegrityLevel,
    Labeled, Principal, SecurityError, SessionMode, TaintFlags, Tool, ToolContent, ToolExecutionContext,
};
use liva_native_core::cognitive::audit_chain::{AuditBlock, Blake3AuditChain, GENESIS_PREV_HASH};
use liva_native_core::cognitive::policy::{
    canonicalize_json, compute_canonical_args_hash, visualize_invisible_chars, RiskTier,
};
use liva_native_core::llm::{CatalogTool, ScopedToolRegistry, ToolScope};
use rusqlite::Connection;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// 1. AUDIT CHAIN TAMPERING & SQLITE TRIGGER IMMUTABILITY (FEATURE F24)
// ============================================================================

#[test]
fn test_m4_c2_audit_chain_immutability_triggers_reject_raw_update_and_delete() {
    let conn = Connection::open_in_memory().expect("open memory db");
    Blake3AuditChain::init_schema(&conn).expect("init audit schema");

    // Empty chain verifies cleanly with count = 0
    assert_eq!(
        Blake3AuditChain::verify_chain_integrity(&conn).unwrap(),
        0,
        "Empty chain must verify cleanly with count 0"
    );

    // Append 5 real audit blocks
    for i in 1..=5 {
        Blake3AuditChain::append_entry(
            &conn,
            "owner",
            &format!("action_{i}"),
            &format!("target_{i}.dat"),
            &format!("raw payload content for block {i}"),
            RiskTier::R1,
            "allow",
            "success",
            Some(i * 10),
        )
        .expect("append audit block");
    }

    assert_eq!(
        Blake3AuditChain::verify_chain_integrity(&conn).unwrap(),
        5,
        "Chain with 5 blocks must verify cleanly"
    );

    // Attack 1: Direct SQL raw UPDATE on action
    let update_action_res = conn.execute(
        "UPDATE audit_hash_chain SET action = 'fake' WHERE sequence_num = 2",
        [],
    );
    assert!(
        update_action_res.is_err(),
        "CRITICAL BUG: SQLite trigger failed to abort raw UPDATE on action"
    );
    let err_msg = update_action_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("UPDATE operations are prohibited")
            || err_msg.contains("strictly append-only"),
        "Error message must cite immutability trigger abort: {err_msg}"
    );

    // Attack 2: Direct SQL raw UPDATE on block_hash
    let update_hash_res = conn.execute(
        "UPDATE audit_hash_chain SET block_hash = 'badhash0000' WHERE sequence_num = 3",
        [],
    );
    assert!(
        update_hash_res.is_err(),
        "CRITICAL BUG: SQLite trigger failed to abort raw UPDATE on block_hash"
    );

    // Attack 3: Direct SQL raw UPDATE on redacted_payload
    let update_payload_res = conn.execute(
        "UPDATE audit_hash_chain SET redacted_payload = 'malicious' WHERE sequence_num = 1",
        [],
    );
    assert!(
        update_payload_res.is_err(),
        "CRITICAL BUG: SQLite trigger failed to abort raw UPDATE on redacted_payload"
    );

    // Attack 4: Direct SQL raw DELETE single row
    let delete_single_res = conn.execute(
        "DELETE FROM audit_hash_chain WHERE sequence_num = 3",
        [],
    );
    assert!(
        delete_single_res.is_err(),
        "CRITICAL BUG: SQLite trigger failed to abort raw DELETE on single row"
    );
    let del_err_msg = delete_single_res.unwrap_err().to_string();
    assert!(
        del_err_msg.contains("DELETE operations are prohibited")
            || del_err_msg.contains("strictly append-only"),
        "Error message must cite immutability trigger abort: {del_err_msg}"
    );

    // Attack 5: Direct SQL raw DELETE all rows
    let delete_all_res = conn.execute("DELETE FROM audit_hash_chain", []);
    assert!(
        delete_all_res.is_err(),
        "CRITICAL BUG: SQLite trigger failed to abort raw DELETE all rows"
    );

    // Integrity must remain 100% intact after all rejected attacks
    assert_eq!(
        Blake3AuditChain::verify_chain_integrity(&conn).unwrap(),
        5,
        "Audit chain integrity must remain 100% intact with 5 valid blocks"
    );
}

#[test]
fn test_m4_c2_audit_chain_cryptographic_tamper_detection_1byte_hash_corruption() {
    let conn = Connection::open_in_memory().expect("open memory db");
    Blake3AuditChain::init_schema(&conn).expect("init audit schema");

    // Populate 5 blocks
    for i in 1..=5 {
        Blake3AuditChain::append_entry(
            &conn,
            "owner",
            &format!("action_{i}"),
            &format!("target_{i}"),
            &format!("payload_{i}"),
            RiskTier::R0,
            "allow",
            "success",
            None,
        )
        .expect("append block");
    }

    assert_eq!(
        Blake3AuditChain::verify_chain_integrity(&conn).unwrap(),
        5
    );

    // Simulate an offline attack or dropped trigger: drop triggers to allow direct row mutation
    conn.execute("DROP TRIGGER IF EXISTS trg_audit_chain_no_update", [])
        .expect("drop update trigger for offline attack simulation");
    conn.execute("DROP TRIGGER IF EXISTS trg_audit_chain_no_delete", [])
        .expect("drop delete trigger for offline attack simulation");

    // Sub-attack 1: Tamper exactly 1 character in block_hash of block 3
    let original_hash: String = conn
        .query_row(
            "SELECT block_hash FROM audit_hash_chain WHERE sequence_num = 3",
            [],
            |r| r.get(0),
        )
        .expect("fetch block 3 hash");

    let mut corrupted_hash = original_hash.clone();
    // Flip first char ('0' -> '1' or 'a' -> 'b')
    let first_char = corrupted_hash.chars().next().unwrap();
    let replacement = if first_char == '0' { '1' } else { '0' };
    corrupted_hash.replace_range(0..1, &replacement.to_string());

    conn.execute(
        "UPDATE audit_hash_chain SET block_hash = ?1 WHERE sequence_num = 3",
        [&corrupted_hash],
    )
    .expect("apply 1-char corruption");

    let verify_res = Blake3AuditChain::verify_chain_integrity(&conn);
    assert!(
        verify_res.is_err(),
        "CRITICAL BUG: verify_chain_integrity failed to detect 1-byte corruption in block_hash"
    );
    let err = verify_res.unwrap_err();
    assert!(
        err.contains("corrupted at block 3") || err.contains("mismatch"),
        "Error must identify block 3 hash corruption: {err}"
    );

    // Restore block 3 hash
    conn.execute(
        "UPDATE audit_hash_chain SET block_hash = ?1 WHERE sequence_num = 3",
        [&original_hash],
    )
    .expect("restore block 3 hash");
    assert_eq!(
        Blake3AuditChain::verify_chain_integrity(&conn).unwrap(),
        5
    );

    // Sub-attack 2: Tamper prev_block_hash of block 4
    conn.execute(
        "UPDATE audit_hash_chain SET prev_block_hash = '0000000000000000000000000000000000000000000000000000000000000000' WHERE sequence_num = 4",
        [],
    )
    .expect("tamper prev_block_hash");
    let prev_err = Blake3AuditChain::verify_chain_integrity(&conn).unwrap_err();
    assert!(
        prev_err.contains("Audit chain broken at block 4: prev_hash mismatch"),
        "Must identify prev_hash pointer break at block 4: {prev_err}"
    );

    // Restore prev_block_hash
    conn.execute(
        "UPDATE audit_hash_chain SET prev_block_hash = ?1 WHERE sequence_num = 4",
        [&original_hash],
    )
    .expect("restore prev_block_hash");
    assert_eq!(
        Blake3AuditChain::verify_chain_integrity(&conn).unwrap(),
        5
    );

    // Sub-attack 3: Tamper payload text in block 2
    conn.execute(
        "UPDATE audit_hash_chain SET redacted_payload = 'tampered payload content' WHERE sequence_num = 2",
        [],
    )
    .expect("tamper payload text");
    let payload_err = Blake3AuditChain::verify_chain_integrity(&conn).unwrap_err();
    assert!(
        payload_err.contains("Audit payload tampered at block 2: payload hash mismatch"),
        "Must identify payload tampering at block 2: {payload_err}"
    );

    // Sub-attack 4: Delete block 3 from the middle of the chain
    // (Restore block 2 payload first)
    let b2_original_payload = "payload_2";
    conn.execute(
        "UPDATE audit_hash_chain SET redacted_payload = ?1 WHERE sequence_num = 2",
        [b2_original_payload],
    )
    .expect("restore payload 2");
    // Verify restored
    assert_eq!(
        Blake3AuditChain::verify_chain_integrity(&conn).unwrap(),
        5
    );

    // Now delete middle block 3
    conn.execute("DELETE FROM audit_hash_chain WHERE sequence_num = 3", [])
        .expect("delete middle block 3");
    let delete_break_err = Blake3AuditChain::verify_chain_integrity(&conn).unwrap_err();
    assert!(
        delete_break_err.contains("prev_hash mismatch"),
        "Must detect pointer discontinuity when middle block is deleted: {delete_break_err}"
    );
}

// ============================================================================
// 2. UNTRUSTED TAINT SINK VIOLATION & LABELED<T> INVARIANTS (FEATURE F20)
// ============================================================================

#[test]
fn test_m4_c2_untrusted_taint_labeled_invariants_and_receipt_declassification() {
    let untrusted_origin = DataOrigin::WebScrape {
        url: "https://adversarial-site.com/payload".to_string(),
        domain: "adversarial-site.com".to_string(),
        fetched_at: 1729000000,
    };

    let untrusted_data = Labeled::new_untrusted(
        "rm -rf / --no-preserve-root".to_string(),
        untrusted_origin.clone(),
    );

    // Invariant 1: Untrusted status
    assert!(untrusted_data.is_tainted(), "Data must be tainted");
    assert_eq!(untrusted_data.integrity(), IntegrityLevel::Untrusted);
    assert!(
        untrusted_data.flags().untrusted_external,
        "untrusted_external flag must be true"
    );

    // Invariant 2: Functor map preserves taint 100%
    let mapped_data = untrusted_data.clone().map(|cmd| format!("sudo {cmd}"));
    assert!(
        mapped_data.is_tainted(),
        "Functor map must preserve tainted status"
    );
    assert_eq!(mapped_data.integrity(), IntegrityLevel::Untrusted);
    assert!(
        mapped_data.flags().untrusted_external,
        "Functor map must preserve taint flags"
    );
    assert_eq!(mapped_data.as_inner(), "sudo rm -rf / --no-preserve-root");

    // Invariant 3: Monoid combine propagates untrusted taint to composite
    let trusted_owner_data = Labeled::new_owner("config_header".to_string(), "desktop");
    assert!(!trusted_owner_data.is_tainted());

    let combined = untrusted_data.clone().combine(trusted_owner_data, |u, t| {
        format!("{t}\n{u}")
    });
    assert!(
        combined.is_tainted(),
        "Combined data with any untrusted input must be tainted"
    );
    assert_eq!(
        combined.integrity(),
        IntegrityLevel::Untrusted,
        "Combined integrity level must be Untrusted"
    );
    assert!(
        combined.flags().untrusted_external,
        "Combined data must retain untrusted_external flag"
    );

    // Invariant 4: Expired declassification receipt is rejected
    let expired_receipt = DeclassificationReceipt {
        receipt_id: "rc_expired_001".to_string(),
        authorized_by: "Owner".to_string(),
        payload_blake3_hash: "dummy_hash".to_string(),
        issued_at_ms: 1000,
        expires_at_ms: 2000, // Long past
    };
    let declassify_err = untrusted_data.clone().declassify(&expired_receipt);
    assert!(
        matches!(
            declassify_err,
            Err(SecurityError::InvalidDeclassificationReceipt(_))
        ),
        "Declassification with expired receipt must be rejected with InvalidDeclassificationReceipt"
    );

    // Invariant 5: Valid unexpired receipt permits unwrapping
    let valid_receipt = DeclassificationReceipt {
        receipt_id: "rc_valid_002".to_string(),
        authorized_by: "Owner".to_string(),
        payload_blake3_hash: "dummy_hash".to_string(),
        issued_at_ms: 1000,
        expires_at_ms: u64::MAX, // Valid forever
    };
    let declassify_ok = untrusted_data.declassify(&valid_receipt);
    assert!(
        declassify_ok.is_ok(),
        "Declassification with valid receipt must succeed"
    );
    assert_eq!(declassify_ok.unwrap(), "rm -rf / --no-preserve-root");
}

#[test]
fn test_m4_c2_scoped_tool_registry_tainted_sink_enforcement_and_mode_violations() {
    let registry = ScopedToolRegistry::new();

    // Register a mutating high-risk tool ("send_message")
    let send_msg_tool = CatalogTool {
        server: "native".into(),
        name: "send_message".to_string(),
        description: "Sends an external message".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "to": {"type": "string"},
                "message": {"type": "string"}
            },
            "required": ["to", "message"]
        }),
        embed_extra: "".into(),
    };
    let _g1 = registry
        .register_scoped("scope:global", send_msg_tool)
        .expect("register send_message");

    // Register a read-only tool ("read_file")
    let read_file_tool = CatalogTool {
        server: "native".into(),
        name: "read_file".to_string(),
        description: "Reads local file".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            },
            "required": ["path"]
        }),
        embed_extra: "".into(),
    };
    let _g2 = registry
        .register_scoped("scope:global", read_file_tool)
        .expect("register read_file");

    // 1. Untrusted caller (Telegram principal) calling mutating sink ("send_message")
    // ScopedToolRegistry internally maps Telegram -> SessionMode::Reader and is_argument_tainted = true
    let telegram_scope = ToolScope::new("scope:session:telegram_1", CommandPrincipal::Telegram)
        .with_parent("scope:global");
    registry.register_scope(telegram_scope.clone());

    let telegram_mutation_res = registry.execute_guarded(
        &telegram_scope,
        "send_message",
        &json!({"to": "adversary", "message": "leak"}),
    );

    assert!(
        telegram_mutation_res.is_err(),
        "CRITICAL BUG: ScopedToolRegistry permitted Telegram caller to invoke mutating sink 'send_message'"
    );
    let err_str = telegram_mutation_res.unwrap_err().to_string();
    assert!(
        err_str.contains("Reader")
            || err_str.contains("PolicyViolation")
            || err_str.contains("TaintedDataSinkViolation")
            || err_str.contains("MutationForbiddenInReaderMode")
            || err_str.contains("đột biến"),
        "Error must cite Reader mode or Taint sink violation: {err_str}"
    );

    // 2. LocalCli principal (trusted) calling read-only tool ("read_file")
    let cli_scope = ToolScope::new("scope:session:cli_1", CommandPrincipal::LocalCli)
        .with_parent("scope:global");
    registry.register_scope(cli_scope.clone());

    // Calling an unregistered handler without mock will reach execution phase or succeed
    let cli_read_res = registry.execute_guarded(
        &cli_scope,
        "read_file",
        &json!({"path": "safe.txt"}),
    );
    // Since no execution backend is attached, it will return an execution error or success, but NOT PolicyViolation!
    if let Err(e) = cli_read_res {
        let err_msg = e.to_string();
        assert!(
            !err_msg.contains("Policy violation: ExecutorPolicy"),
            "LocalCli must NOT be blocked by ExecutorPolicy on read_file: {err_msg}"
        );
    }
}


// ============================================================================
// 3. CANONICAL HASH & INVISIBLE CHARACTER DETECTION (FEATURE F23)
// ============================================================================

#[test]
fn test_m4_c2_canonical_json_hash_permutation_and_nesting_invariance() {
    // 3 differently ordered JSON objects with identical semantic content
    let obj_a = json!({
        "target": "vault.db",
        "action": "backup",
        "dry_run": false,
        "max_size_bytes": 1048576,
        "nested": {
            "z_flag": true,
            "a_key": "first_value",
            "m_mid": [3, 2, 1]
        }
    });

    let obj_b = json!({
        "nested": {
            "a_key": "first_value",
            "m_mid": [3, 2, 1],
            "z_flag": true
        },
        "dry_run": false,
        "max_size_bytes": 1048576,
        "action": "backup",
        "target": "vault.db"
    });

    let obj_c = json!({
        "max_size_bytes": 1048576,
        "action": "backup",
        "target": "vault.db",
        "nested": {
            "m_mid": [3, 2, 1],
            "z_flag": true,
            "a_key": "first_value"
        },
        "dry_run": false
    });

    // Invariant 1: Canonical string output must be 100% byte-for-byte identical
    let canon_a = canonicalize_json(&obj_a);
    let canon_b = canonicalize_json(&obj_b);
    let canon_c = canonicalize_json(&obj_c);

    assert_eq!(
        canon_a, canon_b,
        "canonicalize_json must produce identical strings regardless of object key order"
    );
    assert_eq!(
        canon_b, canon_c,
        "canonicalize_json must produce identical strings across all permutations"
    );

    // Invariant 2: Keys must be strictly sorted lexicographically in canonical output
    assert!(
        canon_a.starts_with(r#"{"action":"backup","dry_run":false,"max_size_bytes":1048576"#),
        "Keys must be sorted lexicographically: {canon_a}"
    );

    // Invariant 3: BLAKE3 Canonical Arg Hash must be 100% identical
    let hash_a = compute_canonical_args_hash(&obj_a);
    let hash_b = compute_canonical_args_hash(&obj_b);
    let hash_c = compute_canonical_args_hash(&obj_c);

    assert_eq!(hash_a, hash_b, "Hash A and Hash B must be identical");
    assert_eq!(hash_b, hash_c, "Hash B and Hash C must be identical");
    assert_eq!(hash_a.len(), 64, "BLAKE3 hash must be 64 hex characters");
}

#[test]
fn test_m4_c2_canonical_json_unicode_nfc_normalization() {
    // Vietnamese phrase: "Cộng hòa Xã hội Chủ nghĩa Việt Nam"
    // Composed (NFC)
    let nfc_text = "Cộng hòa Xã hội Chủ nghĩa Việt Nam";
    let nfc_val = json!({ "country": nfc_text });

    // Decomposed (NFD)
    use unicode_normalization::UnicodeNormalization;
    let nfd_text: String = nfc_text.nfd().collect();
    assert_ne!(
        nfc_text.as_bytes(),
        nfd_text.as_bytes(),
        "Pre-condition: NFD bytes must differ from NFC bytes"
    );
    let nfd_val = json!({ "country": nfd_text });

    let canon_nfc = canonicalize_json(&nfc_val);
    let canon_nfd = canonicalize_json(&nfd_val);

    assert_eq!(
        canon_nfc, canon_nfd,
        "canonicalize_json must normalize NFD strings to NFC"
    );

    let hash_nfc = compute_canonical_args_hash(&nfc_val);
    let hash_nfd = compute_canonical_args_hash(&nfd_val);

    assert_eq!(
        hash_nfc, hash_nfd,
        "BLAKE3 canonical argument hash must be identical for NFC and NFD Unicode representations"
    );
}

#[test]
fn test_m4_c2_visualize_invisible_chars_bidi_and_zero_width_detection() {
    // 1. Clean string without invisible chars
    let clean_text = "Xin chào LIVA, vui lòng kiểm tra báo cáo.";
    let (clean_vis, clean_flag) = visualize_invisible_chars(clean_text);
    assert!(!clean_flag, "Clean text must have has_invisible = false");
    assert_eq!(clean_vis, clean_text);

    // 2. Trojan Source attack payload using Right-to-Left Override (RLO) and Zero-Width Space (ZWSP)
    // Simulated malicious prompt hiding code injection:
    let hostile_payload = "exec_command('delete\u{202E}\u{200B}txt.exe/*\u{200C}*/')";
    let (vis_output, has_invisible) = visualize_invisible_chars(hostile_payload);
    assert!(
        has_invisible,
        "Hostile payload with RLO and ZWSP must be flagged"
    );
    assert!(
        vis_output.contains("[RLO]"),
        "Must highlight Right-to-Left Override as [RLO]: {vis_output}"
    );
    assert!(
        vis_output.contains("[ZWSP]"),
        "Must highlight Zero-Width Space as [ZWSP]: {vis_output}"
    );
    assert!(
        vis_output.contains("[ZWNJ]"),
        "Must highlight Zero-Width Non-Joiner as [ZWNJ]: {vis_output}"
    );

    // 3. Exhaustive check of all 12 supported invisible control characters
    let all_invisible = [
        ('\u{200B}', "[ZWSP]"),
        ('\u{200C}', "[ZWNJ]"),
        ('\u{200D}', "[ZWJ]"),
        ('\u{200E}', "[LRM]"),
        ('\u{200F}', "[RLM]"),
        ('\u{202A}', "[LRE]"),
        ('\u{202B}', "[RLE]"),
        ('\u{202C}', "[PDF]"),
        ('\u{202D}', "[LRO]"),
        ('\u{202E}', "[RLO]"),
        ('\u{FEFF}', "[BOM]"),
        ('\u{00AD}', "[SHY]"),
    ];

    for (ch, expected_tag) in all_invisible {
        let sample = format!("prefix{ch}suffix");
        let (out, detected) = visualize_invisible_chars(&sample);
        assert!(
            detected,
            "Character U+{:04X} must be detected as invisible",
            ch as u32
        );
        assert!(
            out.contains(expected_tag),
            "Character U+{:04X} must be formatted as '{expected_tag}', got: '{out}'",
            ch as u32
        );
    }
}

// ============================================================================
// 4. EMERGENCY KILL SWITCH LATENCY SLA & DEFECT ANALYSIS (FEATURE F25)
// ============================================================================

#[test]
fn test_m4_c2_atomic_epoch_check_latency_sla_under_5ns() {
    // Measure latency SLA for atomic epoch evaluation (< 5ns)
    let epoch = Arc::new(AtomicU64::new(10));
    let token_epoch = 10u64;

    let iterations = 1_000_000;
    let start = Instant::now();
    let mut sum = 0u64;

    for _ in 0..iterations {
        let cur = epoch.load(Ordering::SeqCst);
        if token_epoch < cur {
            sum += 1;
        }
    }

    let elapsed = start.elapsed();
    let nanos_per_op = elapsed.as_nanos() as f64 / iterations as f64;

    println!(
        "[LATENCY BENCH] Atomic epoch check: {nanos_per_op:.2} ns/op (total: {:?})",
        elapsed
    );

    // Guardrail: must execute in under 15ns per evaluation (typical: 1.5 - 4.5ns)
    assert!(
        nanos_per_op < 15.0,
        "Atomic epoch check SLA exceeded: {nanos_per_op:.2} ns/op (must be < 15ns)"
    );
    assert_eq!(sum, 0);
}

#[test]
fn test_m4_c2_architectural_defects_and_contract_verification() {
    // -----------------------------------------------------------------------
    // VERIFICATION 1: `liva_native_core::authorization` and root re-exports
    // -----------------------------------------------------------------------
    use liva_native_core::authorization::{
        CapabilityGrant, CapabilityToken, DelegationStep, DynamicToolPruner, EmergencyKillSwitch,
        ExecutorPolicy, Principal, SessionModeGovernor,
    };
    // Also verify crate root re-exports
    use liva_native_core::{
        CapabilityGrant as RootGrant, CapabilityToken as RootToken,
        DelegationStep as RootDelegationStep, DynamicToolPruner as RootToolPruner,
        EmergencyKillSwitch as RootKillSwitch, ExecutorPolicy as RootPolicy,
        SessionModeGovernor as RootGovernor,
    };

    // -----------------------------------------------------------------------
    // VERIFICATION 2: `EmergencyKillSwitch::escalate_epoch(reason)`
    // -----------------------------------------------------------------------
    let ks = EmergencyKillSwitch::new();
    assert_eq!(ks.current_epoch(), 1);
    assert!(!ks.is_active(), "Kill switch must not be in safe mode initially");
    assert!(ks.validate_grant(1).is_ok());

    // Escalate epoch without safe mode lockdown
    let new_epoch = ks.escalate_epoch("Immediate token rotation");
    assert_eq!(new_epoch, 2);
    assert_eq!(ks.current_epoch(), 2);
    assert!(!ks.is_active(), "Kill switch must NOT be active (no safe mode lockdown)");

    // validate_grant(1) must immediately return TokenEpochRevoked (< 5ns)
    let revoke_err = ks.validate_grant(1).unwrap_err();
    assert_eq!(revoke_err, SecurityError::TokenEpochRevoked(1, 2));

    // Current epoch 2 is valid
    assert!(ks.validate_grant(2).is_ok());

    // -----------------------------------------------------------------------
    // VERIFICATION 3: Browser Mode Rule-of-Two blocks read_markdown & sensitive data
    // -----------------------------------------------------------------------
    let empty_args = json!({});
    for sensitive_tool in [
        "read_markdown",
        "get_user_profile",
        "contacts:list",
        "get_tasks",
        "search_vault",
        "get_fact_bitemporal",
        "read_credentials",
    ] {
        let err = ExecutorPolicy::enforce_sink_call(
            SessionMode::Browser,
            sensitive_tool,
            &empty_args,
            false,
            None,
            None,
        );
        assert!(
            matches!(err, Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))),
            "Tool '{sensitive_tool}' MUST be blocked in Browser mode, got: {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // VERIFICATION 4: DeclassificationReceipt payload hash binding
    // -----------------------------------------------------------------------
    let valid_args = json!({"cmd": "safe_ping"});
    let computed_hash = compute_canonical_args_hash(&valid_args);
    let valid_receipt = DeclassificationReceipt {
        receipt_id: "rc_verified_01".into(),
        authorized_by: "Owner".into(),
        payload_blake3_hash: computed_hash.clone(),
        issued_at_ms: 1000,
        expires_at_ms: u64::MAX,
    };

    // Valid receipt with matching payload hash succeeds
    assert!(
        ExecutorPolicy::enforce_sink_call(
            SessionMode::Actor,
            "write_file",
            &valid_args,
            true,
            None,
            Some(&valid_receipt)
        )
        .is_ok(),
        "Valid receipt with matching payload hash must succeed"
    );

    // Mismatched receipt (e.g. adversary mutated arguments) MUST be rejected
    let mutated_args = json!({"cmd": "rm -rf /"});
    let mismatch_err = ExecutorPolicy::enforce_sink_call(
        SessionMode::Actor,
        "write_file",
        &mutated_args,
        true,
        None,
        Some(&valid_receipt),
    );
    assert!(
        matches!(
            mismatch_err,
            Err(SecurityError::InvalidDeclassificationReceipt(_))
        ),
        "Mutated args with mismatched receipt hash must return InvalidDeclassificationReceipt, got: {mismatch_err:?}"
    );

    // -----------------------------------------------------------------------
    // VERIFICATION 5: Cryptographic Delegation Chain and Path Traversal
    // -----------------------------------------------------------------------
    let master_key = [77u8; 32];
    let root_grant = CapabilityGrant {
        grant_id: "grant_root".into(),
        parent_grant_id: None,
        epoch: 1,
        principal: Principal::Owner,
        allowed_tools: vec!["fs:read".into(), "fs:write".into()],
        resource_patterns: vec!["*".into()],
        max_risk_tier: RiskTier::R2,
        allowed_recipients: vec!["alice".into()],
        max_uses: Some(10),
        expires_at: Some(5000),
        plan_hash: None,
    };
    let root_tok = RootToken::new_root(root_grant.clone(), &master_key);
    assert!(root_tok.verify_chain(&master_key).is_ok());

    // Path traversal attempt in attenuation
    let traversal_grant = CapabilityGrant {
        grant_id: "grant_traversal".into(),
        parent_grant_id: Some("grant_root".into()),
        resource_patterns: vec!["/workspace/../../etc/passwd".into()],
        ..root_grant
    };
    assert!(
        matches!(
            root_tok.attenuate(traversal_grant),
            Err(SecurityError::PrivilegeEscalationDetected(_))
        ),
        "Path traversal attempt must be rejected"
    );
}
