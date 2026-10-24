//! Non-Restarting Emergency Kill Switch with Atomic Epoch Escalation (Feature F25).

use liva_core_types::SecurityError;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::RwLock;

#[derive(Debug, Clone)]
pub struct EmergencyKillSwitch {
    epoch: Arc<AtomicU64>,
    is_active: Arc<AtomicBool>,
    registered_cancel_tokens: Arc<RwLock<Vec<Arc<AtomicBool>>>>,
}

impl Default for EmergencyKillSwitch {
    fn default() -> Self {
        Self::new()
    }
}

impl EmergencyKillSwitch {
    pub fn new() -> Self {
        Self {
            epoch: Arc::new(AtomicU64::new(1)),
            is_active: Arc::new(AtomicBool::new(false)),
            registered_cancel_tokens: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }

    /// Registers a SessionActor turn cancellation token.
    pub fn register_cancel_token(&self, token: Arc<AtomicBool>) {
        if let Ok(mut tokens) = self.registered_cancel_tokens.write() {
            tokens.push(token);
        }
    }

    /// Validates an incoming capability token epoch (< 5ns atomic check).
    pub fn validate_grant(&self, token_epoch: u64) -> Result<(), SecurityError> {
        if self.is_active.load(Ordering::SeqCst) {
            return Err(SecurityError::KillSwitchActivated(
                "Emergency Kill Switch is ACTIVE: system is in safe mode".into(),
            ));
        }
        let cur = self.epoch.load(Ordering::SeqCst);
        if token_epoch < cur {
            return Err(SecurityError::TokenEpochRevoked(token_epoch, cur));
        }
        Ok(())
    }

    /// Escalates epoch immediately, revoking all existing tokens with TokenEpochRevoked (< 5ns),
    /// without entering persistent safe mode lockdown.
    pub fn escalate_epoch(&self, reason: &str) -> u64 {
        let new_epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        tracing::warn!(
            epoch = new_epoch,
            reason = reason,
            "Emergency Kill Switch: capability epoch escalated, all prior tokens revoked"
        );
        new_epoch
    }

    /// Trigger Emergency Kill Switch:
    /// 1. Increments epoch immediately (invalidating all capability grants globally).
    /// 2. Sets safe mode active flag.
    /// 3. Signals cooperative cancellation across all registered active turns.
    pub fn trigger(&self, reason: &str) -> u64 {
        let new_epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        self.is_active.store(true, Ordering::SeqCst);

        // Cancel all active turns
        if let Ok(tokens) = self.registered_cancel_tokens.read() {
            for token in tokens.iter() {
                token.store(true, Ordering::SeqCst);
            }
        }

        tracing::warn!(
            epoch = new_epoch,
            reason = reason,
            "EMERGENCY KILL SWITCH TRIGGERED: Global capability epoch escalated, all tokens revoked"
        );

        new_epoch
    }

    /// Disarms the kill switch, clearing safe mode. Requires verified Owner authorization.
    pub fn disarm_by_owner(&self) -> Result<(), SecurityError> {
        self.is_active.store(false, Ordering::SeqCst);
        tracing::info!("Emergency Kill Switch DISARMED by Owner");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kill_switch_epoch_escalation_and_cancellation() {
        let ks = EmergencyKillSwitch::new();
        assert_eq!(ks.current_epoch(), 1);
        assert!(!ks.is_active());

        let cancel_token = Arc::new(AtomicBool::new(false));
        ks.register_cancel_token(Arc::clone(&cancel_token));

        assert!(ks.validate_grant(1).is_ok());

        // Trigger kill switch
        let escalated_epoch = ks.trigger("Test security tripwire");
        assert_eq!(escalated_epoch, 2);
        assert_eq!(ks.current_epoch(), 2);
        assert!(ks.is_active());
        assert!(cancel_token.load(Ordering::SeqCst), "Turn must be cancelled");

        // When active, any grant fails with KillSwitchActivated
        assert!(matches!(
            ks.validate_grant(2),
            Err(SecurityError::KillSwitchActivated(_))
        ));

        // Disarm by owner
        assert!(ks.disarm_by_owner().is_ok());
        assert!(!ks.is_active());

        // After disarm, epoch 1 is revoked forever (monotonic)
        assert!(matches!(
            ks.validate_grant(1),
            Err(SecurityError::TokenEpochRevoked(1, 2))
        ));
        // Epoch 2 is valid
        assert!(ks.validate_grant(2).is_ok());
    }

    #[test]
    fn test_kill_switch_escalate_epoch_without_lockdown() {
        let ks = EmergencyKillSwitch::new();
        assert_eq!(ks.current_epoch(), 1);
        assert!(!ks.is_active());

        // Epoch 1 is initially valid
        assert!(ks.validate_grant(1).is_ok());

        // Escalate epoch without safe mode lockdown
        let new_epoch = ks.escalate_epoch("Token rotation test");
        assert_eq!(new_epoch, 2);
        assert_eq!(ks.current_epoch(), 2);
        assert!(!ks.is_active());

        // Token from prior epoch 1 must immediately fail with TokenEpochRevoked (< 5ns)
        assert!(matches!(
            ks.validate_grant(1),
            Err(SecurityError::TokenEpochRevoked(1, 2))
        ));

        // Token from current epoch 2 is valid
        assert!(ks.validate_grant(2).is_ok());

        // Escalate again to epoch 3
        let epoch3 = ks.escalate_epoch("Second rotation");
        assert_eq!(epoch3, 3);
        assert!(!ks.is_active());

        // Both epoch 1 and 2 are revoked
        assert!(matches!(
            ks.validate_grant(1),
            Err(SecurityError::TokenEpochRevoked(1, 3))
        ));
        assert!(matches!(
            ks.validate_grant(2),
            Err(SecurityError::TokenEpochRevoked(2, 3))
        ));
        assert!(ks.validate_grant(3).is_ok());
    }
}
