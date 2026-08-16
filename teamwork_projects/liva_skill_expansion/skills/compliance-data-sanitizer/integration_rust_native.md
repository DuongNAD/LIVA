# Rust Native Core Integration: compliance-data-sanitizer

## 1. Native Engine Architecture Alignment

### 1.1 Module Domain & Routing
- **Native Implementation**: `liva-native-core/src/commands/compliance.rs`, `liva-native-core/src/consent.rs`, `liva-native-core/src/db.rs`, `liva-native-core/src/llm/tool_calling.rs`.
- **Command Routing Matrix**:
  - `compliance:scan_pii`: Scans text or structured objects for PII/PHI and credentials. Gated under `ExecPolicy::Auto`.
  - `compliance:mask_payload`: Generates masked text using surrogate tokens (`[REDACTED_CCCD_1]`). Gated under `ExecPolicy::Auto`.
  - `compliance:tokenize_reversible`: Encrypts raw entity mappings with AES-256-GCM and stores in Privacy Vault. Gated under `ExecPolicy::ProposeOnly`.
  - `compliance:audit_report`: Calculates compliance scores against Decree 13 and GDPR. Gated under `ExecPolicy::Auto`.
  - `write_markdown`: Persists privacy reports into `vault/Knowledge/Privacy Audit - <Title>.md`. Gated under `ExecPolicy::ProposeOnly`.

### 1.2 Execution Policy (`ExecPolicy`)
```rust
// In liva-native-core/src/llm/tool_calling.rs
pub fn for_tool(server: &str, name: &str) -> Self {
    match (server, name) {
        ("compliance", "scan_pii") | ("compliance", "mask_payload") | ("compliance", "audit_report") => Self::Auto,
        ("compliance", "tokenize_reversible") => Self::ProposeOnly,
        ("obsidian", "write_markdown") => Self::ProposeOnly,
        _ => Self::ProposeOnly,
    }
}
```

---

## 2. Tool Schema Mapping & Compact Prompt Rendering

### 2.1 Compact Tool Signatures
```
[1] compliance:scan_pii: Scan payload for multi-lingual PII, Vietnamese CCCD, phone, and secrets
   tham số (* = bắt buộc): text* (string), language_hint (string: vi|en|auto)
[2] compliance:mask_payload: Replace detected PII/PHI with deterministic surrogate tokens
   tham số (* = bắt buộc): text* (string), reversible (boolean), mask_style (string: surrogate|redact)
[3] compliance:tokenize_reversible: Store encrypted entity mappings in local AES-256-GCM vault
   tham số (* = bắt buộc): token_mappings* (string), vault_session_id (string)
[4] compliance:audit_report: Evaluate compliance of dataset against Decree 13 and GDPR
   tham số (* = bắt buộc): entity_summary_json* (string), jurisdiction* (string: vietnam_decree13|eu_gdpr|global)
```

### 2.2 Input Schema & Validation Matrix
```json
{
  "compliance:scan_pii": {
    "type": "object",
    "properties": {
      "text": { "type": "string", "minLength": 1, "description": "Text to inspect for PII/PHI" },
      "language_hint": { "type": "string", "enum": ["vi", "en", "auto"], "default": "auto" }
    },
    "required": ["text"]
  },
  "compliance:mask_payload": {
    "type": "object",
    "properties": {
      "text": { "type": "string", "minLength": 1, "description": "Text to mask" },
      "reversible": { "type": "boolean", "default": false },
      "mask_style": { "type": "string", "enum": ["surrogate", "redact"], "default": "surrogate" }
    },
    "required": ["text"]
  },
  "compliance:tokenize_reversible": {
    "type": "object",
    "properties": {
      "token_mappings": { "type": "string", "minLength": 2, "description": "JSON map of surrogate to plaintext" },
      "vault_session_id": { "type": "string", "description": "Optional session identifier" }
    },
    "required": ["token_mappings"]
  },
  "compliance:audit_report": {
    "type": "object",
    "properties": {
      "entity_summary_json": { "type": "string", "minLength": 2 },
      "jurisdiction": { "type": "string", "enum": ["vietnam_decree13", "eu_gdpr", "global"], "default": "vietnam_decree13" }
    },
    "required": ["entity_summary_json"]
  }
}
```

---

## 3. Privacy Vault & AES-256-GCM Storage Schema

### 3.1 SQLite WAL Schema for Encrypted Tokens & Audit Ledger
```sql
-- Encrypted Privacy Vault for Reversible Tokens
CREATE TABLE IF NOT EXISTS privacy_vault_tokens (
    token_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    surrogate_placeholder TEXT NOT NULL,
    encrypted_plaintext BLOB NOT NULL, -- AES-256-GCM ciphertext
    nonce BLOB NOT NULL,               -- 96-bit (12 bytes) IV
    tag BLOB NOT NULL,                 -- 128-bit (16 bytes) GCM auth tag
    entity_type TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL
);

-- Immutable Compliance Audit Ledger (Zero plaintext)
CREATE TABLE IF NOT EXISTS compliance_audit_ledger (
    audit_id TEXT PRIMARY KEY,
    original_hash TEXT NOT NULL,       -- SHA-256 of original input
    sanitized_hash TEXT NOT NULL,      -- SHA-256 of sanitized output
    entities_detected_json TEXT NOT NULL, -- e.g. {"cccd": 2, "phone": 1}
    jurisdiction TEXT NOT NULL,
    compliance_rating TEXT NOT NULL,   -- COMPLIANT, WARNING, VIOLATION
    operator_principal TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### 3.2 Vietnamese PII Regex & Luhn Heuristics
```rust
use regex::Regex;
use std::sync::LazyLock;

static VN_CCCD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b0\d{2}[0-3]\d{2}\d{6}\b").unwrap()
});

static VN_PHONE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\+84|0)(3|5|7|8|9)\d{8}\b").unwrap()
});

pub fn is_valid_luhn_credit_card(card_number: &str) -> bool {
    let digits: Vec<u32> = card_number
        .chars()
        .filter_map(|c| c.to_digit(10))
        .collect();

    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }

    let mut sum = 0;
    let mut double = false;

    for &digit in digits.iter().rev() {
        if double {
            let d = digit * 2;
            sum += if d > 9 { d - 9 } else { d };
        } else {
            sum += digit;
        }
        double = !double;
    }

    sum % 10 == 0
}
```

---

## 4. Fail-Closed Security & Sandboxing Constraints

### 4.1 Cryptographic Key Zeroization & Memory Safety
- Master encryption keys are protected using `secrecy::SecretBox` or `zeroize::ZeroizeOnDrop` to ensure keys are purged from RAM immediately upon cipher completion.
- Reversible token retrieval strictly enforces user consent check via `consent::load().is_capture_allowed()`.

### 4.2 Principal RBAC Authorization
- Principal `CommandPrincipal::TauriDashboard` and `CommandPrincipal::WebSocketDashboard`: Authorized for scanning, masking, and encrypted vault operations.
- Principal `CommandPrincipal::Telegram`: Authorized for read-only sanitization checks; de-anonymization access is strictly denied.

---

## 5. Verification Checklist & Unit Test Scenarios

### 5.1 Verification Checklist
- [x] Frontmatter in `SKILL.md` conforms strictly to `name` and `description` only.
- [x] Frontmatter in generated vault notes conforms strictly to `title`, `tags`, `author`, `last_update`.
- [x] Vietnamese CCCD (12 digits, valid province & century code) is detected reliably.
- [x] Vietnamese mobile phone numbers (`09x`, `08x`, `07x`, `03x`, `+84`) are masked deterministically.
- [x] Luhn algorithm validates credit card numbers before masking.
- [x] AES-256-GCM cipher generates unique 96-bit nonces per token.

### 5.2 Unit Test Scenarios

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vietnamese_pii_detection() {
        let text = "Khách hàng Nguyễn Văn A có CCCD 001095012345 và SĐT 0912345678.";
        assert!(VN_CCCD_REGEX.is_match(text));
        assert!(VN_PHONE_REGEX.is_match(text));
    }

    #[test]
    fn test_luhn_credit_card_validation() {
        // Standard test card numbers (Luhn valid)
        assert!(is_valid_luhn_credit_card("4532015112830366"));
        // Invalid check digit
        assert!(!is_valid_luhn_credit_card("4532015112830367"));
    }

    #[test]
    fn test_compliance_tool_exec_policy() {
        assert_eq!(ExecPolicy::for_tool("compliance", "scan_pii"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool("compliance", "mask_payload"), ExecPolicy::Auto);
        assert_eq!(ExecPolicy::for_tool("compliance", "tokenize_reversible"), ExecPolicy::ProposeOnly);
    }
}
```
