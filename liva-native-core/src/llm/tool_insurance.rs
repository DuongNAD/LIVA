//! 4-Tier Tool Insurance Engine with HMAC Tool Receipts and Read-After-Write Verification (Feature F12).
//!
//! Tiers:
//! 1. Native Tools Parameter: Passes OpenAI-compliant tools parameter based on caller privilege.
//! 2. Closed Grammar Routing: Enforces closed enum routing via JSON Schema or BNF grammar.
//! 3. Argument Validation & Escalation: Validates arguments against schema with max 1 retry; escalates to worker on 2nd failure or chain > 2.
//! 4. HMAC Tool Receipt: Signs immutable execution receipt with HMAC-SHA256 and read-after-write verification.

use crate::llm::scoped_tool_registry::CommandPrincipal;
use crate::llm::tool_calling::{CatalogTool, validate_arguments};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Maximum allowed tool chain depth before automatically escalating to worker model.
pub const MAX_TOOL_CHAIN_DEPTH: usize = 2;

/// Maximum number of schema validation retries allowed per step.
pub const MAX_SCHEMA_RETRIES: usize = 1;

/// Cryptographic Tool Call Receipt confirming legitimate native execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCallReceipt {
    pub call_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub canonical_args_hash: String,
    pub result_hash: String,
    pub read_after_write_verified: bool,
    pub hmac_signature: String,
    pub timestamp_ms: u64,
}

/// Action decision resulting from the tool insurance evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum InsuranceAction {
    /// Proceed with execution.
    Proceed,
    /// Schema validation failed on 1st attempt; retry with feedback prompt.
    RetryWithFeedback {
        feedback_prompt: String,
        retry_count: usize,
    },
    /// Escalation required to heavy worker model.
    EscalateToWorker {
        reason: EscalationReason,
    },
    /// Tool call rejected due to privilege or policy denial.
    Reject {
        reason: String,
    },
}

/// Reason for escalating from lightweight router to worker model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscalationReason {
    SchemaValidationFailedTwice(String),
    ToolChainLimitExceeded(usize),
    HighRiskActionRequiresWorker(String),
}

/// Pure-Rust RFC 2104 compliant HMAC-SHA256 implementation using workspace `sha2` crate.
pub fn compute_hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut k_prime = [0u8; 64];
    if key.len() > 64 {
        let hash = Sha256::digest(key);
        k_prime[..32].copy_from_slice(&hash);
    } else {
        k_prime[..key.len()].copy_from_slice(key);
    }

    let mut i_key_pad = [0u8; 64];
    let mut o_key_pad = [0u8; 64];
    for i in 0..64 {
        i_key_pad[i] = k_prime[i] ^ 0x36;
        o_key_pad[i] = k_prime[i] ^ 0x5c;
    }

    let mut inner = Sha256::new();
    inner.update(&i_key_pad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(&o_key_pad);
    outer.update(&inner_hash);
    let outer_hash = outer.finalize();

    let mut result = [0u8; 32];
    result.copy_from_slice(&outer_hash);
    result
}

/// Constant-time verification of HMAC-SHA256 signature to prevent timing attacks.
pub fn verify_hmac_constant_time(computed: &[u8; 32], expected: &[u8]) -> bool {
    if expected.len() != 32 {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in computed.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Computes a canonical SHA-256 hash of JSON arguments with keys sorted recursively.
pub fn canonical_json_hash(val: &Value) -> String {
    let canonical_str = canonical_json_string(val);
    let hash = Sha256::digest(canonical_str.as_bytes());
    hex::encode(hash)
}

fn canonical_json_string(val: &Value) -> String {
    match val {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut parts = Vec::with_capacity(keys.len());
            for k in keys {
                parts.push(format!("\"{}\":{}", k, canonical_json_string(&map[k])));
            }
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(canonical_json_string).collect();
            format!("[{}]", parts.join(","))
        }
        _ => serde_json::to_string(val).unwrap_or_default(),
    }
}

/// Verifier and issuer of HMAC tool receipts.
#[derive(Debug, Clone)]
pub struct HmacReceiptVerifier {
    session_secret: [u8; 32],
}

impl HmacReceiptVerifier {
    pub fn new(session_secret: [u8; 32]) -> Self {
        Self { session_secret }
    }

    pub fn generate_receipt(
        &self,
        session_id: &str,
        call_id: &str,
        tool_name: &str,
        arguments: &Value,
        result_content: &str,
        read_after_write_verified: bool,
    ) -> ToolCallReceipt {
        let canonical_args_hash = canonical_json_hash(arguments);
        let result_hash = hex::encode(Sha256::digest(result_content.as_bytes()));

        // Format message: call_id|tool_name|canonical_args_hash|result_hash
        let payload = format!("{call_id}|{tool_name}|{canonical_args_hash}|{result_hash}");
        let hmac_bytes = compute_hmac_sha256(&self.session_secret, payload.as_bytes());
        let hmac_signature = hex::encode(hmac_bytes);

        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        ToolCallReceipt {
            call_id: call_id.to_string(),
            session_id: session_id.to_string(),
            tool_name: tool_name.to_string(),
            canonical_args_hash,
            result_hash,
            read_after_write_verified,
            hmac_signature,
            timestamp_ms,
        }
    }

    pub fn verify_receipt(&self, receipt: &ToolCallReceipt) -> Result<(), String> {
        let payload = format!(
            "{}|{}|{}|{}",
            receipt.call_id, receipt.tool_name, receipt.canonical_args_hash, receipt.result_hash
        );
        let expected_bytes = compute_hmac_sha256(&self.session_secret, payload.as_bytes());
        let sig_bytes = match hex::decode(&receipt.hmac_signature) {
            Ok(b) => b,
            Err(_) => return Err("Invalid hex encoding in HMAC signature".into()),
        };

        if !verify_hmac_constant_time(&expected_bytes, &sig_bytes) {
            return Err("HMAC signature verification failed (forged receipt)".into());
        }

        Ok(())
    }
}

/// 4-Tier Tool Insurance Engine.
#[derive(Debug, Clone)]
pub struct ToolInsuranceEngine {
    verifier: HmacReceiptVerifier,
}

impl ToolInsuranceEngine {
    pub fn new(session_secret: [u8; 32]) -> Self {
        Self {
            verifier: HmacReceiptVerifier::new(session_secret),
        }
    }

    pub fn verifier(&self) -> &HmacReceiptVerifier {
        &self.verifier
    }

    // --- TIER 1: NATIVE TOOLS PARAMETER FORMATTING ---
    pub fn format_native_tools(
        &self,
        tools: &[CatalogTool],
        principal: CommandPrincipal,
    ) -> Vec<Value> {
        let is_owner = principal.privilege_level() >= CommandPrincipal::LocalCli.privilege_level();
        tools
            .iter()
            .filter(|t| {
                if !is_owner && (t.name.contains("dpapi") || t.name.contains("policy") || t.name.contains("write_markdown")) {
                    false
                } else {
                    true
                }
            })
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema
                    }
                })
            })
            .collect()
    }

    // --- TIER 2: CLOSED GRAMMAR ENUM ROUTING ---
    pub fn generate_closed_enum_schema(&self, tools: &[CatalogTool]) -> Value {
        let mut enum_names = vec!["none".to_string()];
        for t in tools {
            enum_names.push(t.name.clone());
        }
        json!({
            "type": "object",
            "properties": {
                "tool": {
                    "type": "string",
                    "enum": enum_names
                },
                "arguments": {
                    "type": "object"
                }
            },
            "required": ["tool", "arguments"],
            "additionalProperties": false
        })
    }

    // --- TIER 3: SCHEMA ARGUMENT VALIDATOR & ESCALATION PROTOCOL ---
    pub fn inspect_and_insure(
        &self,
        tool: &CatalogTool,
        arguments: &Value,
        retry_count: usize,
        chain_step: usize,
    ) -> InsuranceAction {
        // Step 1: Check chain depth limit
        if chain_step > MAX_TOOL_CHAIN_DEPTH {
            return InsuranceAction::EscalateToWorker {
                reason: EscalationReason::ToolChainLimitExceeded(chain_step),
            };
        }

        // Step 2: Validate arguments against JSON Schema
        if let Err(err) = validate_arguments(&tool.input_schema, arguments) {
            if retry_count < MAX_SCHEMA_RETRIES {
                let feedback_prompt = format!(
                    "Tool '{}' argument validation failed: {}. Please fix the arguments and provide valid JSON.",
                    tool.name, err
                );
                return InsuranceAction::RetryWithFeedback {
                    feedback_prompt,
                    retry_count: retry_count + 1,
                };
            } else {
                return InsuranceAction::EscalateToWorker {
                    reason: EscalationReason::SchemaValidationFailedTwice(err),
                };
            }
        }

        InsuranceAction::Proceed
    }

    // --- TIER 4: HMAC RECEIPT ISSUANCE & READ-AFTER-WRITE VERIFICATION ---
    pub fn issue_signed_receipt(
        &self,
        session_id: &str,
        call_id: &str,
        tool_name: &str,
        arguments: &Value,
        result_content: &str,
        is_mutating: bool,
        raw_confirmed: bool,
    ) -> Result<ToolCallReceipt, String> {
        if is_mutating && !raw_confirmed {
            return Err(format!(
                "Read-after-write verification failed for state-mutating tool '{}'",
                tool_name
            ));
        }

        Ok(self.verifier.generate_receipt(
            session_id,
            call_id,
            tool_name,
            arguments,
            result_content,
            if is_mutating { raw_confirmed } else { true },
        ))
    }
}
