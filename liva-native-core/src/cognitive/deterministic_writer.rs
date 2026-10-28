//! Deterministic Post-Turn Writer & MemGhost Defense Engine (Feature F16).
//!
//! Provides:
//! - Sub-5ms rule-based extraction using static compiled DFA regexes.
//! - Immediate remember and forget command processing.
//! - Anti-hallucination grounding gate verifying entity and metric presence in source evidence.
//! - MemGhost defense gate preventing untrusted channels and assistant prose from mutating
//!   protected user profile or preferences.
//! - 4-operation bitemporal reconciliation: `ADD`, `EXTEND`, `NOOP`, `INVALIDATE`.
//! - Fact promotion evaluator for Tier 1 Core Memory (USER.md).

use crate::db::bitemporal::{
    FactOrigin, FactSensitivity, SourceChannel, atomic_supersede_fact, get_active_fact,
    ingest_evidence_log, insert_active_fact,
};
use liva_core_types::CommandPrincipal;
use regex::Regex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

// ============================================================================
// COMPILED REGEXES CHO BỘ TRÍCH XUẤT LUẬT (< 5MS)
// ============================================================================

/// Lệnh nhớ tường minh của người dùng
static RE_EXPLICIT_REMEMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[,.\s]+)(?:hãy\s+)?(?:ghi\s*nhớ|nhớ|lưu\s*lại|note)\s+(?:rằng|là|giúp\s+tôi|hộ\s+tôi)?\s*[:,\s]+(.+)")
        .expect("valid remember regex")
});

/// Lệnh quên tường minh của người dùng
static RE_EXPLICIT_FORGET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[,.\s]+)(?:hãy\s+)?(?:quên|xoá|hủy|bỏ|đừng\s+nhớ)\s+(?:rằng|là|đi)?\s*[:,\s]+(.+)")
        .expect("valid forget regex")
});

/// Biểu thức sở thích trực tiếp của người dùng
static RE_USER_PREFERENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\btôi\s+(?:rất\s+)?(?:thích|yêu\s+thích|chuộng|đam\s+mê)\s+([^.,;!?\n]+)")
        .expect("valid preference regex")
});

/// Biểu thức người dùng không còn thích
static RE_USER_DISLIKE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\btôi\s+(?:không\s+còn\s+thích|bỏ\s+thích|chán|không\s+thích\s+nữa)\s+([^.,;!?\n]+)",
    )
    .expect("valid dislike regex")
});

/// Số điện thoại di động Việt Nam (10 chữ số, đầu mạng 03, 05, 07, 08, 09)
static RE_VIETNAMESE_PHONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(0[35789]\d{8})\b").expect("valid phone regex"));

/// Số tài khoản ngân hàng Việt Nam (8 đến 16 số đi liền sau từ khoá STK, tài khoản)
static RE_BANK_ACCOUNT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:stk|số\s+tài\s+khoản|tài\s+khoản\s+số)\s*[:.\s]*([0-9]{8,16})\b")
        .expect("valid bank account regex")
});

/// Ngày tháng tiếng Việt chuẩn (d/m/y hoặc d-m-y)
static RE_VIETNAMESE_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:ngày\s+)?(\d{1,2})[/-](\d{1,2})(?:[/-](\d{2,4}))?\b")
        .expect("valid date regex")
});

/// Tiền tệ VND, USD, K, Triệu
static RE_CURRENCY_METRIC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(\d+(?:[.,]\d+)*)\s*(đ|₫|vnd|vnđ|usd|\$|k|tr|triệu|tỷ)\b")
        .expect("valid currency regex")
});

/// Từ ngữ thời gian tương đối tiếng Việt
static RE_RELATIVE_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(hôm\s+nay|ngày\s+mai|hôm\s+qua|tuần\s+sau|tháng\s+sau|thứ\s+[2-7]|chủ\s+nhật)\b",
    )
    .expect("valid relative time regex")
});

/// Chuỗi chữ số từ 2 ký tự trở lên (kiểm chứng số liệu chống ảo giác)
static RE_DIGITS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d{2,}\b").expect("valid digits regex"));

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Thao tác đối soát bộ nhớ tất định.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationAction {
    Add,
    Extend,
    Noop,
    Invalidate,
}

pub type BitemporalReconcileOp = ReconciliationAction;
pub type FactCandidate = CandidateFact;
pub type PostTurnWriteResult = ReconciliationOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelIntegrity {
    Trusted,
    Untrusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Loại thực thể trích xuất theo luật.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotType {
    DateTime,
    CurrencyAmount,
    PhoneNumber,
    BankAccount,
    UserPreference,
    ExplicitRemember,
    ExplicitForget,
}

/// Giá trị trích xuất được từ câu thoại.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedSlot {
    pub slot_type: SlotType,
    pub raw_text: String,
    pub normalized_value: String,
    pub confidence: f64,
}

/// Ứng viên Fact chuẩn bị đưa vào thẩm định và đối soát.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateFact {
    pub subject: String,
    pub predicate: String,
    pub scope: String,
    pub value: String,
    pub origin: FactOrigin,
    pub confidence: f64,
    pub source_evidence_hash: String,
    pub source_role: String, // "user" hoặc "assistant"
}

/// Kết quả kiểm chứng chống bịa đặt (Anti-Hallucination).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntiHallucinationResult {
    Grounded,
    Rejected {
        reason: String,
        missing_entity_or_metric: String,
    },
}

/// Kết quả lọc phòng vệ MemGhost.
#[derive(Debug, Clone, PartialEq)]
pub enum MemGhostFilterResult {
    Permitted,
    QuarantinedToUntrustedScope {
        redirected_scope: String,
        downgraded_confidence: f64,
    },
    RejectedBlocked {
        reason: String,
    },
}

/// Kết quả đối soát cuối cùng của một lượt ghi.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationOutcome {
    pub action: ReconciliationAction,
    pub fact_id: String,
    pub superseded_fact_id: Option<String>,
    pub message: String,
}

// ============================================================================
// RULE-BASED SLOT EXTRACTOR (< 5MS)
// ============================================================================

/// Trích xuất tín hiệu theo quy tắc sử dụng precompiled regexes (< 5ms).
pub fn extract_rule_based_signals(input_text: &str) -> Vec<ExtractedSlot> {
    let nfc_text: String = input_text.nfc().collect();
    let trimmed = nfc_text.trim();
    let mut slots = Vec::new();

    // 1. Kiểm tra lệnh nhớ tường minh
    if let Some(caps) = RE_EXPLICIT_REMEMBER.captures(trimmed) {
        if let Some(m) = caps.get(1) {
            slots.push(ExtractedSlot {
                slot_type: SlotType::ExplicitRemember,
                raw_text: m.as_str().to_string(),
                normalized_value: m.as_str().trim().to_string(),
                confidence: 1.0,
            });
        }
    }

    // 2. Kiểm tra lệnh quên tường minh
    if let Some(caps) = RE_EXPLICIT_FORGET.captures(trimmed) {
        if let Some(m) = caps.get(1) {
            slots.push(ExtractedSlot {
                slot_type: SlotType::ExplicitForget,
                raw_text: m.as_str().to_string(),
                normalized_value: m.as_str().trim().to_string(),
                confidence: 1.0,
            });
        }
    }

    // 3. Sở thích người dùng
    for caps in RE_USER_PREFERENCE.captures_iter(&nfc_text) {
        if let Some(m) = caps.get(1) {
            slots.push(ExtractedSlot {
                slot_type: SlotType::UserPreference,
                raw_text: m.as_str().to_string(),
                normalized_value: m.as_str().trim().to_string(),
                confidence: 0.95,
            });
        }
    }

    // 4. Số tài khoản ngân hàng
    for caps in RE_BANK_ACCOUNT.captures_iter(&nfc_text) {
        if let Some(m) = caps.get(1) {
            slots.push(ExtractedSlot {
                slot_type: SlotType::BankAccount,
                raw_text: m.as_str().to_string(),
                normalized_value: m.as_str().trim().to_string(),
                confidence: 1.0,
            });
        }
    }

    // 5. Số điện thoại di động
    for caps in RE_VIETNAMESE_PHONE.captures_iter(&nfc_text) {
        if let Some(m) = caps.get(1) {
            slots.push(ExtractedSlot {
                slot_type: SlotType::PhoneNumber,
                raw_text: m.as_str().to_string(),
                normalized_value: m.as_str().trim().to_string(),
                confidence: 1.0,
            });
        }
    }

    // 6. Số tiền / Tiền tệ
    for caps in RE_CURRENCY_METRIC.captures_iter(&nfc_text) {
        let full_match = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        slots.push(ExtractedSlot {
            slot_type: SlotType::CurrencyAmount,
            raw_text: full_match.to_string(),
            normalized_value: full_match.trim().to_string(),
            confidence: 0.90,
        });
    }

    // 7. Ngày tháng
    for caps in RE_VIETNAMESE_DATE.captures_iter(&nfc_text) {
        let full_match = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        slots.push(ExtractedSlot {
            slot_type: SlotType::DateTime,
            raw_text: full_match.to_string(),
            normalized_value: full_match.trim().to_string(),
            confidence: 0.90,
        });
    }

    // 8. Từ ngữ thời gian tương đối
    for caps in RE_RELATIVE_TIME.captures_iter(&nfc_text) {
        let full_match = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        slots.push(ExtractedSlot {
            slot_type: SlotType::DateTime,
            raw_text: full_match.to_string(),
            normalized_value: full_match.trim().to_string(),
            confidence: 0.85,
        });
    }

    // 9. Biểu thức không còn thích
    for caps in RE_USER_DISLIKE.captures_iter(&nfc_text) {
        if let Some(m) = caps.get(1) {
            slots.push(ExtractedSlot {
                slot_type: SlotType::ExplicitForget,
                raw_text: m.as_str().to_string(),
                normalized_value: m.as_str().trim().to_string(),
                confidence: 0.95,
            });
        }
    }

    slots
}

// ============================================================================
// ANTI-HALLUCINATION GROUNDING GATE
// ============================================================================

/// Candidate extracted by rule-based DFA analyzer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFactCandidate {
    pub subject: String,
    pub predicate: String,
    pub object_value: String,
    pub scope: String,
    pub confidence: f64,
    pub rule_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct FactExtractionRule {
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct AntiHallucinationVerifier;

impl AntiHallucinationVerifier {
    pub fn new() -> Self {
        Self
    }

    /// Verify candidate with ExtractedFactCandidate contract.
    pub fn verify_candidate(
        &self,
        candidate: &ExtractedFactCandidate,
        evidence_text_nfc: &str,
    ) -> bool {
        let fake = CandidateFact {
            subject: candidate.subject.clone(),
            predicate: candidate.predicate.clone(),
            value: candidate.object_value.clone(),
            scope: candidate.scope.clone(),
            origin: FactOrigin::Inferred,
            confidence: candidate.confidence,
            source_evidence_hash: String::new(),
            source_role: "user".into(),
        };
        matches!(
            Self::verify(&fake, evidence_text_nfc),
            AntiHallucinationResult::Grounded
        )
    }

    /// Verify that candidate numbers, metrics, and entities actually exist in evidence text.
    pub fn verify(candidate: &CandidateFact, evidence_text_nfc: &str) -> AntiHallucinationResult {
        let evidence_lower = evidence_text_nfc.to_lowercase();
        let value_nfc: String = candidate.value.nfc().collect();
        let value_lower = value_nfc.to_lowercase();

        // 1. Strict numeric check: all 2+ digit numbers in fact value MUST appear in evidence
        for mat in RE_DIGITS.find_iter(&value_nfc) {
            let digit_str = mat.as_str();
            if !evidence_text_nfc.contains(digit_str) {
                return AntiHallucinationResult::Rejected {
                    reason: "Metric or digit sequence not found in source evidence".into(),
                    missing_entity_or_metric: digit_str.to_string(),
                };
            }
        }

        // 2. Token grounding recall check
        let tokens: Vec<&str> = value_lower.split_whitespace().collect();
        if !tokens.is_empty() {
            let matched_tokens = tokens
                .iter()
                .filter(|&&t| t.len() > 2 && evidence_lower.contains(t))
                .count();
            let recall_ratio = matched_tokens as f64 / tokens.len() as f64;
            if recall_ratio < 0.60 && !evidence_lower.contains(&value_lower) {
                return AntiHallucinationResult::Rejected {
                    reason: "Low token grounding recall in source evidence".into(),
                    missing_entity_or_metric: candidate.value.clone(),
                };
            }
        }

        AntiHallucinationResult::Grounded
    }
}

// ============================================================================
// MEMGHOST DEFENSE GATE
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct MemGhostFilter;

impl MemGhostFilter {
    pub fn new() -> Self {
        Self
    }

    /// Canonical security mapping for communication channels.
    ///
    /// Security invariant: Telegram, Zalo, ExternalWeb, and WebRss are strictly
    /// classified as untrusted channels with privilege level 1 (CommandPrincipal::Telegram).
    /// Under NO circumstances can external or untrusted channels be granted LocalCli (Owner level 5).
    pub fn channel_security(channel: SourceChannel) -> (CommandPrincipal, bool) {
        match channel {
            SourceChannel::Telegram | SourceChannel::Zalo => (CommandPrincipal::Telegram, true),
            SourceChannel::ExternalWeb | SourceChannel::WebRss => {
                (CommandPrincipal::Telegram, true)
            }
            SourceChannel::Tauri => (CommandPrincipal::TauriDashboard, false),
            SourceChannel::LocalCli | SourceChannel::Cli => (CommandPrincipal::LocalCli, false),
        }
    }

    /// Check if channel is authorized to record fact candidate.
    pub fn is_allowed(&self, candidate: &ExtractedFactCandidate, channel: SourceChannel) -> bool {
        let (principal, is_untrusted) = Self::channel_security(channel);
        let fake = CandidateFact {
            subject: candidate.subject.clone(),
            predicate: candidate.predicate.clone(),
            value: candidate.object_value.clone(),
            scope: candidate.scope.clone(),
            origin: FactOrigin::Explicit,
            confidence: candidate.confidence,
            source_evidence_hash: String::new(),
            source_role: "user".into(),
        };
        matches!(
            Self::evaluate(&fake, principal, is_untrusted),
            MemGhostFilterResult::Permitted
        )
    }

    /// Evaluate security clearance and data taint against memory injection.
    pub fn evaluate(
        candidate: &CandidateFact,
        principal: CommandPrincipal,
        is_untrusted_channel: bool,
    ) -> MemGhostFilterResult {
        let is_protected_scope = matches!(
            candidate.scope.as_str(),
            "personal" | "user_preference" | "core_profile" | "financial" | "system_instruction"
        );
        let is_protected_predicate = candidate.predicate.starts_with("preference_")
            || candidate.predicate.starts_with("default_")
            || candidate.predicate.contains("bank_account")
            || candidate.predicate.contains("tai_khoan")
            || candidate.predicate.contains("stk")
            || candidate.predicate.contains("password")
            || candidate.predicate.contains("api_key");

        // RULE 1: Untrusted channel is strictly forbidden from mutating protected profile
        if is_untrusted_channel {
            if is_protected_scope || is_protected_predicate {
                return MemGhostFilterResult::RejectedBlocked {
                    reason: "MemGhost Defense: Untrusted channel origin is strictly prohibited from mutating protected user profile or preferences".into(),
                };
            }

            return MemGhostFilterResult::QuarantinedToUntrustedScope {
                redirected_scope: "untrusted_scratchpad".into(),
                downgraded_confidence: 0.35,
            };
        }

        // RULE 2: Assistant output cannot autonomously create or mutate user preferences
        if candidate.source_role == "assistant" && (is_protected_scope || is_protected_predicate) {
            return MemGhostFilterResult::RejectedBlocked {
                reason: "MemGhost Defense: Assistant prose output cannot autonomously create or mutate user profile preferences".into(),
            };
        }

        // RULE 3: Only verified owner principals can write to protected personal scope
        let is_owner_principal = matches!(
            principal,
            CommandPrincipal::LocalCli
                | CommandPrincipal::TauriDashboard
                | CommandPrincipal::TauriWidget
                | CommandPrincipal::Test
        );

        if (is_protected_scope || is_protected_predicate) && !is_owner_principal {
            return MemGhostFilterResult::RejectedBlocked {
                reason: format!(
                    "Insufficient privilege level for principal {principal:?} to alter protected user core memory"
                ),
            };
        }

        MemGhostFilterResult::Permitted
    }
}

// ============================================================================
// 4-OPERATION RECONCILIATION ENGINE
// ============================================================================

pub struct DeterministicReconciliationEngine;

impl DeterministicReconciliationEngine {
    /// Reconcile a candidate fact against current active state, enforcing Active Singleton Invariant.
    pub fn reconcile(
        conn: &Connection,
        candidate: &CandidateFact,
        evidence_id: &str,
        now_epoch_ms: i64,
    ) -> Result<ReconciliationOutcome, rusqlite::Error> {
        let existing = get_active_fact(
            conn,
            &candidate.subject,
            &candidate.predicate,
            &candidate.scope,
        )?;

        match existing {
            Some(existing_fact) => {
                if existing_fact.object_value == candidate.value {
                    // Identical value: check confidence
                    if candidate.confidence <= existing_fact.confidence {
                        Ok(ReconciliationOutcome {
                            action: ReconciliationAction::Noop,
                            fact_id: existing_fact.fact_id,
                            superseded_fact_id: None,
                            message: "Identical active fact already recorded with equal or higher confidence".into(),
                        })
                    } else {
                        // Higher confidence or corroborating evidence -> EXTEND
                        conn.execute(
                            "UPDATE facts SET confidence = ?1, updatedAt = ?2, updated_at = ?2 WHERE fact_id = ?3",
                            rusqlite::params![candidate.confidence, now_epoch_ms, &existing_fact.fact_id],
                        )?;
                        conn.execute(
                            "INSERT OR IGNORE INTO fact_evidence (fact_id, evidence_id, confidence_weight, extracted_at) VALUES (?1, ?2, 1.0, ?3)",
                            rusqlite::params![&existing_fact.fact_id, evidence_id, now_epoch_ms],
                        )?;
                        Ok(ReconciliationOutcome {
                            action: ReconciliationAction::Extend,
                            fact_id: existing_fact.fact_id,
                            superseded_fact_id: None,
                            message: "Extended active fact validity and linked new corroborating evidence".into(),
                        })
                    }
                } else {
                    // Different value -> INVALIDATE / SUPERSEDE
                    let new_fact_id = atomic_supersede_fact(
                        conn,
                        &existing_fact.fact_id,
                        &candidate.value,
                        candidate.origin,
                        candidate.confidence,
                        now_epoch_ms,
                        Some("contradiction_superseded"),
                        Some(evidence_id),
                    )?;

                    Ok(ReconciliationOutcome {
                        action: ReconciliationAction::Invalidate,
                        fact_id: new_fact_id,
                        superseded_fact_id: Some(existing_fact.fact_id),
                        message:
                            "Invalidated old conflicting fact and established new active singleton"
                                .into(),
                    })
                }
            }
            None => {
                // Fresh active singleton -> ADD
                let new_fact = crate::db::bitemporal::BitemporalFact {
                    fact_id: String::new(),
                    subject: candidate.subject.clone(),
                    predicate: candidate.predicate.clone(),
                    value: candidate.value.clone(),
                    object_value: candidate.value.clone(),
                    scope: candidate.scope.clone(),
                    origin: candidate.origin,
                    confidence: candidate.confidence,
                    valid_from: now_epoch_ms,
                    valid_to: None,
                    recorded_at: now_epoch_ms,
                    expired_at: None,
                    superseded_by: None,
                    reason: None,
                    supersede_reason: None,
                    category: None,
                    key: None,
                    importance: 5,
                    sensitivity: FactSensitivity::Normal,
                    created_at: now_epoch_ms,
                    updated_at: now_epoch_ms,
                };

                let new_fact_id = insert_active_fact(conn, &new_fact, Some(evidence_id))?;

                Ok(ReconciliationOutcome {
                    action: ReconciliationAction::Add,
                    fact_id: new_fact_id,
                    superseded_fact_id: None,
                    message: "Inserted fresh active singleton fact".into(),
                })
            }
        }
    }
}

// ============================================================================
// FACT PROMOTION EVALUATOR
// ============================================================================

pub struct FactPromotionEvaluator;

impl FactPromotionEvaluator {
    /// Evaluate whether an active fact is eligible for Tier 1 Core Memory (USER.md).
    ///
    /// Rules:
    /// 1. `origin == Explicit` or `UserEdited` -> immediately eligible.
    /// 2. `origin == Inferred`: candidate facts with < 3 distinct session co-occurrences
    ///    remain pending/candidate; promoted upon reaching >= 3 distinct sessions.
    ///    Must join `fact_evidence` with `evidence_log` and count distinct non-empty `session_id`s.
    pub fn is_eligible_for_core_profile(
        conn: &Connection,
        fact_id: &str,
    ) -> Result<bool, rusqlite::Error> {
        let origin: String = conn.query_row(
            "SELECT origin FROM facts WHERE fact_id = ?1",
            [fact_id],
            |r| r.get(0),
        )?;

        if origin == "explicit" || origin == "user_edited" {
            return Ok(true);
        }

        let distinct_session_count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT el.session_id) \
             FROM fact_evidence fe \
             JOIN evidence_log el ON fe.evidence_id = el.evidence_id \
             WHERE fe.fact_id = ?1 AND el.session_id IS NOT NULL AND el.session_id != ''",
            [fact_id],
            |r| r.get(0),
        )?;

        Ok(distinct_session_count >= 3)
    }
}

// ============================================================================
// DETERMINISTIC POST-TURN WRITER
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct DeterministicPostTurnWriter;

#[derive(Debug, Clone)]
pub struct PostTurnProcessingResult {
    pub evidence_id: Option<String>,
    pub outcomes: Vec<ReconciliationOutcome>,
}

impl DeterministicPostTurnWriter {
    pub fn new() -> Self {
        Self
    }

    /// Extract candidate facts using fast compiled regex rules.
    pub fn extract_rule_based_candidates(&self, user_msg: &str) -> Vec<ExtractedFactCandidate> {
        let slots = extract_rule_based_signals(user_msg);
        let mut candidates: Vec<ExtractedFactCandidate> = slots
            .into_iter()
            .map(|s| {
                let (pred, scope) = match s.slot_type {
                    SlotType::ExplicitRemember => ("note", "personal"),
                    SlotType::ExplicitForget => ("forget", "personal"),
                    SlotType::UserPreference => ("preference", "personal"),
                    SlotType::BankAccount => ("bank_account", "personal"),
                    SlotType::PhoneNumber => ("phone_number", "personal"),
                    SlotType::CurrencyAmount => ("currency_metric", "personal"),
                    SlotType::DateTime => ("temporal_date", "personal"),
                };
                ExtractedFactCandidate {
                    subject: "user".to_string(),
                    predicate: pred.to_string(),
                    object_value: s.normalized_value,
                    scope: scope.to_string(),
                    confidence: s.confidence,
                    rule_name: format!("{:?}", s.slot_type),
                }
            })
            .collect();

        // If no explicit slots matched, check if user states name or personal preference directly
        if candidates.is_empty() {
            let nfc_msg = user_msg.trim();
            if let Some(idx) = nfc_msg.to_lowercase().find("tên là") {
                let rest = &nfc_msg[idx + "tên là".len()..];
                let name = rest.split([',', '.', ';']).next().unwrap_or("").trim();
                if !name.is_empty() {
                    candidates.push(ExtractedFactCandidate {
                        subject: "user".to_string(),
                        predicate: "name".to_string(),
                        object_value: name.to_string(),
                        scope: "personal".to_string(),
                        confidence: 0.95,
                        rule_name: "name_identity".to_string(),
                    });
                }
            }
        }

        candidates
    }

    /// Full end-to-end post-turn processor.
    pub fn process_turn(
        &self,
        conn: &Connection,
        user_msg: &str,
        source_channel: SourceChannel,
        integrity: crate::db::bitemporal::IntegrityLabel,
        turn_id: &str,
    ) -> Result<PostTurnProcessingResult, rusqlite::Error> {
        let (channel_principal, channel_untrusted) =
            MemGhostFilter::channel_security(source_channel);
        let is_untrusted = channel_untrusted
            || integrity == crate::db::bitemporal::IntegrityLabel::ExternalUntrusted
            || integrity == crate::db::bitemporal::IntegrityLabel::Untrusted;
        let principal = if is_untrusted {
            channel_principal
        } else {
            CommandPrincipal::LocalCli
        };

        let (evi_id, _) = crate::db::bitemporal::ingest_evidence_log(
            conn,
            source_channel,
            user_msg,
            integrity,
            Some(turn_id),
        )?;
        let outcomes =
            Self::process_turn_sync(conn, user_msg, "", principal, source_channel, is_untrusted)?;
        Ok(PostTurnProcessingResult {
            evidence_id: Some(evi_id),
            outcomes,
        })
    }
    /// Synchronously process turn text (<5ms), record immutable evidence, extract slots,
    /// filter MemGhost and anti-hallucination gates, and reconcile active facts.
    pub fn process_turn_sync(
        conn: &Connection,
        user_input: &str,
        assistant_output: &str,
        principal: CommandPrincipal,
        source_channel: SourceChannel,
        is_untrusted_channel: bool,
    ) -> Result<Vec<ReconciliationOutcome>, rusqlite::Error> {
        let (channel_principal, channel_untrusted) =
            MemGhostFilter::channel_security(source_channel);
        let is_untrusted_channel = is_untrusted_channel || channel_untrusted;
        let principal = if is_untrusted_channel {
            channel_principal
        } else {
            principal
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let integrity_label = if is_untrusted_channel {
            crate::db::bitemporal::IntegrityLabel::ExternalUntrusted
        } else {
            crate::db::bitemporal::IntegrityLabel::OwnerTrusted
        };

        // 1. Ingest immutable evidence
        let (evidence_id, _) =
            ingest_evidence_log(conn, source_channel, user_input, integrity_label, None)?;

        // 2. Rule-based extraction (<5ms)
        let slots = extract_rule_based_signals(user_input);
        let mut outcomes = Vec::new();

        for slot in slots {
            let (predicate, value, origin, confidence) = match slot.slot_type {
                SlotType::ExplicitRemember => (
                    "note".to_string(),
                    slot.normalized_value,
                    FactOrigin::Explicit,
                    1.0,
                ),
                SlotType::ExplicitForget => {
                    // Close active fact matching value
                    if let Ok(Some(f)) =
                        get_active_fact(conn, "user", &slot.normalized_value, "personal")
                    {
                        let _ = atomic_supersede_fact(
                            conn,
                            &f.fact_id,
                            "[FORGOTTEN]",
                            FactOrigin::Explicit,
                            1.0,
                            now_ms,
                            Some("user_explicit_forget"),
                            Some(&evidence_id),
                        );
                        outcomes.push(ReconciliationOutcome {
                            action: ReconciliationAction::Invalidate,
                            fact_id: f.fact_id,
                            superseded_fact_id: None,
                            message: "Explicit forget completed".into(),
                        });
                    }
                    continue;
                }
                SlotType::UserPreference => (
                    "preference_general".to_string(),
                    slot.normalized_value,
                    FactOrigin::Explicit,
                    slot.confidence,
                ),
                SlotType::BankAccount => (
                    "default_bank_account".to_string(),
                    slot.normalized_value,
                    FactOrigin::Inferred,
                    slot.confidence,
                ),
                SlotType::PhoneNumber => (
                    "phone_number".to_string(),
                    slot.normalized_value,
                    FactOrigin::Inferred,
                    slot.confidence,
                ),
                SlotType::CurrencyAmount => (
                    "currency_metric".to_string(),
                    slot.normalized_value,
                    FactOrigin::Inferred,
                    slot.confidence,
                ),
                SlotType::DateTime => (
                    "temporal_date".to_string(),
                    slot.normalized_value,
                    FactOrigin::Inferred,
                    slot.confidence,
                ),
            };

            let mut candidate = CandidateFact {
                subject: "user".to_string(),
                predicate,
                scope: "personal".to_string(),
                value,
                origin,
                confidence,
                source_evidence_hash: evidence_id.clone(),
                source_role: "user".to_string(),
            };

            // 3. MemGhost filter
            match MemGhostFilter::evaluate(&candidate, principal, is_untrusted_channel) {
                MemGhostFilterResult::Permitted => {}
                MemGhostFilterResult::QuarantinedToUntrustedScope {
                    redirected_scope,
                    downgraded_confidence,
                } => {
                    candidate.scope = redirected_scope;
                    candidate.confidence = downgraded_confidence;
                }
                MemGhostFilterResult::RejectedBlocked { .. } => {
                    continue;
                }
            }

            // 4. Anti-hallucination verification
            if let AntiHallucinationResult::Rejected { .. } =
                AntiHallucinationVerifier::verify(&candidate, user_input)
            {
                continue;
            }

            // 5. Reconciliation
            let outcome = DeterministicReconciliationEngine::reconcile(
                conn,
                &candidate,
                &evidence_id,
                now_ms,
            )?;
            outcomes.push(outcome);
        }

        let _ = assistant_output; // Unused for mutating user preferences (MemGhost defense)
        Ok(outcomes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::bitemporal::init_bitemporal_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO evidence_log (evidence_id, content, nfc_content, source_type, session_id, principal, channel, created_at)
             VALUES ('ev1', 'c1', 'c1', 'turn', 'sess_1', 'user', 'cli', 1000),
                    ('ev2', 'c2', 'c2', 'turn', 'sess_2', 'user', 'cli', 1000),
                    ('ev3', 'c3', 'c3', 'turn', 'sess_3', 'user', 'cli', 1000),
                    ('ev_1', 'c1b', 'c1b', 'turn', 'sess_1', 'user', 'cli', 1000),
                    ('ev_2', 'c2b', 'c2b', 'turn', 'sess_2', 'user', 'cli', 1000),
                    ('ev_3', 'c3b', 'c3b', 'turn', 'sess_3', 'user', 'cli', 1000),
                    ('ev_same_sess', 'c4', 'c4', 'turn', 'sess_1', 'user', 'cli', 1000)",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn test_extract_rule_based_signals_speed_and_accuracy() {
        let text = "Hôm nay ngày 24/10/2026, hãy ghi nhớ rằng số tài khoản ngân hàng của tôi là STK: 0912345678 và tôi thích uống cà phê sữa đá. Số điện thoại của tôi là 0987654321, tôi chuyển 500k.";

        // Warmup regex DFA and memory
        let _ = extract_rule_based_signals(text);
        let start = std::time::Instant::now();
        let slots = extract_rule_based_signals(text);
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 50,
            "Extraction must complete with low latency (took {:?})",
            elapsed
        );

        let has_remember = slots
            .iter()
            .any(|s| s.slot_type == SlotType::ExplicitRemember);
        let has_bank = slots
            .iter()
            .any(|s| s.slot_type == SlotType::BankAccount && s.normalized_value == "0912345678");
        let has_pref = slots
            .iter()
            .any(|s| s.slot_type == SlotType::UserPreference);
        let has_phone = slots
            .iter()
            .any(|s| s.slot_type == SlotType::PhoneNumber && s.normalized_value == "0987654321");
        let has_curr = slots
            .iter()
            .any(|s| s.slot_type == SlotType::CurrencyAmount);
        let has_date = slots.iter().any(|s| s.slot_type == SlotType::DateTime);

        assert!(has_remember, "Must extract remember command");
        assert!(has_bank, "Must extract bank account");
        assert!(has_pref, "Must extract user preference");
        assert!(has_phone, "Must extract phone number");
        assert!(has_curr, "Must extract currency");
        assert!(has_date, "Must extract date");
    }

    #[test]
    fn test_anti_hallucination_gate() {
        let evidence = "Tôi vừa chuyển 200.000 VND tiền ăn trưa cho Nam qua số tài khoản 04910001.";

        let valid_fact = CandidateFact {
            subject: "user".into(),
            predicate: "transaction".into(),
            scope: "personal".into(),
            value: "200.000 VND".into(),
            origin: FactOrigin::Inferred,
            confidence: 0.9,
            source_evidence_hash: "ev1".into(),
            source_role: "user".into(),
        };
        assert_eq!(
            AntiHallucinationVerifier::verify(&valid_fact, evidence),
            AntiHallucinationResult::Grounded
        );

        // Hallucinated number (500.000 instead of 200.000)
        let hallucinated_fact = CandidateFact {
            subject: "user".into(),
            predicate: "transaction".into(),
            scope: "personal".into(),
            value: "500.000 VND".into(),
            origin: FactOrigin::Inferred,
            confidence: 0.9,
            source_evidence_hash: "ev1".into(),
            source_role: "user".into(),
        };
        match AntiHallucinationVerifier::verify(&hallucinated_fact, evidence) {
            AntiHallucinationResult::Rejected {
                missing_entity_or_metric,
                ..
            } => {
                assert_eq!(missing_entity_or_metric, "500");
            }
            _ => panic!("Must reject hallucinated metric"),
        }
    }

    #[test]
    fn test_memghost_defense_untrusted_channel_and_assistant() {
        let candidate_personal = CandidateFact {
            subject: "user".into(),
            predicate: "preference_beverage".into(),
            scope: "personal".into(),
            value: "bia hơi Hà Nội".into(),
            origin: FactOrigin::Inferred,
            confidence: 0.8,
            source_evidence_hash: "ev1".into(),
            source_role: "user".into(),
        };

        // 1. Untrusted channel trying to write personal preference -> RejectedBlocked
        let res1 = MemGhostFilter::evaluate(&candidate_personal, CommandPrincipal::LocalCli, true);
        assert!(matches!(res1, MemGhostFilterResult::RejectedBlocked { .. }));

        // 2. Assistant prose trying to create personal preference -> RejectedBlocked
        let mut candidate_as_assistant = candidate_personal.clone();
        candidate_as_assistant.source_role = "assistant".into();
        let res2 =
            MemGhostFilter::evaluate(&candidate_as_assistant, CommandPrincipal::LocalCli, false);
        assert!(matches!(res2, MemGhostFilterResult::RejectedBlocked { .. }));

        // 3. Trusted owner writing personal preference -> Permitted
        let res3 = MemGhostFilter::evaluate(&candidate_personal, CommandPrincipal::LocalCli, false);
        assert_eq!(res3, MemGhostFilterResult::Permitted);
    }

    #[test]
    fn test_deterministic_reconciliation_workflow() {
        let conn = setup_test_db();
        let now_ms = 1_000_000;

        let candidate_a = CandidateFact {
            subject: "user".into(),
            predicate: "favorite_drink".into(),
            scope: "personal".into(),
            value: "cà phê đen".into(),
            origin: FactOrigin::Explicit,
            confidence: 0.9,
            source_evidence_hash: "ev1".into(),
            source_role: "user".into(),
        };

        // 1. ADD
        let out1 = DeterministicReconciliationEngine::reconcile(&conn, &candidate_a, "ev1", now_ms)
            .unwrap();
        assert_eq!(out1.action, ReconciliationAction::Add);

        // 2. NOOP (same value, lower or equal confidence)
        let out2 =
            DeterministicReconciliationEngine::reconcile(&conn, &candidate_a, "ev1", now_ms + 100)
                .unwrap();
        assert_eq!(out2.action, ReconciliationAction::Noop);

        // 3. EXTEND (same value, higher confidence)
        let mut candidate_boosted = candidate_a.clone();
        candidate_boosted.confidence = 1.0;
        let out3 = DeterministicReconciliationEngine::reconcile(
            &conn,
            &candidate_boosted,
            "ev2",
            now_ms + 200,
        )
        .unwrap();
        assert_eq!(out3.action, ReconciliationAction::Extend);

        // 4. INVALIDATE (user changed mind to "trà xanh")
        let candidate_b = CandidateFact {
            subject: "user".into(),
            predicate: "favorite_drink".into(),
            scope: "personal".into(),
            value: "trà xanh".into(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            source_evidence_hash: "ev3".into(),
            source_role: "user".into(),
        };
        let out4 =
            DeterministicReconciliationEngine::reconcile(&conn, &candidate_b, "ev3", now_ms + 300)
                .unwrap();
        assert_eq!(out4.action, ReconciliationAction::Invalidate);
        assert_eq!(out4.superseded_fact_id, Some(out1.fact_id));

        // Active singleton invariant verified: exactly 1 active fact
        let active = get_active_fact(&conn, "user", "favorite_drink", "personal")
            .unwrap()
            .unwrap();
        assert_eq!(active.object_value, "trà xanh");
    }

    #[test]
    fn test_re_digits_static_regex_compilation() {
        let sample = "Chuyển khoản 1500000 vnđ qua STK 123456789 và mã giao dịch 99";
        let matches: Vec<&str> = RE_DIGITS.find_iter(sample).map(|m| m.as_str()).collect();
        assert_eq!(matches, vec!["1500000", "123456789", "99"]);
    }

    #[test]
    fn test_memghost_is_allowed_blocks_telegram_and_zalo() {
        let filter = MemGhostFilter::new();
        let candidate_pref = ExtractedFactCandidate {
            subject: "user".to_string(),
            predicate: "preference_general".to_string(),
            object_value: "cà phê trứng".to_string(),
            scope: "personal".to_string(),
            confidence: 0.9,
            rule_name: "test_pref".to_string(),
        };

        // Untrusted channels MUST NOT be allowed to mutate protected personal preferences
        assert!(!filter.is_allowed(&candidate_pref, SourceChannel::Telegram));
        assert!(!filter.is_allowed(&candidate_pref, SourceChannel::Zalo));
        assert!(!filter.is_allowed(&candidate_pref, SourceChannel::ExternalWeb));
        assert!(!filter.is_allowed(&candidate_pref, SourceChannel::WebRss));

        // Trusted owner channels MUST be permitted
        assert!(filter.is_allowed(&candidate_pref, SourceChannel::LocalCli));
        assert!(filter.is_allowed(&candidate_pref, SourceChannel::Cli));
        assert!(filter.is_allowed(&candidate_pref, SourceChannel::Tauri));
    }

    #[test]
    fn test_fact_promotion_eligibility() {
        let conn = setup_test_db();
        let now_ms = 1_000_000;

        // Fact 1: explicit -> eligible immediately
        let f_exp = crate::db::bitemporal::BitemporalFact {
            fact_id: "f_exp".into(),
            subject: "user".into(),
            predicate: "lang".into(),
            value: "vi".into(),
            object_value: "vi".into(),
            scope: "personal".into(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            valid_from: now_ms,
            valid_to: None,
            recorded_at: now_ms,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: None,
            key: None,
            importance: 8,
            sensitivity: FactSensitivity::Normal,
            created_at: now_ms,
            updated_at: now_ms,
        };
        let id_exp = insert_active_fact(&conn, &f_exp, Some("ev_1")).unwrap();
        assert!(FactPromotionEvaluator::is_eligible_for_core_profile(&conn, &id_exp).unwrap());

        // Fact 2: inferred with 1 evidence (session 1) -> not eligible yet (< 3 sessions)
        let f_inf = crate::db::bitemporal::BitemporalFact {
            fact_id: "f_inf".into(),
            subject: "user".into(),
            predicate: "music".into(),
            value: "jazz".into(),
            object_value: "jazz".into(),
            scope: "personal".into(),
            origin: FactOrigin::Inferred,
            confidence: 0.7,
            valid_from: now_ms,
            valid_to: None,
            recorded_at: now_ms,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: None,
            key: None,
            importance: 5,
            sensitivity: FactSensitivity::Normal,
            created_at: now_ms,
            updated_at: now_ms,
        };
        let id_inf = insert_active_fact(&conn, &f_inf, Some("ev_1")).unwrap();
        assert!(
            !FactPromotionEvaluator::is_eligible_for_core_profile(&conn, &id_inf).unwrap(),
            "1 session must NOT promote"
        );

        // Link second evidence in the SAME session ('ev_same_sess' -> 'sess_1') -> distinct session count is still 1 (< 3)
        conn.execute(
            "INSERT INTO fact_evidence (fact_id, evidence_id, confidence_weight, extracted_at) VALUES (?1, 'ev_same_sess', 1.0, ?2)",
            rusqlite::params![&id_inf, now_ms + 500],
        ).unwrap();
        assert!(
            !FactPromotionEvaluator::is_eligible_for_core_profile(&conn, &id_inf).unwrap(),
            "2 evidences in SAME session must NOT promote (Challenge C2)"
        );

        // Link evidence from second session ('ev_2' -> 'sess_2') -> distinct session count is 2 (< 3)
        conn.execute(
            "INSERT INTO fact_evidence (fact_id, evidence_id, confidence_weight, extracted_at) VALUES (?1, 'ev_2', 1.0, ?2)",
            rusqlite::params![&id_inf, now_ms + 1000],
        ).unwrap();
        assert!(
            !FactPromotionEvaluator::is_eligible_for_core_profile(&conn, &id_inf).unwrap(),
            "2 distinct sessions must NOT promote (< 3)"
        );

        // Link evidence from third session ('ev_3' -> 'sess_3') -> distinct session count is 3 (>= 3) -> PROMOTED!
        conn.execute(
            "INSERT INTO fact_evidence (fact_id, evidence_id, confidence_weight, extracted_at) VALUES (?1, 'ev_3', 1.0, ?2)",
            rusqlite::params![&id_inf, now_ms + 2000],
        ).unwrap();
        assert!(
            FactPromotionEvaluator::is_eligible_for_core_profile(&conn, &id_inf).unwrap(),
            "3 distinct sessions MUST promote"
        );
    }
}
