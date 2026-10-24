//! Turn-Level Principal Identification and Provenance (Feature F21).

pub use liva_core_types::{AuthorityTier, ContactTrustLevel, Principal};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrincipalProvenance {
    pub principal: Principal,
    pub authenticated_at: u64,
    pub channel_token_or_sig: Option<String>,
    pub client_process_pid: Option<u32>,
}
