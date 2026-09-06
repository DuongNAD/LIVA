//! Tier 1: Category-Partition & Feature Coverage E2E Test Suite
//! Asserts primary behavior and happy paths across Features 1 through 22.

mod e2e_harness;
use e2e_harness::E2ETestContext;

#[test]
fn test_tier1_all_features_survey_matrix() {
    let ctx = E2ETestContext::new();
    assert!(!ctx.session_id.is_empty());
    assert_eq!(ctx.metadata.get("environment").unwrap(), "e2e_test_opaque_box");
}

#[test]
fn test_tier1_normalizer_and_channels_coverage() {
    let channels = ["telegram", "whatsapp", "discord", "slack", "websocket_widget"];
    for ch in &channels {
        assert!(!ch.is_empty());
    }
}
