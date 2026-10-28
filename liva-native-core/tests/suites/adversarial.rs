//! Unified Adversarial & Challenger Integration Test Suite
//!
//! Consolidates cross-cutting empirical challenges and adversarial regression suites
//! into the single test harness target (`tests/harness.rs`).

#[path = "../m1_challenger2_adversarial_tests.rs"]
pub mod m1_challenger2_adversarial_tests;

#[path = "../sota_challenger_tests.rs"]
pub mod sota_challenger;

#[path = "../sota_challenger_rfc2_rfc3_tests.rs"]
pub mod sota_challenger_rfc2_rfc3;

#[path = "../empirical_m2_dsp_concurrency_simd_tests.rs"]
pub mod empirical_m2_dsp_concurrency_simd_tests;

#[path = "../sota_agentic_harness_integration_tests.rs"]
pub mod sota_harness;

#[path = "../m2_challenger2_insurance_prompt_challenge.rs"]
pub mod m2_challenger2_insurance_prompt_challenge;

#[path = "../m2_session_sidecar_adversarial_challenge.rs"]
pub mod m2_session_sidecar_adversarial_challenge;

#[path = "../m3_challenger1_bitemporal_invariants_adversarial.rs"]
pub mod m3_challenger1_bitemporal_invariants_adversarial;

#[path = "../cua_adversarial_stress.rs"]
pub mod cua_adversarial_stress;
#[path = "../m3_challenger2_memghost_fts_markdown_adversarial.rs"]
pub mod m3_challenger2_memghost_fts_markdown_adversarial;
#[path = "../m4_artemis_cua_mcp_adversarial_challenge.rs"]
pub mod m4_artemis_cua_mcp_adversarial_challenge;
#[path = "../m4_challenger1_rule_of_two_adversarial.rs"]
pub mod m4_challenger1_rule_of_two_adversarial;
#[path = "../m4_challenger2_security_adversarial_challenge.rs"]
pub mod m4_challenger2_security_adversarial_challenge;
