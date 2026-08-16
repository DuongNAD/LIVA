use regex::Regex;
use rusqlite::{Connection, Result as SqlResult, params};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

static RE_OPENAI_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"sk-[a-zA-Z0-9_-]{20,}").expect("valid regex"));

static RE_ANTHROPIC_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"sk-ant-[a-zA-Z0-9_-]{20,}").expect("valid regex"));

static RE_BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)Bearer\s+[a-zA-Z0-9_\.\-\+/=]{20,}").expect("valid regex"));

static RE_URI_CREDENTIALS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"://([^:\s@]+):([^@\s]+)@"#).expect("valid regex"));

static RE_PEM_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z0-9 ]*PRIVATE KEY-----")
        .expect("valid regex")
});

static RE_JSON_SECRETS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)("?(?:password|passwd|secret|api_key|apikey|token|private_key)"?\s*:\s*)"(?:[^"\\]|\\.)*""#)
        .expect("valid regex")
});

static RE_KV_SECRETS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(password|passwd|secret|api_key|apikey|token|private_key)\s*=\s*[^\s,;&]+")
        .expect("valid regex")
});

static RE_JWT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\beyJ[a-zA-Z0-9_-]{10,}\.eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\b")
        .expect("valid regex")
});

static RE_CREDIT_CARD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\b|_)(?:\d{4}[ -]?){3}\d{4}(\b|_)").expect("valid regex"));

/// High-performance Secret and PII Scrubber using compiled regexes.
pub struct SecretScrubber;

impl SecretScrubber {
    /// Zeroizes and sanitizes all detected API keys, passwords, bearer tokens, PEM keys, URI credentials, and credit cards.
    pub fn scrub(input: &str) -> String {
        let step1 = RE_PEM_KEY.replace_all(input, "[REDACTED_PRIVATE_KEY]");
        let step2 = RE_ANTHROPIC_KEY.replace_all(&step1, "[REDACTED_ANTHROPIC_KEY]");
        let step3 = RE_OPENAI_KEY.replace_all(&step2, "[REDACTED_API_KEY]");
        let step4 = RE_BEARER.replace_all(&step3, "Bearer [REDACTED_BEARER_TOKEN]");
        let step5 = RE_URI_CREDENTIALS.replace_all(&step4, "://$1:[REDACTED_PASSWORD]@");
        let step6 = RE_JWT.replace_all(&step5, "[REDACTED_JWT]");
        let step7 = RE_JSON_SECRETS.replace_all(&step6, r#"$1"[REDACTED_SECRET]""#);
        let step8 = RE_KV_SECRETS.replace_all(&step7, "$1=[REDACTED_SECRET]");
        let step9 = RE_CREDIT_CARD.replace_all(&step8, "$1[REDACTED_CREDIT_CARD]$2");
        step9.into_owned()
    }

    /// Recursively scrubs all string fields in a JSON Value tree.
    pub fn scrub_json(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => serde_json::Value::String(Self::scrub(s)),
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(Self::scrub_json).collect())
            }
            serde_json::Value::Object(map) => {
                let mut new_map = serde_json::Map::with_capacity(map.len());
                for (k, v) in map {
                    let key_lower = k.to_lowercase();
                    if key_lower.contains("password")
                        || key_lower.contains("secret")
                        || key_lower.contains("api_key")
                        || key_lower.contains("apikey")
                        || key_lower.contains("token")
                        || key_lower.contains("private_key")
                    {
                        new_map.insert(
                            k.clone(),
                            serde_json::Value::String("[REDACTED_SECRET]".to_string()),
                        );
                    } else {
                        new_map.insert(k.clone(), Self::scrub_json(v));
                    }
                }
                serde_json::Value::Object(new_map)
            }
            other => other.clone(),
        }
    }
}

/// A structured row representing an action execution in the SQLite action_audit_ledger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionAuditRecord {
    pub id: Option<i64>,
    pub action_id: String,
    pub idempotency_key: String,
    pub source_event_id: Option<String>,
    pub tool_id: String,
    pub risk_tier: String,
    pub policy_decision: String,
    pub principal: String,
    pub redacted_params: String,
    pub redacted_observation: Option<String>,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub created_at_ms: i64,
}

/// Database layer for storing and querying redacted action audit records in SQLite.
pub struct RedactedAuditLedger;

impl RedactedAuditLedger {
    /// Inserts a new audit record into SQLite, automatically applying secret scrubbing.
    pub fn record_action(conn: &Connection, record: &ActionAuditRecord) -> SqlResult<i64> {
        let scrubbed_params = SecretScrubber::scrub(&record.redacted_params);
        let scrubbed_obs = record
            .redacted_observation
            .as_deref()
            .map(SecretScrubber::scrub);

        conn.execute(
            "INSERT INTO action_audit_ledger (
                action_id, idempotency_key, source_event_id, tool_id,
                risk_tier, policy_decision, principal, redacted_params,
                redacted_observation, status, duration_ms, created_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                record.action_id,
                record.idempotency_key,
                record.source_event_id,
                record.tool_id,
                record.risk_tier,
                record.policy_decision,
                record.principal,
                scrubbed_params,
                scrubbed_obs,
                record.status,
                record.duration_ms,
                record.created_at_ms,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Queries an audit record by its unique action_id.
    pub fn query_by_action_id(
        conn: &Connection,
        action_id: &str,
    ) -> SqlResult<Option<ActionAuditRecord>> {
        let mut stmt = conn.prepare(
            "SELECT id, action_id, idempotency_key, source_event_id, tool_id,
                    risk_tier, policy_decision, principal, redacted_params,
                    redacted_observation, status, duration_ms, created_at_ms
             FROM action_audit_ledger
             WHERE action_id = ?1",
        )?;

        let mut rows = stmt.query(params![action_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ActionAuditRecord {
                id: Some(row.get(0)?),
                action_id: row.get(1)?,
                idempotency_key: row.get(2)?,
                source_event_id: row.get(3)?,
                tool_id: row.get(4)?,
                risk_tier: row.get(5)?,
                policy_decision: row.get(6)?,
                principal: row.get(7)?,
                redacted_params: row.get(8)?,
                redacted_observation: row.get(9)?,
                status: row.get(10)?,
                duration_ms: row.get(11)?,
                created_at_ms: row.get(12)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Queries recent audit records ordered by creation timestamp descending.
    pub fn query_recent(conn: &Connection, limit: usize) -> SqlResult<Vec<ActionAuditRecord>> {
        let mut stmt = conn.prepare(
            "SELECT id, action_id, idempotency_key, source_event_id, tool_id,
                    risk_tier, policy_decision, principal, redacted_params,
                    redacted_observation, status, duration_ms, created_at_ms
             FROM action_audit_ledger
             ORDER BY created_at_ms DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ActionAuditRecord {
                id: Some(row.get(0)?),
                action_id: row.get(1)?,
                idempotency_key: row.get(2)?,
                source_event_id: row.get(3)?,
                tool_id: row.get(4)?,
                risk_tier: row.get(5)?,
                policy_decision: row.get(6)?,
                principal: row.get(7)?,
                redacted_params: row.get(8)?,
                redacted_observation: row.get(9)?,
                status: row.get(10)?,
                duration_ms: row.get(11)?,
                created_at_ms: row.get(12)?,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }
}
