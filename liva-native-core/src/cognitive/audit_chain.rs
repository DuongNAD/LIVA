//! Cryptographic Tamper-Evident Append-Only Audit Ledger (Feature F24).

use crate::cognitive::policy::RiskTier;
use crate::cognitive::redaction::SecretScrubber;
use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};

pub const GENESIS_PREV_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Single cryptographically-chained block in the audit ledger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditBlock {
    pub sequence_num: u64,
    pub block_hash: String,
    pub prev_block_hash: String,
    pub timestamp_ms: i64,
    pub principal: String,
    pub action: String,
    pub target: String,
    pub payload_hash: String,
    pub redacted_payload: String,
    pub risk_tier: RiskTier,
    pub policy_decision: String,
    pub status: String,
    pub duration_ms: Option<u64>,
}

impl AuditBlock {
    /// Computes deterministic BLAKE3 block hash:
    /// Block_n = BLAKE3(prev_block_hash || timestamp_ms || principal || action || target || payload_hash)
    pub fn compute_hash(
        prev_hash: &str,
        timestamp_ms: i64,
        principal: &str,
        action: &str,
        target: &str,
        payload_hash: &str,
    ) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(prev_hash.as_bytes());
        hasher.update(&timestamp_ms.to_le_bytes());
        hasher.update(principal.as_bytes());
        hasher.update(action.as_bytes());
        hasher.update(target.as_bytes());
        hasher.update(payload_hash.as_bytes());
        hasher.finalize().to_hex().to_string()
    }
}

pub struct Blake3AuditChain;

impl Blake3AuditChain {
    /// Initializes schema and immutability triggers.
    pub fn init_schema(conn: &Connection) -> SqlResult<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_hash_chain (
                sequence_num INTEGER PRIMARY KEY AUTOINCREMENT,
                block_hash TEXT NOT NULL UNIQUE,
                prev_block_hash TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                principal TEXT NOT NULL,
                action TEXT NOT NULL,
                target TEXT NOT NULL,
                payload_hash TEXT NOT NULL,
                redacted_payload TEXT NOT NULL,
                risk_tier TEXT NOT NULL,
                policy_decision TEXT NOT NULL,
                status TEXT NOT NULL,
                duration_ms INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_audit_chain_block_hash ON audit_hash_chain(block_hash);
            CREATE INDEX IF NOT EXISTS idx_audit_chain_prev_hash ON audit_hash_chain(prev_block_hash);
            CREATE INDEX IF NOT EXISTS idx_audit_chain_timestamp ON audit_hash_chain(timestamp_ms);

            -- Immutability Trigger: Prohibit UPDATE
            CREATE TRIGGER IF NOT EXISTS trg_audit_chain_no_update
            BEFORE UPDATE ON audit_hash_chain
            BEGIN
                SELECT RAISE(ABORT, 'Audit log is strictly append-only: UPDATE operations are prohibited');
            END;

            -- Immutability Trigger: Prohibit DELETE
            CREATE TRIGGER IF NOT EXISTS trg_audit_chain_no_delete
            BEFORE DELETE ON audit_hash_chain
            BEGIN
                SELECT RAISE(ABORT, 'Audit log is strictly append-only: DELETE operations are prohibited');
            END;"
        )?;
        Ok(())
    }

    /// Appends a new verified block to the chain.
    pub fn append_entry(
        conn: &Connection,
        principal: &str,
        action: &str,
        target: &str,
        raw_payload: &str,
        risk_tier: RiskTier,
        policy_decision: &str,
        status: &str,
        duration_ms: Option<u64>,
    ) -> SqlResult<AuditBlock> {
        let (latest_seq, latest_hash) = Self::get_latest_tip(conn)?
            .unwrap_or_else(|| (0, GENESIS_PREV_HASH.to_string()));

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let redacted_payload = SecretScrubber::scrub(raw_payload);
        let payload_hash = blake3::hash(redacted_payload.as_bytes()).to_hex().to_string();

        let block_hash = AuditBlock::compute_hash(
            &latest_hash,
            now_ms,
            principal,
            action,
            target,
            &payload_hash,
        );

        let new_seq = latest_seq + 1;

        conn.execute(
            "INSERT INTO audit_hash_chain (
                sequence_num, block_hash, prev_block_hash, timestamp_ms, principal,
                action, target, payload_hash, redacted_payload, risk_tier, policy_decision,
                status, duration_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                new_seq,
                block_hash,
                latest_hash,
                now_ms,
                principal,
                action,
                target,
                payload_hash,
                redacted_payload,
                risk_tier.as_str(),
                policy_decision,
                status,
                duration_ms.map(|d| d as i64),
            ],
        )?;

        Ok(AuditBlock {
            sequence_num: new_seq,
            block_hash,
            prev_block_hash: latest_hash,
            timestamp_ms: now_ms,
            principal: principal.to_string(),
            action: action.to_string(),
            target: target.to_string(),
            payload_hash,
            redacted_payload,
            risk_tier,
            policy_decision: policy_decision.to_string(),
            status: status.to_string(),
            duration_ms,
        })
    }

    /// Linear forward verification of the entire hash chain from Genesis to tip.
    pub fn verify_chain_integrity(conn: &Connection) -> Result<usize, String> {
        let mut stmt = conn
            .prepare(
                "SELECT sequence_num, block_hash, prev_block_hash, timestamp_ms, principal,
                        action, target, payload_hash, redacted_payload, risk_tier, policy_decision,
                        status, duration_ms
                 FROM audit_hash_chain
                 ORDER BY sequence_num ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                let risk_str: String = row.get(9)?;
                Ok(AuditBlock {
                    sequence_num: row.get(0)?,
                    block_hash: row.get(1)?,
                    prev_block_hash: row.get(2)?,
                    timestamp_ms: row.get(3)?,
                    principal: row.get(4)?,
                    action: row.get(5)?,
                    target: row.get(6)?,
                    payload_hash: row.get(7)?,
                    redacted_payload: row.get(8)?,
                    risk_tier: RiskTier::from_str_loose(&risk_str).unwrap_or(RiskTier::R3),
                    policy_decision: row.get(10)?,
                    status: row.get(11)?,
                    duration_ms: row.get::<_, Option<i64>>(12)?.map(|d| d as u64),
                })
            })
            .map_err(|e| e.to_string())?;

        let mut expected_prev = GENESIS_PREV_HASH.to_string();
        let mut count = 0;

        for r in rows {
            let block = r.map_err(|e| e.to_string())?;
            count += 1;

            // Invariant 1: Pointer continuity
            if block.prev_block_hash != expected_prev {
                return Err(format!(
                    "Audit chain broken at block {}: prev_hash mismatch (expected {}, found {})",
                    block.sequence_num, expected_prev, block.prev_block_hash
                ));
            }

            // Invariant 2: Payload integrity
            let computed_payload_hash =
                blake3::hash(block.redacted_payload.as_bytes()).to_hex().to_string();
            if computed_payload_hash != block.payload_hash {
                return Err(format!(
                    "Audit payload tampered at block {}: payload hash mismatch",
                    block.sequence_num
                ));
            }

            // Invariant 3: Block hash integrity
            let computed_block_hash = AuditBlock::compute_hash(
                &block.prev_block_hash,
                block.timestamp_ms,
                &block.principal,
                &block.action,
                &block.target,
                &block.payload_hash,
            );
            if computed_block_hash != block.block_hash {
                return Err(format!(
                    "Audit block corrupted at block {}: block hash mismatch",
                    block.sequence_num
                ));
            }

            expected_prev = block.block_hash;
        }

        Ok(count)
    }

    pub fn get_latest_tip(conn: &Connection) -> SqlResult<Option<(u64, String)>> {
        let mut stmt = conn.prepare(
            "SELECT sequence_num, block_hash FROM audit_hash_chain ORDER BY sequence_num DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(r) = rows.next()? {
            Ok(Some((r.get(0)?, r.get(1)?)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_chain_append_and_verify_clean() {
        let conn = Connection::open_in_memory().unwrap();
        Blake3AuditChain::init_schema(&conn).unwrap();

        // Empty chain verifies cleanly with count 0
        assert_eq!(Blake3AuditChain::verify_chain_integrity(&conn).unwrap(), 0);

        // Append 3 blocks
        let b1 = Blake3AuditChain::append_entry(
            &conn,
            "owner",
            "read_file",
            "notes.md",
            "content: secret",
            RiskTier::R0,
            "allow",
            "success",
            Some(12),
        )
        .unwrap();
        assert_eq!(b1.sequence_num, 1);
        assert_eq!(b1.prev_block_hash, GENESIS_PREV_HASH);

        let b2 = Blake3AuditChain::append_entry(
            &conn,
            "owner",
            "write_file",
            "notes.md",
            "content: updated",
            RiskTier::R1,
            "allow",
            "success",
            Some(20),
        )
        .unwrap();
        assert_eq!(b2.sequence_num, 2);
        assert_eq!(b2.prev_block_hash, b1.block_hash);

        let b3 = Blake3AuditChain::append_entry(
            &conn,
            "owner",
            "send_message",
            "tg:123",
            "hello world",
            RiskTier::R3,
            "confirm",
            "success",
            Some(35),
        )
        .unwrap();
        assert_eq!(b3.sequence_num, 3);
        assert_eq!(b3.prev_block_hash, b2.block_hash);

        // Verification passes with count 3
        assert_eq!(Blake3AuditChain::verify_chain_integrity(&conn).unwrap(), 3);
    }

    #[test]
    fn test_audit_chain_immutability_triggers() {
        let conn = Connection::open_in_memory().unwrap();
        Blake3AuditChain::init_schema(&conn).unwrap();

        Blake3AuditChain::append_entry(
            &conn,
            "owner",
            "exec",
            "cmd",
            "payload",
            RiskTier::R3,
            "allow",
            "ok",
            None,
        )
        .unwrap();

        // UPDATE must be aborted by trigger
        let update_res = conn.execute(
            "UPDATE audit_hash_chain SET action = 'tampered' WHERE sequence_num = 1",
            [],
        );
        assert!(update_res.is_err(), "Trigger must abort UPDATE");

        // DELETE must be aborted by trigger
        let delete_res = conn.execute("DELETE FROM audit_hash_chain WHERE sequence_num = 1", []);
        assert!(delete_res.is_err(), "Trigger must abort DELETE");
    }
}
