//! Attenuation-Only Capabilities & Structural Cure for CVE-2026-32922 (Feature F22).

use crate::authorization::principal::Principal;
use crate::cognitive::policy::RiskTier;
pub use liva_core_types::SecurityError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityGrant {
    pub grant_id: String,
    pub parent_grant_id: Option<String>,
    pub epoch: u64,
    pub principal: Principal,
    pub allowed_tools: Vec<String>,
    pub resource_patterns: Vec<String>,
    pub max_risk_tier: RiskTier,
    pub allowed_recipients: Vec<String>,
    pub max_uses: Option<u32>,
    pub expires_at: Option<u64>,
    pub plan_hash: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DelegationStep {
    pub grant: CapabilityGrant,
    pub mac: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityToken {
    pub grant: CapabilityGrant,
    pub delegation_chain: Vec<DelegationStep>,
    pub mac: [u8; 32],
}

impl CapabilityGrant {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Verifies the 7 Monotonic Attenuation Invariants: self ⊆ parent.
    pub fn verify_attenuation_of(&self, parent: &CapabilityGrant) -> Result<(), SecurityError> {
        // 1. Tool subset check: Child tools ⊆ Parent tools
        for tool in &self.allowed_tools {
            if !parent.allowed_tools.contains(tool) {
                return Err(SecurityError::PrivilegeEscalationDetected(format!(
                    "Tool '{tool}' is not present in parent grant"
                )));
            }
        }

        // 2. Resource pattern containment: Child paths strictly contained within Parent paths
        for child_res in &self.resource_patterns {
            if child_res.contains("..") || child_res.contains("/../") || child_res.contains("\\..\\") {
                return Err(SecurityError::PrivilegeEscalationDetected(
                    "Path traversal attempt in resource pattern detected".into(),
                ));
            }

            let contained = parent.resource_patterns.iter().any(|parent_res| {
                if parent_res == "*" || child_res == parent_res {
                    return true;
                }
                let prefix = parent_res.trim_end_matches('*');
                if !prefix.is_empty() && child_res.starts_with(prefix) {
                    return true;
                }
                false
            });
            if !contained {
                return Err(SecurityError::PrivilegeEscalationDetected(format!(
                    "Resource pattern '{child_res}' exceeds parent scope"
                )));
            }
        }

        // 3. Risk tier monotonicity: Child tier <= Parent tier
        if self.max_risk_tier > parent.max_risk_tier {
            return Err(SecurityError::PrivilegeEscalationDetected(format!(
                "Risk tier '{:?}' exceeds parent risk tier '{:?}'",
                self.max_risk_tier, parent.max_risk_tier
            )));
        }

        // 4. Temporal monotonicity (TTL / Expiry): Child TTL <= Parent TTL
        match (self.expires_at, parent.expires_at) {
            (Some(c_exp), Some(p_exp)) if c_exp > p_exp => {
                return Err(SecurityError::PrivilegeEscalationDetected(format!(
                    "Child expiry ({c_exp}) cannot exceed parent expiry ({p_exp})"
                )));
            }
            (None, Some(p_exp)) => {
                return Err(SecurityError::PrivilegeEscalationDetected(format!(
                    "Child grant cannot be immortal when parent expires at {p_exp}"
                )));
            }
            _ => {}
        }

        // 5. Usage bound monotonicity: Child uses <= Parent uses
        match (self.max_uses, parent.max_uses) {
            (Some(c_uses), Some(p_uses)) if c_uses > p_uses => {
                return Err(SecurityError::PrivilegeEscalationDetected(format!(
                    "Child uses ({c_uses}) cannot exceed parent uses ({p_uses})"
                )));
            }
            (None, Some(p_uses)) => {
                return Err(SecurityError::PrivilegeEscalationDetected(format!(
                    "Child grant cannot have unlimited uses when parent limit is {p_uses}"
                )));
            }
            _ => {}
        }

        // 6. Recipient containment: Child recipients ⊆ Parent recipients
        for recipient in &self.allowed_recipients {
            if !parent.allowed_recipients.contains(recipient) {
                return Err(SecurityError::PrivilegeEscalationDetected(format!(
                    "Recipient '{recipient}' is not present in parent allowed recipients"
                )));
            }
        }

        // 7. Epoch matching: Child epoch == Parent epoch
        if self.epoch != parent.epoch {
            return Err(SecurityError::PrivilegeEscalationDetected(format!(
                "Child epoch ({}) does not match parent epoch ({})",
                self.epoch, parent.epoch
            )));
        }

        Ok(())
    }
}

impl CapabilityToken {
    /// Creates a root token using the master authentication key.
    pub fn new_root(grant: CapabilityGrant, master_key: &[u8; 32]) -> Self {
        let bytes = grant.canonical_bytes();
        let mac = blake3::keyed_hash(master_key, &bytes);
        Self {
            grant,
            delegation_chain: Vec::new(),
            mac: *mac.as_bytes(),
        }
    }

    /// Derives an attenuated child token from this parent token.
    pub fn attenuate(&self, child_grant: CapabilityGrant) -> Result<Self, SecurityError> {
        // Enforce monotonic attenuation
        child_grant.verify_attenuation_of(&self.grant)?;

        // Cryptographic MAC chaining via BLAKE3 keyed hash
        let child_bytes = child_grant.canonical_bytes();
        let child_mac = blake3::keyed_hash(&self.mac, &child_bytes);

        let mut chain = self.delegation_chain.clone();
        chain.push(DelegationStep {
            grant: self.grant.clone(),
            mac: self.mac,
        });

        Ok(Self {
            grant: child_grant,
            delegation_chain: chain,
            mac: *child_mac.as_bytes(),
        })
    }

    /// Verifies the cryptographic MAC chain from master_key down through all delegation steps to the leaf grant.
    pub fn verify_chain(&self, master_key: &[u8; 32]) -> Result<(), SecurityError> {
        if self.delegation_chain.is_empty() {
            let expected_mac = blake3::keyed_hash(master_key, &self.grant.canonical_bytes());
            if expected_mac.as_bytes() != &self.mac {
                return Err(SecurityError::InvalidSignature);
            }
        } else {
            // 1. Root step: verify delegation_chain[0].grant with master_key
            let root_mac = blake3::keyed_hash(master_key, &self.delegation_chain[0].grant.canonical_bytes());
            if root_mac.as_bytes() != &self.delegation_chain[0].mac {
                return Err(SecurityError::InvalidSignature);
            }

            // 2. Intermediate steps: verify monotonic attenuation and sequential MAC chaining
            for i in 1..self.delegation_chain.len() {
                self.delegation_chain[i]
                    .grant
                    .verify_attenuation_of(&self.delegation_chain[i - 1].grant)?;

                let step_mac = blake3::keyed_hash(
                    &self.delegation_chain[i - 1].mac,
                    &self.delegation_chain[i].grant.canonical_bytes(),
                );
                if step_mac.as_bytes() != &self.delegation_chain[i].mac {
                    return Err(SecurityError::InvalidSignature);
                }
            }

            // 3. Leaf step (self.grant): verify monotonic attenuation and leaf MAC chaining
            let last_step = self.delegation_chain.last().unwrap();
            self.grant.verify_attenuation_of(&last_step.grant)?;

            let leaf_mac = blake3::keyed_hash(&last_step.mac, &self.grant.canonical_bytes());
            if leaf_mac.as_bytes() != &self.mac {
                return Err(SecurityError::InvalidSignature);
            }
        }
        Ok(())
    }

    /// Validates validity at the current timestamp and epoch.
    pub fn validate_usable(&self, current_epoch: u64, now_ms: u64) -> Result<(), SecurityError> {
        if self.grant.epoch != current_epoch {
            return Err(SecurityError::TokenEpochRevoked(self.grant.epoch, current_epoch));
        }
        if let Some(exp) = self.grant.expires_at {
            if now_ms > exp {
                return Err(SecurityError::TokenExpired(exp));
            }
        }
        if let Some(uses) = self.grant.max_uses {
            if uses == 0 {
                return Err(SecurityError::TokenExhausted);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_token_attenuation_and_privilege_escalation() {
        let master_key = [7u8; 32];
        let root_grant = CapabilityGrant {
            grant_id: "grant_root".into(),
            parent_grant_id: None,
            epoch: 1,
            principal: Principal::Owner,
            allowed_tools: vec!["fs:read".into(), "fs:write".into()],
            resource_patterns: vec!["/data/workspace/*".into()],
            max_risk_tier: RiskTier::R2,
            allowed_recipients: vec!["alice".into()],
            max_uses: Some(10),
            expires_at: Some(2000),
            plan_hash: None,
        };

        let root_token = CapabilityToken::new_root(root_grant.clone(), &master_key);
        assert!(root_token.verify_chain(&master_key).is_ok());
        assert!(root_token.validate_usable(1, 1500).is_ok());

        // 1. Valid Attenuation: narrow tools, more restrictive path, lower max_uses, earlier expiration
        let valid_child_grant = CapabilityGrant {
            grant_id: "grant_child".into(),
            parent_grant_id: Some("grant_root".into()),
            epoch: 1,
            principal: Principal::Owner,
            allowed_tools: vec!["fs:read".into()],
            resource_patterns: vec!["/data/workspace/subdir/*".into()],
            max_risk_tier: RiskTier::R1,
            allowed_recipients: vec!["alice".into()],
            max_uses: Some(5),
            expires_at: Some(1800),
            plan_hash: None,
        };
        let child_token = root_token
            .attenuate(valid_child_grant)
            .expect("Valid attenuation must succeed");
        assert!(child_token.verify_chain(&master_key).is_ok());

        // 2. Escalation Invariant: expanding operations fails
        let escal_ops_grant = CapabilityGrant {
            grant_id: "grant_escal".into(),
            parent_grant_id: Some("grant_root".into()),
            allowed_tools: vec!["fs:read".into(), "fs:delete_all".into()],
            ..root_grant.clone()
        };
        let err = root_token.attenuate(escal_ops_grant).unwrap_err();
        match err {
            SecurityError::PrivilegeEscalationDetected(_) => {}
            other => panic!("Expected PrivilegeEscalationDetected, got {other:?}"),
        }

        // 3. Escalation Invariant: broadening path prefix fails
        let escal_path_grant = CapabilityGrant {
            grant_id: "grant_escal_path".into(),
            parent_grant_id: Some("grant_root".into()),
            resource_patterns: vec!["/data/*".into()],
            ..root_grant.clone()
        };
        let err = root_token.attenuate(escal_path_grant).unwrap_err();
        assert!(matches!(err, SecurityError::PrivilegeEscalationDetected(_)));

        // 4. Escalation Invariant: extending expiration fails
        let escal_exp_grant = CapabilityGrant {
            grant_id: "grant_escal_exp".into(),
            parent_grant_id: Some("grant_root".into()),
            expires_at: Some(3000),
            ..root_grant.clone()
        };
        let err = root_token.attenuate(escal_exp_grant).unwrap_err();
        assert!(matches!(err, SecurityError::PrivilegeEscalationDetected(_)));

        // 5. Invalid Signature Detection
        let mut tampered = child_token.clone();
        tampered.mac[0] ^= 0xff;
        assert!(matches!(
            tampered.verify_chain(&master_key),
            Err(SecurityError::InvalidSignature)
        ));

        // 6. Usability Invariants: Epoch and Expiration
        assert!(matches!(
            root_token.validate_usable(2, 1500),
            Err(SecurityError::TokenEpochRevoked(1, 2))
        ));
        assert!(matches!(
            root_token.validate_usable(1, 2500),
            Err(SecurityError::TokenExpired(2000))
        ));
    }

    #[test]
    fn test_delegation_chain_verification_and_intermediate_forgery_prevention() {
        let master_key = [42u8; 32];
        let root_grant = CapabilityGrant {
            grant_id: "grant_root_001".into(),
            parent_grant_id: None,
            epoch: 1,
            principal: Principal::Owner,
            allowed_tools: vec!["fs:read".into(), "fs:write".into()],
            resource_patterns: vec!["*".into()],
            max_risk_tier: RiskTier::R2,
            allowed_recipients: vec!["alice".into(), "bob".into()],
            max_uses: Some(100),
            expires_at: Some(10_000),
            plan_hash: None,
        };

        let root_token = CapabilityToken::new_root(root_grant.clone(), &master_key);
        assert!(root_token.verify_chain(&master_key).is_ok());

        // Step 1: Intermediate child
        let child_grant = CapabilityGrant {
            grant_id: "grant_child_002".into(),
            parent_grant_id: Some("grant_root_001".into()),
            epoch: 1,
            principal: Principal::Owner,
            allowed_tools: vec!["fs:read".into(), "fs:write".into()],
            resource_patterns: vec!["/workspace/*".into()],
            max_risk_tier: RiskTier::R2,
            allowed_recipients: vec!["alice".into()],
            max_uses: Some(50),
            expires_at: Some(8000),
            plan_hash: None,
        };
        let child_token = root_token.attenuate(child_grant.clone()).expect("attenuate child");
        assert!(child_token.verify_chain(&master_key).is_ok());

        // Step 2: Leaf grandchild
        let grandchild_grant = CapabilityGrant {
            grant_id: "grant_grandchild_003".into(),
            parent_grant_id: Some("grant_child_002".into()),
            epoch: 1,
            principal: Principal::Owner,
            allowed_tools: vec!["fs:read".into()],
            resource_patterns: vec!["/workspace/sub/*".into()],
            max_risk_tier: RiskTier::R1,
            allowed_recipients: vec!["alice".into()],
            max_uses: Some(10),
            expires_at: Some(5000),
            plan_hash: None,
        };
        let grandchild_token = child_token
            .attenuate(grandchild_grant.clone())
            .expect("attenuate grandchild");
        assert_eq!(grandchild_token.delegation_chain.len(), 2);
        assert!(grandchild_token.verify_chain(&master_key).is_ok());

        // Forgery 1: Wrong master key fails verification
        let wrong_key = [99u8; 32];
        assert!(matches!(
            grandchild_token.verify_chain(&wrong_key),
            Err(SecurityError::InvalidSignature)
        ));

        // Forgery 2: Tampering intermediate step MAC fails verification
        let mut tampered_intermediate_mac = grandchild_token.clone();
        tampered_intermediate_mac.delegation_chain[0].mac[0] ^= 0xaa;
        assert!(matches!(
            tampered_intermediate_mac.verify_chain(&master_key),
            Err(SecurityError::InvalidSignature)
        ));

        // Forgery 3: Tampering intermediate step grant fails verification
        let mut tampered_intermediate_grant = grandchild_token.clone();
        tampered_intermediate_grant.delegation_chain[0].grant.max_risk_tier = RiskTier::R4;
        assert!(matches!(
            tampered_intermediate_grant.verify_chain(&master_key),
            Err(SecurityError::InvalidSignature)
        ));

        // Forgery 4: Fabricated delegation chain from an arbitrary invented secret
        let fake_secret = [0x55u8; 32];
        let fake_root = CapabilityToken::new_root(root_grant, &fake_secret);
        let fake_child = fake_root.attenuate(child_grant).expect("attenuate fake");
        let fake_grandchild = fake_child.attenuate(grandchild_grant).expect("attenuate fake leaf");
        assert!(matches!(
            fake_grandchild.verify_chain(&master_key),
            Err(SecurityError::InvalidSignature)
        ));
    }

    #[test]
    fn test_path_traversal_and_wildcard_containment() {
        let master_key = [1u8; 32];
        let root_grant = CapabilityGrant {
            grant_id: "grant_root".into(),
            parent_grant_id: None,
            epoch: 1,
            principal: Principal::Owner,
            allowed_tools: vec!["fs:read".into()],
            resource_patterns: vec!["/workspace/*".into()],
            max_risk_tier: RiskTier::R1,
            allowed_recipients: vec![],
            max_uses: None,
            expires_at: None,
            plan_hash: None,
        };
        let root_token = CapabilityToken::new_root(root_grant.clone(), &master_key);

        // Path traversal attack: /workspace/../../etc/shadow
        let traversal_grant = CapabilityGrant {
            grant_id: "grant_traversal".into(),
            parent_grant_id: Some("grant_root".into()),
            resource_patterns: vec!["/workspace/../../etc/shadow".into()],
            ..root_grant.clone()
        };
        let err = root_token.attenuate(traversal_grant).unwrap_err();
        assert!(matches!(err, SecurityError::PrivilegeEscalationDetected(_)));

        // Path traversal attack with backslashes
        let win_traversal_grant = CapabilityGrant {
            grant_id: "grant_win_traversal".into(),
            parent_grant_id: Some("grant_root".into()),
            resource_patterns: vec!["\\workspace\\..\\secret".into()],
            ..root_grant.clone()
        };
        let err = root_token.attenuate(win_traversal_grant).unwrap_err();
        assert!(matches!(err, SecurityError::PrivilegeEscalationDetected(_)));

        // Wildcard '*' in parent allows any valid child pattern without traversal
        let wildcard_root_grant = CapabilityGrant {
            grant_id: "grant_wc_root".into(),
            parent_grant_id: None,
            epoch: 1,
            principal: Principal::Owner,
            allowed_tools: vec!["fs:read".into()],
            resource_patterns: vec!["*".into()],
            max_risk_tier: RiskTier::R1,
            allowed_recipients: vec![],
            max_uses: None,
            expires_at: None,
            plan_hash: None,
        };
        let wc_root_token = CapabilityToken::new_root(wildcard_root_grant.clone(), &master_key);
        let child_scoped_grant = CapabilityGrant {
            grant_id: "grant_wc_child".into(),
            parent_grant_id: Some("grant_wc_root".into()),
            resource_patterns: vec!["/app/data/*".into()],
            ..wildcard_root_grant.clone()
        };
        assert!(wc_root_token.attenuate(child_scoped_grant).is_ok());

        // Expanding from specific pattern to '*' is rejected
        let expand_to_wildcard = CapabilityGrant {
            grant_id: "grant_expand_wc".into(),
            parent_grant_id: Some("grant_root".into()),
            resource_patterns: vec!["*".into()],
            ..root_grant
        };
        let err = root_token.attenuate(expand_to_wildcard).unwrap_err();
        assert!(matches!(err, SecurityError::PrivilegeEscalationDetected(_)));
    }
}
