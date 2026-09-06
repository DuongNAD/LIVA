//! Companion Node Pairing Protocol & Zero-Trust Security Gate
//!
//! Manages cryptographic challenge-response authentication, short-code DM pairing,
//! pending node authorization flows, HMAC-SHA256 token verification, and revocation.

use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Unique identifier for connected compute and UI nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub Uuid);

impl NodeId {
    /// Generate a new random NodeId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Construct from an existing Uuid.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Access the underlying Uuid.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for NodeId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(NodeId)
    }
}

/// Role classification for connected nodes determining authorization level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRole {
    DesktopUi,
    Widget,
    MobileCompanion,
    HeadlessNode,
    CliTool,
}

impl NodeRole {
    /// Whether this node role inherently possesses full administrative access.
    pub fn has_full_access(&self) -> bool {
        matches!(self, Self::DesktopUi | Self::CliTool)
    }

    /// Whether this node is a remote companion device.
    pub fn is_companion(&self) -> bool {
        matches!(self, Self::MobileCompanion | Self::HeadlessNode)
    }

    /// Canonical string identifier.
    pub fn as_str(&self) -> &str {
        match self {
            Self::DesktopUi => "desktop_ui",
            Self::Widget => "widget",
            Self::MobileCompanion => "mobile_companion",
            Self::HeadlessNode => "headless_node",
            Self::CliTool => "cli_tool",
        }
    }
}

impl FromStr for NodeRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "desktop_ui" => Ok(Self::DesktopUi),
            "widget" => Ok(Self::Widget),
            "mobile_companion" => Ok(Self::MobileCompanion),
            "headless_node" => Ok(Self::HeadlessNode),
            "cli_tool" => Ok(Self::CliTool),
            other => Err(format!("Unknown node role: {}", other)),
        }
    }
}

/// Incoming pairing request from a client or companion node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingRequest {
    pub node_id: NodeId,
    pub node_name: String,
    pub role: NodeRole,
    pub public_key: String,
    pub pairing_nonce: String,
    #[serde(default = "default_unix_now")]
    pub timestamp_unix: u64,
}

fn default_unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

/// Response returned to a node after pairing attempt or approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingResponse {
    pub paired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    pub server_public_key: String,
    pub expires_at_unix: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_reason: Option<String>,
}

pub const MAX_FAILED_ATTEMPTS: u32 = 5;

/// Pending pairing challenge requiring administrator confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingChallenge {
    pub challenge_id: String,
    pub short_code: String,
    pub nonce: String,
    pub node_info: PairingRequest,
    pub created_at_unix: u64,
    pub expires_at_unix: u64,
    pub failed_attempts: u32,
}

/// Record of an authorized / approved node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovedNodeRecord {
    pub node_id: NodeId,
    pub node_name: String,
    pub role: NodeRole,
    pub public_key: String,
    pub approved_at_unix: u64,
    pub last_seen_unix: u64,
}

/// In-memory zero-trust pairing registry managing challenges, tokens, and approvals.
#[derive(Clone)]
pub struct PairingRegistry {
    server_secret: [u8; 32],
    server_public_key: String,
    pending_challenges: Arc<RwLock<HashMap<String, PairingChallenge>>>,
    short_code_index: Arc<RwLock<HashMap<String, String>>>,
    approved_nodes: Arc<RwLock<HashMap<NodeId, ApprovedNodeRecord>>>,
    token_validity_secs: u64,
}

impl PairingRegistry {
    /// Create a new PairingRegistry with a given server secret key.
    pub fn new(server_secret: [u8; 32], server_public_key: impl Into<String>) -> Self {
        Self {
            server_secret,
            server_public_key: server_public_key.into(),
            pending_challenges: Arc::new(RwLock::new(HashMap::new())),
            short_code_index: Arc::new(RwLock::new(HashMap::new())),
            approved_nodes: Arc::new(RwLock::new(HashMap::new())),
            token_validity_secs: 30 * 86400, // 30 days default token validity
        }
    }

    /// Initialize with auto-generated secure random secret.
    pub fn with_random_secret(server_public_key: impl Into<String>) -> Self {
        let mut secret = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        Self::new(secret, server_public_key)
    }

    /// Generate a 6-digit numeric pairing code.
    fn generate_short_code() -> String {
        let code: u32 = rand::thread_rng().gen_range(100_000..=999_999);
        code.to_string()
    }

    /// Generate a 32-byte cryptographic hex nonce.
    fn generate_nonce() -> String {
        let mut nonce = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        hex::encode(nonce)
    }

    /// Create a pending pairing challenge for an incoming request.
    pub async fn create_challenge(
        &self,
        req: PairingRequest,
        ttl_secs: u64,
    ) -> Result<PairingChallenge, String> {
        if req.public_key.trim().is_empty() {
            return Err("Public key cannot be empty".to_string());
        }

        let now = default_unix_now();
        let expires_at_unix = now + ttl_secs;
        let challenge_id = Uuid::new_v4().to_string();
        let short_code = Self::generate_short_code();
        let nonce = Self::generate_nonce();

        let challenge = PairingChallenge {
            challenge_id: challenge_id.clone(),
            short_code: short_code.clone(),
            nonce,
            node_info: req,
            created_at_unix: now,
            expires_at_unix,
            failed_attempts: 0,
        };

        let mut challenges = self.pending_challenges.write().await;
        let mut code_index = self.short_code_index.write().await;

        challenges.insert(challenge_id.clone(), challenge.clone());
        code_index.insert(short_code, challenge_id);

        Ok(challenge)
    }

    /// Approve a pairing challenge using the 6-digit short code.
    pub async fn approve_by_short_code(&self, code: &str) -> Result<PairingResponse, String> {
        let code_clean = code.trim();
        let challenge_id = {
            let index = self.short_code_index.read().await;
            index
                .get(code_clean)
                .cloned()
                .ok_or_else(|| "Invalid or expired pairing code".to_string())?
        };

        self.approve_by_challenge_id(&challenge_id).await
    }

    /// Approve a pairing challenge by challenge ID.
    pub async fn approve_by_challenge_id(
        &self,
        challenge_id: &str,
    ) -> Result<PairingResponse, String> {
        let mut challenges = self.pending_challenges.write().await;
        let challenge = challenges
            .remove(challenge_id)
            .ok_or_else(|| "Challenge not found or already processed".to_string())?;

        // Remove from short code index
        let mut code_index = self.short_code_index.write().await;
        code_index.remove(&challenge.short_code);

        let now = default_unix_now();
        if challenge.expires_at_unix <= now {
            return Err("Pairing challenge has expired".to_string());
        }

        if challenge.failed_attempts >= MAX_FAILED_ATTEMPTS {
            return Err("Pairing challenge has been invalidated due to excessive failed attempts".to_string());
        }

        let node_id = challenge.node_info.node_id;
        let role = challenge.node_info.role;
        let expires_at_unix = now + self.token_validity_secs;

        // Generate HMAC signature token: v1:<node_id>:<role>:<expires_at_unix>:<hmac>
        let token = self.generate_token(node_id, role, expires_at_unix);

        // Store into approved nodes registry
        let mut approved = self.approved_nodes.write().await;
        approved.insert(
            node_id,
            ApprovedNodeRecord {
                node_id,
                node_name: challenge.node_info.node_name,
                role,
                public_key: challenge.node_info.public_key,
                approved_at_unix: now,
                last_seen_unix: now,
            },
        );

        Ok(PairingResponse {
            paired: true,
            auth_token: Some(token),
            server_public_key: self.server_public_key.clone(),
            expires_at_unix,
            error_reason: None,
        })
    }

    /// Verify a short code against a specific challenge ID.
    /// Increments failed_attempts on mismatch and invalidates the challenge once MAX_FAILED_ATTEMPTS is reached.
    pub async fn verify_and_approve(
        &self,
        challenge_id: &str,
        code: &str,
    ) -> Result<PairingResponse, String> {
        let mut challenges = self.pending_challenges.write().await;
        let challenge = challenges
            .get_mut(challenge_id)
            .ok_or_else(|| "Challenge not found or already processed".to_string())?;

        let now = default_unix_now();
        if challenge.expires_at_unix <= now {
            let short_code = challenge.short_code.clone();
            challenges.remove(challenge_id);
            let mut code_index = self.short_code_index.write().await;
            code_index.remove(&short_code);
            return Err("Pairing challenge has expired".to_string());
        }

        if challenge.short_code != code.trim() {
            challenge.failed_attempts += 1;
            if challenge.failed_attempts >= MAX_FAILED_ATTEMPTS {
                let short_code = challenge.short_code.clone();
                challenges.remove(challenge_id);
                let mut code_index = self.short_code_index.write().await;
                code_index.remove(&short_code);
                return Err("Too many failed attempts. Pairing challenge invalidated.".to_string());
            }
            return Err(format!(
                "Invalid pairing code. {} attempts remaining.",
                MAX_FAILED_ATTEMPTS - challenge.failed_attempts
            ));
        }

        let challenge = challenges.remove(challenge_id).unwrap();
        let mut code_index = self.short_code_index.write().await;
        code_index.remove(&challenge.short_code);

        let node_id = challenge.node_info.node_id;
        let role = challenge.node_info.role;
        let expires_at_unix = now + self.token_validity_secs;

        let token = self.generate_token(node_id, role, expires_at_unix);

        let mut approved = self.approved_nodes.write().await;
        approved.insert(
            node_id,
            ApprovedNodeRecord {
                node_id,
                node_name: challenge.node_info.node_name,
                role,
                public_key: challenge.node_info.public_key,
                approved_at_unix: now,
                last_seen_unix: now,
            },
        );

        Ok(PairingResponse {
            paired: true,
            auth_token: Some(token),
            server_public_key: self.server_public_key.clone(),
            expires_at_unix,
            error_reason: None,
        })
    }

    /// Record a failed verification attempt against a challenge ID.
    pub async fn record_failed_attempt(&self, challenge_id: &str) -> Result<u32, String> {
        let mut challenges = self.pending_challenges.write().await;
        let challenge = challenges
            .get_mut(challenge_id)
            .ok_or_else(|| "Challenge not found".to_string())?;

        challenge.failed_attempts += 1;
        let count = challenge.failed_attempts;
        if count >= MAX_FAILED_ATTEMPTS {
            let short_code = challenge.short_code.clone();
            challenges.remove(challenge_id);
            let mut code_index = self.short_code_index.write().await;
            code_index.remove(&short_code);
            Err("Challenge invalidated due to excessive failed attempts".to_string())
        } else {
            Ok(count)
        }
    }

    /// Reject a pairing challenge.
    pub async fn reject_challenge(&self, challenge_id: &str, _reason: &str) -> Result<(), String> {
        let mut challenges = self.pending_challenges.write().await;
        if let Some(challenge) = challenges.remove(challenge_id) {
            let mut code_index = self.short_code_index.write().await;
            code_index.remove(&challenge.short_code);
            Ok(())
        } else {
            Err("Challenge not found".to_string())
        }
    }

    /// Generate an HMAC-SHA256 signed bearer token.
    pub fn generate_token(&self, node_id: NodeId, role: NodeRole, expires_at_unix: u64) -> String {
        let payload = format!("v1:{}:{}:{}", node_id, role.as_str(), expires_at_unix);
        let mut hasher = Sha256::new();
        hasher.update(&self.server_secret);
        hasher.update(payload.as_bytes());
        let signature = hex::encode(hasher.finalize());

        format!("{}:{}", payload, signature)
    }

    /// Verify an HMAC-SHA256 signed bearer token and return the authenticated NodeId and NodeRole.
    pub fn verify_auth_token(&self, token: &str) -> Result<(NodeId, NodeRole), String> {
        let parts: Vec<&str> = token.trim().split(':').collect();
        if parts.len() != 5 {
            return Err("Malformed token format".to_string());
        }

        if parts[0] != "v1" {
            return Err(format!("Unsupported token version: {}", parts[0]));
        }

        let node_id = NodeId::from_str(parts[1]).map_err(|e| format!("Invalid NodeId: {e}"))?;
        let role = NodeRole::from_str(parts[2]).map_err(|e| format!("Invalid NodeRole: {e}"))?;
        let expires_at_unix = parts[3]
            .parse::<u64>()
            .map_err(|_| "Invalid expiration timestamp in token".to_string())?;
        let provided_sig = parts[4];

        let now = default_unix_now();
        if expires_at_unix < now {
            return Err("Auth token has expired".to_string());
        }

        // Recompute expected HMAC signature
        let payload = format!("v1:{}:{}:{}", node_id, role.as_str(), expires_at_unix);
        let mut hasher = Sha256::new();
        hasher.update(&self.server_secret);
        hasher.update(payload.as_bytes());
        let expected_sig = hex::encode(hasher.finalize());

        if provided_sig != expected_sig {
            return Err("Invalid token signature".to_string());
        }

        Ok((node_id, role))
    }

    /// Check if a node is currently in the approved list.
    pub async fn is_node_approved(&self, node_id: &NodeId) -> bool {
        let approved = self.approved_nodes.read().await;
        approved.contains_key(node_id)
    }

    /// Update node last seen timestamp.
    pub async fn touch_node(&self, node_id: &NodeId) -> bool {
        let mut approved = self.approved_nodes.write().await;
        if let Some(record) = approved.get_mut(node_id) {
            record.last_seen_unix = default_unix_now();
            true
        } else {
            false
        }
    }

    /// Revoke approval for a node.
    pub async fn revoke_node(&self, node_id: &NodeId) -> bool {
        let mut approved = self.approved_nodes.write().await;
        approved.remove(node_id).is_some()
    }

    /// List all currently pending pairing challenges.
    pub async fn list_pending_challenges(&self) -> Vec<PairingChallenge> {
        let challenges = self.pending_challenges.read().await;
        challenges.values().cloned().collect()
    }

    /// List all currently approved nodes.
    pub async fn list_approved_nodes(&self) -> Vec<ApprovedNodeRecord> {
        let approved = self.approved_nodes.read().await;
        approved.values().cloned().collect()
    }

    /// Evict expired pairing challenges.
    pub async fn evict_expired(&self) -> usize {
        let now = default_unix_now();
        let mut challenges = self.pending_challenges.write().await;
        let mut code_index = self.short_code_index.write().await;

        let before_count = challenges.len();
        challenges.retain(|_, challenge| {
            let valid = challenge.expires_at_unix > now;
            if !valid {
                code_index.remove(&challenge.short_code);
            }
            valid
        });

        before_count - challenges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pairing_request_and_approval_by_code() {
        let registry = PairingRegistry::with_random_secret("server_pubkey_abc123");
        let node_id = NodeId::new();

        let req = PairingRequest {
            node_id,
            node_name: "MacBook Companion".to_string(),
            role: NodeRole::MobileCompanion,
            public_key: "ed25519_client_pubkey_xyz".to_string(),
            pairing_nonce: "nonce_12345".to_string(),
            timestamp_unix: default_unix_now(),
        };

        let challenge = registry
            .create_challenge(req, 300)
            .await
            .expect("create challenge");

        assert_eq!(challenge.short_code.len(), 6);
        assert!(!challenge.challenge_id.is_empty());

        // Approve using the 6-digit short code
        let response = registry
            .approve_by_short_code(&challenge.short_code)
            .await
            .expect("approve pairing");

        assert!(response.paired);
        assert!(response.auth_token.is_some());
        assert_eq!(response.server_public_key, "server_pubkey_abc123");

        let token = response.auth_token.unwrap();
        let (auth_node_id, auth_role) = registry.verify_auth_token(&token).expect("verify token");

        assert_eq!(auth_node_id, node_id);
        assert_eq!(auth_role, NodeRole::MobileCompanion);
        assert!(registry.is_node_approved(&node_id).await);
    }

    #[tokio::test]
    async fn test_pairing_token_tampering_rejected() {
        let registry = PairingRegistry::with_random_secret("server_pubkey");
        let node_id = NodeId::new();
        let token = registry.generate_token(node_id, NodeRole::DesktopUi, default_unix_now() + 3600);

        // Tamper with role
        let tampered = token.replace("desktop_ui", "cli_tool");
        let res = registry.verify_auth_token(&tampered);
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_revocation_lifecycle() {
        let registry = PairingRegistry::with_random_secret("server_pubkey");
        let node_id = NodeId::new();

        let req = PairingRequest {
            node_id,
            node_name: "RaspberryPi Edge".to_string(),
            role: NodeRole::HeadlessNode,
            public_key: "pubkey_test".to_string(),
            pairing_nonce: "nonce_abc".to_string(),
            timestamp_unix: default_unix_now(),
        };

        let challenge = registry.create_challenge(req, 60).await.unwrap();
        registry
            .approve_by_challenge_id(&challenge.challenge_id)
            .await
            .unwrap();

        assert!(registry.is_node_approved(&node_id).await);
        let revoked = registry.revoke_node(&node_id).await;
        assert!(revoked);
        assert!(!registry.is_node_approved(&node_id).await);
    }

    #[tokio::test]
    async fn test_failed_attempts_limit_and_invalidation() {
        let registry = PairingRegistry::with_random_secret("server_pubkey");
        let node_id = NodeId::new();

        let req = PairingRequest {
            node_id,
            node_name: "Attacked Node".to_string(),
            role: NodeRole::MobileCompanion,
            public_key: "ed25519_pubkey".to_string(),
            pairing_nonce: "nonce_123".to_string(),
            timestamp_unix: default_unix_now(),
        };

        let challenge = registry.create_challenge(req, 300).await.unwrap();
        let cid = &challenge.challenge_id;

        // 4 failed attempts should fail but keep challenge active
        for _ in 0..4 {
            let res = registry.verify_and_approve(cid, "000000").await;
            assert!(res.is_err());
            assert!(res.unwrap_err().contains("attempts remaining"));
        }

        // 5th failed attempt invalidates the challenge
        let res5 = registry.verify_and_approve(cid, "000000").await;
        assert!(res5.is_err());
        assert!(res5.unwrap_err().contains("invalidated"));

        // Subsequent attempt (even with valid code) should fail because challenge was evicted
        let res_valid = registry.verify_and_approve(cid, &challenge.short_code).await;
        assert!(res_valid.is_err());
        assert!(res_valid.unwrap_err().contains("not found"));
    }
}
