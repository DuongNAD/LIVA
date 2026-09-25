//! Consolidated Integration Test Harness for crates/liva-cua.
//! Respects sequential execution: cargo test -j 2 -p liva-cua -- --test-threads 2.

#[path = "suites/coordinate_math.rs"]
pub mod coordinate_math;

#[path = "suites/state_guards.rs"]
pub mod state_guards;

#[path = "suites/scroll_recovery.rs"]
pub mod scroll_recovery;

#[path = "suites/emergency_halt.rs"]
pub mod emergency_halt;

#[path = "suites/security_policy.rs"]
pub mod security_policy;

#[path = "suites/uipi_tests.rs"]
pub mod uipi_tests;

#[path = "suites/live_win32.rs"]
pub mod live_win32;

#[path = "suites/challenge_1_coordinate_guards.rs"]
pub mod challenge_1_coordinate_guards;

#[path = "suites/challenger2_adversarial.rs"]
pub mod challenger2_adversarial;

#[path = "suites/audit_tests.rs"]
pub mod audit_tests;

#[path = "suites/cua_m2_challenger2_tests.rs"]
pub mod cua_m2_challenger2_tests;

#[path = "suites/cua_m2_challenger_1.rs"]
pub mod cua_m2_challenger_1;

#[path = "suites/system1_router_tests.rs"]
pub mod system1_router_tests;

#[path = "suites/cua_m3_challenger2_tests.rs"]
pub mod cua_m3_challenger2_tests;

#[path = "suites/cua_m3_challenger_1.rs"]
pub mod cua_m3_challenger_1;

#[path = "suites/cua_m4_challenger2_tests.rs"]
pub mod cua_m4_challenger2_tests;
