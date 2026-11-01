//! Empirical Challenger M3 Test Harness: Default Implementations & unwrap_unchecked Safety Contract
//!
//! Objectives:
//! 1. Adversarially verify canonical Default implementations for SessionQueueMode, SessionMode, Principal, IntegrityLevel.
//! 2. Adversarially test SessionTurnRequest::default() variant consistency.
//! 3. Adversarially test Labeled<T>::unwrap_unchecked() safety invariant, unsafe contract, and bypass behavior.

use liva_core_types::{
    AuthorityTier, DataOrigin, DeclassificationReceipt, IntegrityLevel, Labeled, Principal,
    SecurityError, SessionMode, SessionQueueMode, SessionTurnRequest,
};

#[test]
fn test_default_session_queue_mode_is_followup() {
    let def = SessionQueueMode::default();
    assert_eq!(
        def,
        SessionQueueMode::Followup,
        "SessionQueueMode default MUST be Followup"
    );
    assert_ne!(def, SessionQueueMode::Steer);
    assert_ne!(def, SessionQueueMode::Collect);
    assert_ne!(def, SessionQueueMode::Interrupt);

    // Verify Serde roundtrip matches "followup"
    let json = serde_json::to_string(&def).expect("serialization failed");
    assert_eq!(json, "\"followup\"");
    let de: SessionQueueMode = serde_json::from_str(&json).expect("deserialization failed");
    assert_eq!(de, SessionQueueMode::Followup);
}

#[test]
fn test_default_session_mode_is_actor() {
    let def = SessionMode::default();
    assert_eq!(
        def,
        SessionMode::Actor,
        "SessionMode default MUST be Actor (Vertex B+C)"
    );
    assert_ne!(def, SessionMode::Reader);
    assert_ne!(def, SessionMode::Browser);

    // Verify Serde roundtrip matches "actor"
    let json = serde_json::to_string(&def).expect("serialization failed");
    assert_eq!(json, "\"actor\"");
    let de: SessionMode = serde_json::from_str(&json).expect("deserialization failed");
    assert_eq!(de, SessionMode::Actor);
}

#[test]
fn test_default_principal_is_owner() {
    let def = Principal::default();
    assert_eq!(
        def,
        Principal::Owner,
        "Principal default MUST be Owner (highest authority)"
    );
    assert_eq!(def.authority_tier(), AuthorityTier::Owner);
    assert!(def.is_owner());

    // Verify Serde roundtrip matches {"kind":"owner"}
    let json = serde_json::to_string(&def).expect("serialization failed");
    assert_eq!(json, "{\"kind\":\"owner\"}");
    let de: Principal = serde_json::from_str(&json).expect("deserialization failed");
    assert_eq!(de, Principal::Owner);
}

#[test]
fn test_default_integrity_level_is_trusted() {
    let def = IntegrityLevel::default();
    assert_eq!(
        def,
        IntegrityLevel::Trusted,
        "IntegrityLevel default MUST be Trusted"
    );
    assert_eq!(def as u8, 0);

    // Verify monotonic ordering: Trusted < SemiTrusted < Untrusted < Quarantined
    assert!(IntegrityLevel::Trusted < IntegrityLevel::SemiTrusted);
    assert!(IntegrityLevel::SemiTrusted < IntegrityLevel::Untrusted);
    assert!(IntegrityLevel::Untrusted < IntegrityLevel::Quarantined);

    // Verify Serde roundtrip matches "trusted"
    let json = serde_json::to_string(&def).expect("serialization failed");
    assert_eq!(json, "\"trusted\"");
    let de: IntegrityLevel = serde_json::from_str(&json).expect("deserialization failed");
    assert_eq!(de, IntegrityLevel::Trusted);
}

#[test]
fn test_session_turn_request_defaults_cohere_with_enum_defaults() {
    let req = SessionTurnRequest::default();
    assert_eq!(req.queue_mode, SessionQueueMode::default());
    assert_eq!(req.session_mode, SessionMode::default());
    assert_eq!(req.turn_principal, Principal::default());
    assert_eq!(req.queue_mode, SessionQueueMode::Followup);
    assert_eq!(req.session_mode, SessionMode::Actor);
    assert_eq!(req.turn_principal, Principal::Owner);
}

#[test]
fn test_labeled_unwrap_unchecked_safety_contract_and_bypass() {
    let untrusted_origin = DataOrigin::WebScrape {
        url: "https://adversarial-payload.com/exploit".to_string(),
        domain: "adversarial-payload.com".to_string(),
        fetched_at: 1729000000,
    };

    let labeled = Labeled::new_untrusted(
        "DROP TABLE users; -- sensitive sql injection".to_string(),
        untrusted_origin.clone(),
    );

    // 1. Data is strongly tainted and untrusted
    assert!(labeled.is_tainted());
    assert_eq!(labeled.integrity(), IntegrityLevel::Untrusted);

    // 2. Safe declassification with invalid/unauthorized receipt MUST FAIL closed
    let invalid_receipt = DeclassificationReceipt {
        receipt_id: "fake_receipt".to_string(),
        authorized_by: "UntrustedActor".to_string(),
        payload_blake3_hash: "00000000000000000000000000000000".to_string(),
        issued_at_ms: 1000,
        expires_at_ms: 2000,
    };
    let declassify_res = labeled.clone().declassify(&invalid_receipt);
    assert!(
        matches!(
            declassify_res,
            Err(SecurityError::InvalidDeclassificationReceipt(_))
        ),
        "Safe declassify must reject invalid receipt"
    );

    // 3. Unsafe unwrap_unchecked strips the taint boundary directly without receipt validation
    // The caller assumes full responsibility according to `# Safety` documentation.
    let extracted_inner = unsafe { labeled.unwrap_unchecked() };
    assert_eq!(
        extracted_inner,
        "DROP TABLE users; -- sensitive sql injection",
        "unwrap_unchecked must accurately extract inner value when caller provides unsafe guarantee"
    );
}
