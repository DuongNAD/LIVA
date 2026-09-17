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
    Regex::new(r#"(?i)("?[a-zA-Z0-9_-]*(?:password|passwd|secret|api_key|apikey|token|private_key)[a-zA-Z0-9_-]*"?\s*:\s*)"(?:[^"\\]|\\.)*""#)
        .expect("valid regex")
});

static RE_KV_SECRETS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(["']?[a-zA-Z0-9_-]*(?:password|passwd|secret|api_key|apikey|token|private_key)[a-zA-Z0-9_-]*["']?)\s*=\s*[^\s,;&]+"#)
        .expect("valid regex")
});

static RE_JWT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\beyJ[a-zA-Z0-9_-]{10,}\.eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\b")
        .expect("valid regex")
});

static RE_CREDIT_CARD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\b|_)(?:\d{4}[ -]?){3}\d{4}(\b|_)").expect("valid regex"));

static RE_BANK_ACCOUNT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)((?:\b|_)(?:stk|số[_\s]+tài[_\s]+khoản|so[_\s]+tai[_\s]+khoan|tài[_\s]+khoản(?:[_\s]+số)?|tai[_\s]+khoan(?:[_\s]+so)?|chuyển[_\s]+khoản|chuyen[_\s]+khoan|tk(?:\s+nh|\s+ngân\s+hàng)?|account(?:[_\s]+(?:number|no|num))?|acct(?:[_\s]+(?:no|num))?|bank[_\s]+account|iban)(?:["']?\s*[:=-]\s*["']?|\s+))(\d{9,16})\b"#,
    )
    .expect("valid regex")
});

static RE_CCCD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b0\d{11}\b").expect("valid regex"));

static RE_VIETNAMESE_PHONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\+84|\b0)(?:3|5|7|8|9)\d{8}\b").expect("valid regex"));

/// High-performance Secret and PII Scrubber using compiled regexes.
pub struct SecretScrubber;

impl SecretScrubber {
    /// Alias for `scrub` to satisfy PROJECT.md interface contract specification.
    pub fn mask_secrets(input: &str) -> String {
        Self::scrub(input)
    }

    /// Zeroizes and sanitizes all detected API keys, passwords, bearer tokens,
    /// PEM keys, URI credentials, credit cards, and Vietnamese PII
    /// (CCCD, mobile phones, bank accounts) per Decree 13/2023/NĐ-CP.
    pub fn scrub(input: &str) -> String {
        let step1 = RE_PEM_KEY.replace_all(input, "[REDACTED_PRIVATE_KEY]");
        let step2 = RE_ANTHROPIC_KEY.replace_all(&step1, "[REDACTED_ANTHROPIC_KEY]");
        let step3 = RE_OPENAI_KEY.replace_all(&step2, "[REDACTED_API_KEY]");
        let step4 = RE_BEARER.replace_all(&step3, "Bearer [REDACTED_BEARER_TOKEN]");
        let step5 = RE_URI_CREDENTIALS.replace_all(&step4, "://$1:[REDACTED_PASSWORD]@");
        let step6 = RE_JWT.replace_all(&step5, "[REDACTED_JWT]");
        let step7 = RE_JSON_SECRETS.replace_all(&step6, r#"$1"[REDACTED_SECRET]""#);
        let step8 = RE_KV_SECRETS.replace_all(&step7, "$1=[REDACTED_SECRET]");
        let step9 = RE_BANK_ACCOUNT.replace_all(&step8, "$1[REDACTED_BANK_ACCOUNT]");
        let step10 = RE_CREDIT_CARD.replace_all(&step9, "$1[REDACTED_CREDIT_CARD]$2");
        let step11 = RE_CCCD.replace_all(&step10, "[REDACTED_CCCD]");
        let step12 = RE_VIETNAMESE_PHONE.replace_all(&step11, "[REDACTED_PHONE]");
        step12.into_owned()
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
                    } else if key_lower == "cccd"
                        || key_lower == "cmnd"
                        || key_lower.ends_with("_cccd")
                        || key_lower.ends_with("_cmnd")
                    {
                        new_map.insert(
                            k.clone(),
                            serde_json::Value::String("[REDACTED_CCCD]".to_string()),
                        );
                    } else if key_lower == "stk"
                        || key_lower == "bank_account"
                        || key_lower == "account_number"
                        || key_lower.ends_with("_stk")
                        || key_lower.ends_with("_bank_account")
                        || key_lower.ends_with("_account_number")
                    {
                        new_map.insert(
                            k.clone(),
                            serde_json::Value::String("[REDACTED_BANK_ACCOUNT]".to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_scrubber_standard_secrets() {
        let input =
            "My OpenAI key is sk-12345678901234567890 and Bearer token12345678901234567890.";
        let scrubbed = SecretScrubber::scrub(input);
        assert!(!scrubbed.contains("sk-12345678901234567890"));
        assert!(scrubbed.contains("[REDACTED_API_KEY]"));
        assert!(scrubbed.contains("Bearer [REDACTED_BEARER_TOKEN]"));
    }

    #[test]
    fn test_secret_scrubber_vietnamese_cccd() {
        let raw = "Khách hàng có số CCCD là 001098012345 đăng ký tại Hà Nội";
        let scrubbed = SecretScrubber::scrub(raw);
        assert!(!scrubbed.contains("001098012345"));
        assert!(scrubbed.contains("[REDACTED_CCCD]"));

        // Boundary conditions
        let cccd_11_digits = "Mã 00109801234 có 11 chữ số";
        assert_eq!(SecretScrubber::scrub(cccd_11_digits), cccd_11_digits);
        let cccd_13_digits = "Mã 0010980123456 có 13 chữ số";
        assert_eq!(SecretScrubber::scrub(cccd_13_digits), cccd_13_digits);
        let cccd_nonzero = "Mã 101098012345 không bắt đầu bằng 0";
        assert_eq!(SecretScrubber::scrub(cccd_nonzero), cccd_nonzero);
    }

    #[test]
    fn test_secret_scrubber_vietnamese_phone() {
        let raw = "Liên hệ hotline +84912345678 hoặc di động 0381234567 ngay";
        let scrubbed = SecretScrubber::scrub(raw);
        assert!(!scrubbed.contains("+84912345678"));
        assert!(!scrubbed.contains("0381234567"));
        assert_eq!(
            scrubbed,
            "Liên hệ hotline [REDACTED_PHONE] hoặc di động [REDACTED_PHONE] ngay"
        );

        let phone_start = "+84987654321 là số chính";
        assert_eq!(
            SecretScrubber::scrub(phone_start),
            "[REDACTED_PHONE] là số chính"
        );

        // Non-mobile numbers preserved
        let phone_landline = "Số máy bàn 02438255555 Hà Nội";
        assert_eq!(SecretScrubber::scrub(phone_landline), phone_landline);
        let phone_short = "Số ngắn 091234567 không đủ độ dài";
        assert_eq!(SecretScrubber::scrub(phone_short), phone_short);
    }

    #[test]
    fn test_secret_scrubber_vietnamese_bank_account() {
        let raw = "Vui lòng chuyển tiền vào STK: 123456789012 tại Vietcombank";
        let scrubbed = SecretScrubber::scrub(raw);
        assert!(!scrubbed.contains("123456789012"));
        assert!(scrubbed.contains("STK: [REDACTED_BANK_ACCOUNT]"));

        let bank_9 = "STK: 123456789";
        assert_eq!(
            SecretScrubber::scrub(bank_9),
            "STK: [REDACTED_BANK_ACCOUNT]"
        );
        let bank_16 = "Số tài khoản: 1234567890123456";
        assert_eq!(
            SecretScrubber::scrub(bank_16),
            "Số tài khoản: [REDACTED_BANK_ACCOUNT]"
        );

        let bank_8 = "STK: 12345678";
        assert_eq!(SecretScrubber::scrub(bank_8), bank_8);
        let bank_17 = "STK: 12345678901234567";
        assert_eq!(SecretScrubber::scrub(bank_17), bank_17);

        // Currencies and timestamps preserved
        let currency = "Số tiền 500000000 VND";
        assert_eq!(SecretScrubber::scrub(currency), currency);
        let timestamp = "Timestamp 1726176000000 ms";
        assert_eq!(SecretScrubber::scrub(timestamp), timestamp);

        // Precedence: STK starts with 0 or 09
        let bank_starts_with_0 = "STK: 001098012345";
        assert_eq!(
            SecretScrubber::scrub(bank_starts_with_0),
            "STK: [REDACTED_BANK_ACCOUNT]"
        );
        let bank_starts_with_09 = "Số tài khoản: 0912345678";
        assert_eq!(
            SecretScrubber::scrub(bank_starts_with_09),
            "Số tài khoản: [REDACTED_BANK_ACCOUNT]"
        );
    }

    #[test]
    fn test_secret_scrubber_json_and_alias() {
        let raw = "CCCD 012345678901";
        assert_eq!(SecretScrubber::mask_secrets(raw), "CCCD [REDACTED_CCCD]");

        let json_val = serde_json::json!({
            "cccd": "001098012345",
            "stk": "98765432101234",
            "phone": "0912345678",
            "customer": "Nguyen Van A"
        });
        let scrubbed_json = SecretScrubber::scrub_json(&json_val);
        assert_eq!(scrubbed_json["cccd"], "[REDACTED_CCCD]");
        assert_eq!(scrubbed_json["stk"], "[REDACTED_BANK_ACCOUNT]");
        assert_eq!(scrubbed_json["phone"], "[REDACTED_PHONE]");
        assert_eq!(scrubbed_json["customer"], "Nguyen Van A");
    }

    #[test]
    fn test_secret_scrubber_composite_payload() {
        let composite = "Hồ sơ: API sk-live-99887766554433221100, CCCD 001098012345, SĐT 0912345678, STK: 123456789012 tại Techcombank";
        let scrubbed_comp = SecretScrubber::scrub(composite);
        assert!(!scrubbed_comp.contains("sk-live-99887766554433221100"));
        assert!(!scrubbed_comp.contains("001098012345"));
        assert!(!scrubbed_comp.contains("0912345678"));
        assert!(!scrubbed_comp.contains("123456789012"));
        assert_eq!(
            scrubbed_comp,
            "Hồ sơ: API [REDACTED_API_KEY], CCCD [REDACTED_CCCD], SĐT [REDACTED_PHONE], STK: [REDACTED_BANK_ACCOUNT] tại Techcombank"
        );
    }
}
