//! Pairing Command Domain (Milestone 2 - Node Pairing Monitor)
//!
//! Provides IPC commands for inspecting, approving, rejecting, and revoking
//! companion compute nodes and mobile devices:
//! - `pairing:list` / `pairing:list_nodes`: Returns list of all active approved companion nodes.
//! - `pairing:list_pending`: Returns active short-code challenges awaiting admin approval.
//! - `pairing:approve`: Approves a pending node challenge via 6-digit short code or challenge ID.
//! - `pairing:reject`: Declines and evicts a pending challenge.
//! - `pairing:revoke`: Revokes authorization for an approved node.
//! - `pairing:create_challenge`: Generates a pending pairing challenge for testing/onboarding.

use crate::gateway::pairing::{NodeId, NodeRole, PairingRegistry, PairingRequest};
use crate::AppState;
use serde_json::{json, Value};
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const OWNED: &[&str] = &[
    "pairing:list",
    "pairing:list_nodes",
    "pairing:list_pending",
    "pairing:approve",
    "pairing:reject",
    "pairing:revoke",
    "pairing:create_challenge",
];

pub fn owns(command: &str) -> bool {
    OWNED.contains(&command)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn global_pairing_registry() -> &'static PairingRegistry {
    static REGISTRY: OnceLock<PairingRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        PairingRegistry::with_random_secret("liva_ed25519_core_server_public_key_v2")
    })
}

pub async fn handle(state: Arc<AppState>, command: &str, payload: Value) -> Result<Value, String> {
    let _ = &state;
    let registry = global_pairing_registry();

    match command {
        "pairing:list" | "pairing:list_nodes" => {
            let nodes = registry.list_approved_nodes().await;
            Ok(json!({
                "count": nodes.len(),
                "nodes": nodes.iter().map(|n| json!({
                    "nodeId": n.node_id.to_string(),
                    "nodeName": n.node_name,
                    "role": n.role.as_str(),
                    "publicKey": n.public_key,
                    "approvedAtUnix": n.approved_at_unix,
                    "lastSeenUnix": n.last_seen_unix,
                    "deviceType": match n.role {
                        NodeRole::MobileCompanion => "mobile",
                        NodeRole::DesktopUi => "desktop",
                        NodeRole::HeadlessNode => "server",
                        NodeRole::CliTool => "terminal",
                        NodeRole::Widget => "widget",
                    }
                })).collect::<Vec<_>>()
            }))
        }

        "pairing:list_pending" => {
            registry.evict_expired().await;
            let challenges = registry.list_pending_challenges().await;
            Ok(json!({
                "count": challenges.len(),
                "challenges": challenges.iter().map(|c| json!({
                    "challengeId": c.challenge_id,
                    "shortCode": c.short_code,
                    "nonce": c.nonce,
                    "nodeId": c.node_info.node_id.to_string(),
                    "nodeName": c.node_info.node_name,
                    "role": c.node_info.role.as_str(),
                    "publicKey": c.node_info.public_key,
                    "createdAtUnix": c.created_at_unix,
                    "expiresAtUnix": c.expires_at_unix,
                    "ttlRemainingSeconds": c.expires_at_unix.saturating_sub(now_unix()),
                })).collect::<Vec<_>>()
            }))
        }

        "pairing:approve" => {
            if let (Some(cid), Some(code)) = (
                payload.get("challengeId").and_then(Value::as_str),
                payload.get("shortCode").or_else(|| payload.get("code")).and_then(Value::as_str),
            ) {
                let resp = registry.verify_and_approve(cid, code).await?;
                Ok(json!({
                    "success": true,
                    "paired": resp.paired,
                    "authToken": resp.auth_token,
                    "serverPublicKey": resp.server_public_key,
                    "expiresAtUnix": resp.expires_at_unix,
                }))
            } else if let Some(code) = payload.get("shortCode").or_else(|| payload.get("code")).and_then(Value::as_str) {
                let resp = registry.approve_by_short_code(code).await?;
                Ok(json!({
                    "success": true,
                    "paired": resp.paired,
                    "authToken": resp.auth_token,
                    "serverPublicKey": resp.server_public_key,
                    "expiresAtUnix": resp.expires_at_unix,
                }))
            } else if let Some(cid) = payload.get("challengeId").and_then(Value::as_str) {
                let resp = registry.approve_by_challenge_id(cid).await?;
                Ok(json!({
                    "success": true,
                    "paired": resp.paired,
                    "authToken": resp.auth_token,
                    "serverPublicKey": resp.server_public_key,
                    "expiresAtUnix": resp.expires_at_unix,
                }))
            } else {
                Err("Missing 'code', 'shortCode' or 'challengeId' in payload".to_string())
            }
        }

        "pairing:reject" => {
            let cid = payload
                .get("challengeId")
                .and_then(Value::as_str)
                .ok_or("Missing 'challengeId'")?;
            let reason = payload
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("Rejected by administrator");
            registry.reject_challenge(cid, reason).await?;
            Ok(json!({
                "success": true,
                "challengeId": cid,
                "reason": reason
            }))
        }

        "pairing:revoke" => {
            let node_id_str = payload
                .get("nodeId")
                .and_then(Value::as_str)
                .ok_or("Missing 'nodeId'")?;
            let node_id = NodeId::from_str(node_id_str).map_err(|e| format!("Invalid NodeId: {e}"))?;
            let removed = registry.revoke_node(&node_id).await;
            if removed {
                Ok(json!({
                    "success": true,
                    "nodeId": node_id_str,
                    "revoked": true
                }))
            } else {
                Err(format!("Node '{node_id_str}' was not found in approved list"))
            }
        }

        "pairing:create_challenge" => {
            let node_name = payload
                .get("nodeName")
                .and_then(Value::as_str)
                .unwrap_or("Companion Device")
                .to_string();
            let role_str = payload
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("mobile_companion");
            let role = NodeRole::from_str(role_str).map_err(|e| format!("Invalid role: {e}"))?;
            let public_key = payload
                .get("publicKey")
                .and_then(Value::as_str)
                .unwrap_or("ed25519_dummy_companion_key")
                .to_string();

            let req = PairingRequest {
                node_id: NodeId::new(),
                node_name,
                role,
                public_key,
                pairing_nonce: uuid::Uuid::new_v4().to_string(),
                timestamp_unix: now_unix(),
            };

            let challenge = registry.create_challenge(req, 300).await?;
            Ok(json!({
                "challengeId": challenge.challenge_id,
                "shortCode": challenge.short_code,
                "nodeId": challenge.node_info.node_id.to_string(),
                "nodeName": challenge.node_info.node_name,
                "expiresAtUnix": challenge.expires_at_unix,
                "qrPayload": format!("liva-pair:{}:{}", challenge.short_code, challenge.challenge_id)
            }))
        }

        _ => Err(format!("Unknown command: {command}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pairing_owns_and_challenge_approval() {
        assert!(owns("pairing:list"));
        assert!(owns("pairing:approve"));
        assert!(owns("pairing:revoke"));
        assert!(!owns("chat:completion"));

        let registry = global_pairing_registry();
        let req = PairingRequest {
            node_id: NodeId::new(),
            node_name: "Test iPhone 16".to_string(),
            role: NodeRole::MobileCompanion,
            public_key: "ed25519_test_pubkey".to_string(),
            pairing_nonce: "nonce_test".to_string(),
            timestamp_unix: now_unix(),
        };

        let ch = registry.create_challenge(req, 300).await.unwrap();
        assert_eq!(ch.short_code.len(), 6);

        let resp = registry.approve_by_short_code(&ch.short_code).await.unwrap();
        assert!(resp.paired);
        assert!(resp.auth_token.is_some());
    }
}
