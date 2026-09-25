//! Agent Graph, State Machine, and Swarm Test Suite

#[path = "../agent_graph_checkpoint_tests.rs"]
pub mod agent_graph_checkpoint_tests;
#[path = "../anti_hallucination_adversarial_stress.rs"]
pub mod anti_hallucination_adversarial_stress;
#[path = "../continuous_stress_1000_rounds.rs"]
pub mod continuous_stress_1000_rounds;
#[path = "../m1_challenger2_integration.rs"]
pub mod m1_challenger2_integration;
#[path = "../m1_resilience_adversarial_challenge.rs"]
pub mod m1_resilience_adversarial_challenge;
#[path = "../m4_adversarial_stress_challenge.rs"]
pub mod m4_adversarial_stress_challenge;
#[path = "../m4_checkpoint_adversarial_challenge.rs"]
pub mod m4_checkpoint_adversarial_challenge;
#[path = "../m4_escalation_adversarial_challenge.rs"]
pub mod m4_escalation_adversarial_challenge;
#[cfg(feature = "experimental")]
#[path = "../self_correction_stress.rs"]
pub mod self_correction_stress;
#[path = "../swarm_stress_tests.rs"]
pub mod swarm_stress_tests;
