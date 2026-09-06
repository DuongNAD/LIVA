//! Mid-Execution Consent Engine & Suspension Manager (Feature 12).
//!
//! Provides async mid-execution suspension (`AWAITING_CONSENT`), promise channels,
//! interactive approval routing, session scoping, and fail-closed timeout auto-rejection.

use super::manifest::RiskLevel;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, oneshot};

/// Consent requirement level for tools and actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConsentLevel {
    NeverForbidden,
    AskAlways,
    AskOncePerSession,
    #[default]
    AutoAllowSafe,
    AlwaysAllowExplicit,
}

/// Incoming consent request for an elevated or dangerous tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRequest {
    pub request_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub target_resource: String,
    pub risk_level: RiskLevel,
    pub arguments_preview: serde_json::Value,
}

/// The resolution decision of a consent evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentDecision {
    Approved {
        user_id: String,
        timestamp_unix: u64,
    },
    Denied {
        reason: String,
    },
    TimedOut,
}

/// Execution state of a consent evaluation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentStatus {
    AwaitingConsent,
    Approved,
    Denied,
    TimedOut,
}

/// Trait defining the contract for evaluating tool and action consent.
#[async_trait::async_trait]
pub trait ConsentAuthority: Send + Sync {
    async fn evaluate_consent(&self, request: ConsentRequest) -> Result<ConsentDecision, String>;
    fn check_fast_path(&self, tool_name: &str, risk: RiskLevel) -> bool;
}

/// In-memory asynchronous consent manager with promise channels and timeout guards.
#[derive(Clone)]
pub struct InMemoryConsentManager {
    pending_approvals: Arc<Mutex<HashMap<String, oneshot::Sender<ConsentDecision>>>>,
    pending_metadata: Arc<RwLock<HashMap<String, ConsentRequest>>>,
    session_approvals: Arc<RwLock<HashSet<(String, String)>>>,
    default_timeout: Duration,
}

impl Default for InMemoryConsentManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryConsentManager {
    pub fn new() -> Self {
        Self {
            pending_approvals: Arc::new(Mutex::new(HashMap::new())),
            pending_metadata: Arc::new(RwLock::new(HashMap::new())),
            session_approvals: Arc::new(RwLock::new(HashSet::new())),
            default_timeout: Duration::from_secs(60),
        }
    }

    pub fn with_default_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    /// Fast-path check: safe read-only operations do not require suspension.
    pub fn check_fast_path(&self, _tool_name: &str, risk: RiskLevel) -> bool {
        risk == RiskLevel::ReadOnlySafe
    }

    /// Suspend async execution pipeline until approved, denied, or timed out.
    pub async fn request_consent(
        &self,
        request_id: &str,
        risk_level: RiskLevel,
        timeout_duration: Duration,
    ) -> ConsentDecision {
        if self.check_fast_path("", risk_level) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            return ConsentDecision::Approved {
                user_id: "system_auto".to_string(),
                timestamp_unix: now,
            };
        }

        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending_approvals.lock().await;
            map.insert(request_id.to_string(), tx);
        }

        match tokio::time::timeout(timeout_duration, rx).await {
            Ok(Ok(decision)) => {
                let mut meta = self.pending_metadata.write().await;
                meta.remove(request_id);
                decision
            }
            _ => {
                let mut map = self.pending_approvals.lock().await;
                map.remove(request_id);
                let mut meta = self.pending_metadata.write().await;
                meta.remove(request_id);
                ConsentDecision::TimedOut
            }
        }
    }

    /// Interactive resolution (by UI, DM pairing, or administrator).
    pub async fn resolve_consent(&self, request_id: &str, decision: ConsentDecision) -> bool {
        let sender = {
            let mut map = self.pending_approvals.lock().await;
            map.remove(request_id)
        };

        if let Some(tx) = sender {
            if let ConsentDecision::Approved { .. } = &decision {
                let meta = self.pending_metadata.read().await;
                if let Some(req) = meta.get(request_id) {
                    let mut session_map = self.session_approvals.write().await;
                    session_map.insert((req.session_id.clone(), req.tool_name.clone()));
                }
            }
            let _ = tx.send(decision);
            true
        } else {
            false
        }
    }

    pub async fn list_pending(&self) -> Vec<ConsentRequest> {
        let meta = self.pending_metadata.read().await;
        meta.values().cloned().collect()
    }

    pub async fn pending_count(&self) -> usize {
        let map = self.pending_approvals.lock().await;
        map.len()
    }

    pub async fn clear_session(&self, session_id: &str) {
        let mut session_map = self.session_approvals.write().await;
        session_map.retain(|(s, _)| s != session_id);
    }
}

#[async_trait::async_trait]
impl ConsentAuthority for InMemoryConsentManager {
    async fn evaluate_consent(&self, request: ConsentRequest) -> Result<ConsentDecision, String> {
        if self.check_fast_path(&request.tool_name, request.risk_level) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            return Ok(ConsentDecision::Approved {
                user_id: "system_auto".to_string(),
                timestamp_unix: now,
            });
        }

        // Check session cached approval
        {
            let session_map = self.session_approvals.read().await;
            if session_map.contains(&(request.session_id.clone(), request.tool_name.clone())) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                return Ok(ConsentDecision::Approved {
                    user_id: "session_cached".to_string(),
                    timestamp_unix: now,
                });
            }
        }

        // Register metadata
        {
            let mut meta = self.pending_metadata.write().await;
            meta.insert(request.request_id.clone(), request.clone());
        }

        let decision = self
            .request_consent(&request.request_id, request.risk_level, self.default_timeout)
            .await;
        Ok(decision)
    }

    fn check_fast_path(&self, tool_name: &str, risk: RiskLevel) -> bool {
        InMemoryConsentManager::check_fast_path(self, tool_name, risk)
    }
}

/// Convenience alias maintaining full backwards compatibility with existing test harnesses.
pub type ConsentSuspender = InMemoryConsentManager;
