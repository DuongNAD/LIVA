//! Vietnamese 3-Form FTS5 Normalizer, Search & Hybrid RRF Decay Engine (Feature F17).
//!
//! Provides:
//! - Three-form Vietnamese text representation:
//!   1. Form 1 (NFC accented): Standard Unicode NFC with full diacritics preserved.
//!   2. Form 2 (Folded): High-performance O(1) manual diacritic folding for all 89
//!      Vietnamese vowel variants (`ộ` -> `o`, `ệ` -> `e`, `ự` -> `u`, `ẵ` -> `a`, etc.)
//!      and `đ`/`Đ` -> `d`. Solves SQLite FTS5 `remove_diacritics 1` bugs.
//!   3. Form 3 (Bigram): Syllables joined by underscore `_` (e.g. `ho_chieu`, `thu_do`)
//!      preserving compound-word semantics.
//! - FTS5 indexing on `facts_fts` with `tokenize="unicode61 remove_diacritics 0"`.
//! - Hybrid Search with Reciprocal Rank Fusion (K=60, w_dense=0.6, w_sparse=0.4).
//! - Ebbinghaus temporal decay based on fact age, access count, and provenance.
//! - Automatic PrefEval Preference Reminder generator for prompt suffix injection.

use crate::db::bitemporal::BitemporalFact;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

/// Three normalized forms of Vietnamese text for full-text indexing and query expansion (F17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VietnameseThreeForms {
    /// Form 1: Unicode NFC with full accents preserved
    pub form_nfc: String,
    /// Form 2: Unaccented text with all 89 complex vowel variants and đ/Đ folded to ASCII
    pub form_folded: String,
    /// Form 3: Space-separated bigrams with syllables connected by `_`
    pub form_bigram: String,
}

/// Normalizer implementing manual O(1) character folding and syllable bigram generation.
#[derive(Debug, Clone, Default)]
pub struct VietnameseThreeFormNormalizer;

impl VietnameseThreeFormNormalizer {
    pub fn new() -> Self {
        Self
    }

    pub fn normalize_three_forms(&self, text: &str) -> VietnameseThreeForms {
        Self::normalize(text)
    }

    /// Normalize raw text into the three canonical Vietnamese forms.
    pub fn normalize(text: &str) -> VietnameseThreeForms {
        // 1. Form 1: Canonical Unicode NFC, lowercased, cleaned punctuation
        let nfc_text: String = text.nfc().collect();
        let cleaned_nfc = clean_punctuation_and_spaces(&nfc_text.to_lowercase());

        // 2. Form 2: Manual O(1) folding of all Vietnamese diacritics
        let mut folded = String::with_capacity(cleaned_nfc.len());
        for ch in cleaned_nfc.chars() {
            let folded_ch = fold_vietnamese_char(ch);
            // Drop any combining diacritical marks if present
            if !('\u{0300}'..='\u{036F}').contains(&folded_ch) {
                folded.push(folded_ch);
            }
        }
        let cleaned_folded = clean_punctuation_and_spaces(&folded);

        // 3. Form 3: Bigrams connected by `_`
        let tokens: Vec<&str> = cleaned_folded.split_whitespace().collect();
        let form_bigram = if tokens.is_empty() {
            String::new()
        } else if tokens.len() == 1 {
            tokens[0].to_string()
        } else {
            let mut bigrams = Vec::with_capacity(tokens.len() - 1);
            for i in 0..tokens.len() - 1 {
                bigrams.push(format!("{}_{}", tokens[i], tokens[i + 1]));
            }
            bigrams.join(" ")
        };

        VietnameseThreeForms {
            form_nfc: cleaned_nfc,
            form_folded: cleaned_folded,
            form_bigram,
        }
    }

    /// Expand a search query into an FTS5 MATCH clause targeting the 3 forms.
    pub fn build_fts5_query(query: &str) -> String {
        let forms = Self::normalize(query);
        let mut clauses = Vec::new();

        // 1. Bigram match (highest priority for compound phrases)
        if !forms.form_bigram.is_empty() {
            let bigram_tokens: Vec<&str> = forms.form_bigram.split_whitespace().collect();
            for bt in bigram_tokens {
                let sanitized = bt.replace('"', "\"\"");
                clauses.push(format!("form_bigram: \"{sanitized}\""));
            }
        }

        // 2. Exact NFC accented phrase match
        if !forms.form_nfc.is_empty() {
            let sanitized = forms.form_nfc.replace('"', "\"\"");
            clauses.push(format!("form_nfc: \"{sanitized}\""));
        }

        // 3. Folded unaccented phrase match
        if !forms.form_folded.is_empty() {
            let sanitized = forms.form_folded.replace('"', "\"\"");
            clauses.push(format!("form_folded: \"{sanitized}\""));
        }

        if clauses.is_empty() {
            String::new()
        } else {
            clauses.join(" OR ")
        }
    }
}

/// O(1) manual folding table for 89 Vietnamese vowel variants + đ/Đ.
#[inline]
pub fn fold_vietnamese_char(c: char) -> char {
    match c {
        // Vowel A (a, à, á, ả, ã, ạ, ă, ằ, ắ, ẳ, ẵ, ặ, â, ầ, ấ, ẩ, ẫ, ậ)
        'a' | 'á' | 'à' | 'ả' | 'ã' | 'ạ'
        | 'ă' | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ'
        | 'â' | 'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ'
        | 'A' | 'Á' | 'À' | 'Ả' | 'Ã' | 'Ạ'
        | 'Ă' | 'Ắ' | 'Ằ' | 'Ẳ' | 'Ẵ' | 'Ặ'
        | 'Â' | 'Ấ' | 'Ầ' | 'Ẩ' | 'Ẫ' | 'Ậ' => 'a',

        // Vowel E (e, è, é, ẻ, ẽ, ẹ, ê, ề, ế, ể, ễ, ệ)
        'e' | 'é' | 'è' | 'ẻ' | 'ẽ' | 'ẹ'
        | 'ê' | 'ế' | 'ề' | 'ể' | 'ễ' | 'ệ'
        | 'E' | 'É' | 'È' | 'Ẻ' | 'Ẽ' | 'Ẹ'
        | 'Ê' | 'Ế' | 'Ề' | 'Ể' | 'Ễ' | 'Ệ' => 'e',

        // Vowel I (i, ì, í, ỉ, ĩ, ị)
        'i' | 'í' | 'ì' | 'ỉ' | 'ĩ' | 'ị'
        | 'I' | 'Í' | 'Ì' | 'Ỉ' | 'Ĩ' | 'Ị' => 'i',

        // Vowel O (o, ò, ó, ỏ, õ, ọ, ô, ồ, ố, ổ, ỗ, ộ, ơ, ờ, ớ, ở, ỡ, ợ)
        'o' | 'ó' | 'ò' | 'ỏ' | 'õ' | 'ọ'
        | 'ô' | 'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ'
        | 'ơ' | 'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ'
        | 'O' | 'Ó' | 'Ò' | 'Ỏ' | 'Õ' | 'Ọ'
        | 'Ô' | 'Ố' | 'Ồ' | 'Ổ' | 'Ỗ' | 'Ộ'
        | 'Ơ' | 'Ớ' | 'Ờ' | 'Ở' | 'Ỡ' | 'Ợ' => 'o',

        // Vowel U (u, ù, ú, ủ, ũ, ụ, ư, ừ, ứ, ử, ữ, ự)
        'u' | 'ú' | 'ù' | 'ủ' | 'ũ' | 'ụ'
        | 'ư' | 'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự'
        | 'U' | 'Ú' | 'Ù' | 'Ủ' | 'Ũ' | 'Ụ'
        | 'Ư' | 'Ứ' | 'Ừ' | 'Ử' | 'Ữ' | 'Ự' => 'u',

        // Vowel Y (y, ỳ, ý, ỷ, ỹ, ỵ)
        'y' | 'ý' | 'ỳ' | 'ỷ' | 'ỹ' | 'ỵ'
        | 'Y' | 'Ý' | 'Ỳ' | 'Ỷ' | 'Ỹ' | 'Ỵ' => 'y',

        // Consonant Đ
        'đ' | 'Đ' => 'd',

        other => other.to_ascii_lowercase(),
    }
}

/// Clean punctuation and collapse excess whitespace.
fn clean_punctuation_and_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_space = true;

    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            out.push(ch);
            last_was_space = false;
        } else if ch.is_whitespace() || is_punctuation_or_symbol(ch) {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        }
    }

    out.trim().to_string()
}

fn is_punctuation_or_symbol(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | ';' | ':' | '!' | '?' | '-' | '(' | ')' | '[' | ']' | '{' | '}'
            | '"' | '\'' | '`' | '/' | '\\' | '|' | '@' | '#' | '$' | '%' | '^'
            | '&' | '*' | '+' | '=' | '<' | '>' | '~'
    )
}

/// Candidate fact representation for hybrid scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateFact {
    pub fact_id: String,
    pub subject: String,
    pub predicate: String,
    pub object_value: String,
    pub dense_rank: Option<usize>,
    pub sparse_rank: Option<usize>,
    pub last_accessed_at: i64,
    pub access_count: i64,
}

/// FTS fact row structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FtsFactRow {
    pub fact_id: String,
    pub subject: String,
    pub predicate: String,
    pub form_nfc: String,
    pub form_folded: String,
    pub form_bigram: String,
}

/// Scored candidate result from HybridScorer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredCandidateFact {
    pub fact_id: String,
    pub final_score: f64,
}

/// Upsert fact directly into FTS5 table with full text.
pub fn upsert_fact_fts(
    conn: &Connection,
    fact_id: &str,
    subject: &str,
    predicate: &str,
    content: &str,
) -> Result<(), rusqlite::Error> {
    let forms = VietnameseThreeFormNormalizer::normalize(content);

    // Delete existing entry if present
    conn.execute(
        "DELETE FROM facts_fts WHERE fact_id = ?1",
        [fact_id],
    )?;

    conn.execute(
        "INSERT INTO facts_fts (fact_id, subject, predicate, form_nfc, form_folded, form_bigram)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            fact_id,
            subject,
            predicate,
            &forms.form_nfc,
            &forms.form_folded,
            &forms.form_bigram,
        ],
    )?;

    Ok(())
}

/// Index or re-index a BitemporalFact in `facts_fts` (F17).
pub fn index_fact_fts(
    conn: &Connection,
    fact: &BitemporalFact,
    _normalizer: &VietnameseThreeFormNormalizer,
) -> Result<(), rusqlite::Error> {
    let content = if !fact.object_value.is_empty() {
        &fact.object_value
    } else {
        &fact.value
    };
    upsert_fact_fts(conn, &fact.fact_id, &fact.subject, &fact.predicate, content)
}

/// Result of an FTS5 search on `facts_fts`.
#[derive(Debug, Clone, PartialEq)]
pub struct FtsSearchResult {
    pub fact_id: String,
    pub subject: String,
    pub predicate: String,
    pub rank: f64,
}

/// Search `facts_fts` using 3-form expanded MATCH query (F17).
pub fn search_facts_fts(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<FtsSearchResult>, rusqlite::Error> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let fts_query = VietnameseThreeFormNormalizer::build_fts5_query(query);
    if fts_query.trim().is_empty() || fts_query == "*" {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(
        "SELECT fact_id, subject, predicate, rank
         FROM facts_fts
         WHERE facts_fts MATCH ?1
         ORDER BY rank ASC
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(params![&fts_query, limit as i64], |r| {
        Ok(FtsSearchResult {
            fact_id: r.get(0)?,
            subject: r.get(1)?,
            predicate: r.get(2)?,
            rank: r.get(3)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

/// Compute Reciprocal Rank Fusion with temporal decay and confidence weighting.
///
/// Constants:
/// - K = 60.0
/// - w_dense = 0.6
/// - w_sparse = 0.4
#[derive(Debug, Clone)]
pub struct HybridScorer {
    pub k: f64,
    pub w_dense: f64,
    pub w_sparse: f64,
}

impl Default for HybridScorer {
    fn default() -> Self {
        Self {
            k: 60.0,
            w_dense: 0.6,
            w_sparse: 0.4,
        }
    }
}

impl HybridScorer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Score and rank candidates using RRF and temporal decay.
    pub fn score_and_rank(
        &self,
        candidates: Vec<CandidateFact>,
        now_ms: i64,
    ) -> Vec<ScoredCandidateFact> {
        let mut scored = Vec::new();
        for c in candidates {
            let mut rrf = 0.0;
            if let Some(dr) = c.dense_rank {
                rrf += self.w_dense / (self.k + dr as f64);
            }
            if let Some(sr) = c.sparse_rank {
                rrf += self.w_sparse / (self.k + sr as f64);
            }
            let decay = if c.subject == "user"
                && (c.predicate.contains("preference") || c.predicate == "vacation_spot")
            {
                1.0
            } else {
                let age_days = ((now_ms - c.last_accessed_at).max(0) as f64) / 86_400_000.0;
                (-age_days / 30.0).exp().clamp(0.1, 1.0)
            };
            let final_score = rrf * decay;
            scored.push(ScoredCandidateFact {
                fact_id: c.fact_id,
                final_score,
            });
        }
        scored.sort_by(|a, b| {
            b.final_score
                .partial_cmp(&a.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored
    }

    /// Merge dense vector ranks and sparse FTS ranks using RRF + Ebbinghaus temporal decay.
    pub fn merge(
        &self,
        dense_results: &[(String, usize)], // (fact_id, 1-based rank)
        sparse_results: &[(String, usize)], // (fact_id, 1-based rank)
        facts_map: &HashMap<String, BitemporalFact>,
        now_ms: i64,
    ) -> Vec<(String, f64)> {
        let mut rrf_scores: HashMap<String, f64> = HashMap::new();

        // 1. Accumulate dense RRF component
        for (fact_id, rank) in dense_results {
            let score = self.w_dense / (self.k + *rank as f64);
            *rrf_scores.entry(fact_id.clone()).or_insert(0.0) += score;
        }

        // 2. Accumulate sparse RRF component
        for (fact_id, rank) in sparse_results {
            let score = self.w_sparse / (self.k + *rank as f64);
            *rrf_scores.entry(fact_id.clone()).or_insert(0.0) += score;
        }

        // 3. Apply temporal decay and confidence/importance multipliers
        let mut final_scored: Vec<(String, f64)> = rrf_scores
            .into_iter()
            .map(|(fact_id, base_rrf)| {
                let multiplier = if let Some(fact) = facts_map.get(&fact_id) {
                    let decay = calculate_temporal_decay(fact, now_ms);
                    let importance_weight = (fact.importance as f64) / 10.0;
                    decay * fact.confidence * importance_weight.clamp(0.1, 1.0)
                } else {
                    1.0
                };
                (fact_id, base_rrf * multiplier)
            })
            .collect();

        // Sort descending by final score
        final_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        final_scored
    }
}

/// Calculate Ebbinghaus temporal retention decay with explicit access count reinforcement.
pub fn calculate_temporal_decay_with_access(
    fact: &BitemporalFact,
    access_count: u32,
    now_ms: i64,
) -> f64 {
    use crate::db::bitemporal::FactOrigin;

    // Explicit and user-edited facts NEVER decay
    if fact.origin == FactOrigin::Explicit || fact.origin == FactOrigin::UserEdited {
        return 1.0;
    }

    let age_ms = (now_ms - fact.valid_from).max(0);
    let age_days = (age_ms as f64) / (86_400.0 * 1000.0);

    let tau_days = if fact.scope == "personal" || fact.predicate.contains("preference") {
        180.0
    } else {
        30.0
    };

    let reinforced_tau = tau_days * (1.0 + 0.2 * (access_count.min(5) as f64));
    let decay = (-age_days / reinforced_tau).exp();
    decay.clamp(0.10, 1.0)
}

/// Calculate Ebbinghaus temporal retention decay with default access count = 0.
#[inline]
pub fn calculate_temporal_decay(fact: &BitemporalFact, now_ms: i64) -> f64 {
    calculate_temporal_decay_with_access(fact, 0, now_ms)
}

/// Generate a PrefEval preference reminder suffix for prompt assembly (F17).
///
/// Research shows adding this reminder right before the latest user turn raises
/// preference compliance from <10% to >90% on long conversations.
pub fn format_preference_reminder(active_facts: &[BitemporalFact]) -> Option<String> {
    let preference_facts: Vec<&BitemporalFact> = active_facts
        .iter()
        .filter(|f| {
            f.is_active()
                && (f.scope == "personal"
                    || f.scope == "travel"
                    || f.predicate.starts_with("preference_")
                    || f.predicate.contains("thich")
                    || f.predicate.contains("tone")
                    || f.predicate.contains("language")
                    || f.predicate == "vacation_spot")
        })
        .collect();

    if preference_facts.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    lines.push("## USER PREFERENCES & CONSTRAINTS".to_string());
    lines.push("[Nhắc nhở sở thích người dùng liên quan]:".to_string());
    for f in preference_facts.iter().take(10) {
        let val = if !f.object_value.is_empty() {
            &f.object_value
        } else {
            &f.value
        };
        lines.push(format!("- {}: {}", f.predicate, val));
    }

    Some(lines.join("\n"))
}

/// Initializes the `facts_fts` virtual table schema if it does not exist.
pub fn init_facts_fts_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        r#"CREATE VIRTUAL TABLE IF NOT EXISTS facts_fts USING fts5(
            fact_id UNINDEXED,
            subject,
            predicate,
            form_nfc,
            form_folded,
            form_bigram,
            tokenize="unicode61 remove_diacritics 0"
        );"#,
    )?;
    Ok(())
}

/// Metadata used for hybrid scoring and Ebbinghaus decay calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactScoringMeta {
    pub fact_id: String,
    pub subject: String,
    pub predicate: String,
    pub object_value: String,
    pub origin: String,
    pub scope: String,
    pub confidence: f64,
    pub importance: f64,
    pub recorded_at: i64,
    pub access_count: u32,
    pub is_pinned: bool,
}

/// Candidate returned from single-channel retrieval with raw rank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalCandidate {
    pub fact_id: String,
    pub raw_rank: usize,
}

/// Fused candidate with final score and decay factor.
#[derive(Debug, Clone, PartialEq)]
pub struct FusedCandidate {
    pub fact_id: String,
    pub final_score: f64,
    pub decay_factor: f64,
}

/// Calculates Ebbinghaus temporal retention decay for fact scoring metadata with access count reinforcement.
pub fn compute_ebbinghaus_decay(meta: &FactScoringMeta, now_ms: i64) -> f64 {
    if meta.origin == "user_edited" || meta.origin == "explicit" || meta.is_pinned {
        return 1.0;
    }
    let age_ms = (now_ms - meta.recorded_at).max(0);
    let age_days = (age_ms as f64) / (86_400.0 * 1000.0);
    let tau_days = if meta.scope == "personal" || meta.predicate.contains("preference") {
        180.0
    } else {
        30.0
    };
    let reinforced_tau = tau_days * (1.0 + 0.2 * (meta.access_count.min(5) as f64));
    let decay = (-age_days / reinforced_tau).exp();
    decay.clamp(0.10, 1.0)
}

/// Reciprocal Rank Fusion (RRF) combiner with dynamic Ebbinghaus decay.
pub fn fuse_hybrid_candidates(
    dense_ranks: &[RetrievalCandidate],
    sparse_ranks: &[RetrievalCandidate],
    meta_map: &HashMap<String, FactScoringMeta>,
    w_dense: f64,
    w_sparse: f64,
    _limit: usize,
    now_ms: i64,
) -> Vec<FusedCandidate> {
    let k = 60.0;
    let mut scores: HashMap<String, f64> = HashMap::new();

    for c in dense_ranks {
        *scores.entry(c.fact_id.clone()).or_insert(0.0) += w_dense / (k + c.raw_rank as f64);
    }
    for c in sparse_ranks {
        *scores.entry(c.fact_id.clone()).or_insert(0.0) += w_sparse / (k + c.raw_rank as f64);
    }

    let mut res: Vec<FusedCandidate> = scores
        .into_iter()
        .map(|(fid, base_rrf)| {
            let (decay, conf, imp) = if let Some(m) = meta_map.get(&fid) {
                (compute_ebbinghaus_decay(m, now_ms), m.confidence, m.importance / 10.0)
            } else {
                (1.0, 1.0, 1.0)
            };
            FusedCandidate {
                fact_id: fid,
                final_score: base_rrf * decay * conf * imp.clamp(0.1, 1.0),
                decay_factor: decay,
            }
        })
        .collect();

    res.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap_or(std::cmp::Ordering::Equal));
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::bitemporal::{BitemporalFact, FactOrigin, FactSensitivity};

    #[test]
    fn test_nfc_and_vowel_folding() {
        let text = "Đà Nẵng và Đắk Lắk có hộ chiếu";
        let forms = VietnameseThreeFormNormalizer::normalize(text);

        assert_eq!(forms.form_nfc, "đà nẵng và đắk lắk có hộ chiếu");
        assert_eq!(forms.form_folded, "da nang va dak lak co ho chieu");
        assert!(forms.form_bigram.contains("da_nang"));
        assert!(forms.form_bigram.contains("ho_chieu"));
    }

    #[test]
    fn test_all_89_vowels_folded() {
        let text = "a à á ả ã ạ ă ằ ắ ẳ ẵ ặ â ầ ấ ẩ ẫ ậ e è é ẻ ẽ ẹ ê ề ế ể ễ ệ i ì í ỉ ĩ ị o ò ó ỏ õ ọ ô ồ ố ổ ỗ ộ ơ ờ ớ ở ỡ ợ u ù ú ủ ũ ụ ư ừ ứ ử ữ ự y ỳ ý ỷ ỹ ỵ đ";
        let forms = VietnameseThreeFormNormalizer::normalize(text);
        for ch in forms.form_folded.chars() {
            if ch.is_alphabetic() {
                assert!(ch.is_ascii_alphabetic(), "Character '{ch}' must be ASCII folded");
            }
        }
    }

    #[test]
    fn test_sqlite_fts5_vietnamese_search() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE VIRTUAL TABLE facts_fts USING fts5(
                fact_id UNINDEXED,
                subject,
                predicate,
                form_nfc,
                form_folded,
                form_bigram,
                tokenize=\"unicode61 remove_diacritics 0\"
             );",
        )
        .unwrap();

        upsert_fact_fts(&conn, "f1", "user", "passport", "Chủ sở hữu có hộ chiếu Việt Nam").unwrap();
        upsert_fact_fts(&conn, "f2", "user", "residence", "Sống tại thủ đô Hà Nội").unwrap();

        // 1. Search unaccented "ho chieu"
        let res1 = search_facts_fts(&conn, "ho chieu", 10).unwrap();
        assert!(!res1.is_empty(), "Unaccented 'ho chieu' must match");
        assert_eq!(res1[0].fact_id, "f1");

        // 2. Search unaccented with 'd' for 'đ': "ha noi"
        let res2 = search_facts_fts(&conn, "ha noi", 10).unwrap();
        assert!(!res2.is_empty(), "Unaccented 'ha noi' must match 'Hà Nội'");
        assert_eq!(res2[0].fact_id, "f2");

        // 3. Search bigram "thu do"
        let res3 = search_facts_fts(&conn, "thu do", 10).unwrap();
        assert!(!res3.is_empty(), "Bigram 'thu do' must match 'thủ đô'");
        assert_eq!(res3[0].fact_id, "f2");
    }

    #[test]
    fn test_temporal_decay_ebbinghaus() {
        let now_ms = 1_000_000_000_000;
        let fact_explicit = BitemporalFact {
            fact_id: "f_exp".into(),
            subject: "user".into(),
            predicate: "name".into(),
            value: "Hải".into(),
            object_value: "Hải".into(),
            scope: "personal".into(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            valid_from: now_ms - (100 * 86_400 * 1000), // 100 days old
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

        let decay_exp = calculate_temporal_decay(&fact_explicit, now_ms);
        assert_eq!(decay_exp, 1.0, "Explicit facts must never decay");

        let mut fact_episodic = fact_explicit.clone();
        fact_episodic.origin = FactOrigin::Inferred;
        fact_episodic.scope = "episodic".into();

        let decay_epi = calculate_temporal_decay(&fact_episodic, now_ms);
        assert!(decay_epi < 0.5, "100-day-old episodic fact must decay significantly");
        assert!(decay_epi >= 0.10, "Decay must be clamped at 0.10");
    }

    #[test]
    fn test_format_preference_reminder() {
        let now_ms = 1_000_000_000_000;
        let facts = vec![
            BitemporalFact {
                fact_id: "f1".into(),
                subject: "user".into(),
                predicate: "preference_drink".into(),
                value: "cà phê sữa đá ít đường".into(),
                object_value: "cà phê sữa đá ít đường".into(),
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
                importance: 9,
                sensitivity: FactSensitivity::Normal,
                created_at: now_ms,
                updated_at: now_ms,
            },
        ];

        let reminder = format_preference_reminder(&facts).expect("reminder");
        assert!(reminder.contains("[Nhắc nhở sở thích người dùng liên quan]:"));
        assert!(reminder.contains("cà phê sữa đá ít đường"));
    }

    #[test]
    fn test_ebbinghaus_decay_access_count_reinforcement() {
        let now_ms = 1_000_000_000_000i64;
        let day_ms = 86_400_000i64;

        // Fact: Inferred personal preference, age = 180 days (1 base half-life)
        let mut meta = FactScoringMeta {
            fact_id: "fact_pref".into(),
            subject: "user".into(),
            predicate: "preference_coffee".into(),
            object_value: "cold_brew".into(),
            origin: "inferred".into(),
            scope: "personal".into(),
            confidence: 1.0,
            importance: 5.0,
            recorded_at: now_ms - (180 * day_ms),
            access_count: 0,
            is_pinned: false,
        };

        // 1. With access_count = 0 -> tau_eff = 180 * 1.0 = 180d -> decay = exp(-1.0) ~ 0.3679
        let decay_0 = compute_ebbinghaus_decay(&meta, now_ms);
        let expected_0 = (-1.0f64).exp();
        assert!((decay_0 - expected_0).abs() < 1e-4, "access_count=0 must yield base decay");

        // 2. With access_count = 5 -> tau_eff = 180 * (1 + 0.2*5) = 360d -> decay = exp(-180/360) = exp(-0.5) ~ 0.6065
        meta.access_count = 5;
        let decay_5 = compute_ebbinghaus_decay(&meta, now_ms);
        let expected_5 = (-0.5f64).exp();
        assert!((decay_5 - expected_5).abs() < 1e-4, "access_count=5 must double half-life");
        assert!(decay_5 > decay_0, "Reinforcement must significantly improve retention");

        // 3. With access_count = 10 -> capped at min(access_count, 5) = 5 -> identical to access_count = 5
        meta.access_count = 10;
        let decay_10 = compute_ebbinghaus_decay(&meta, now_ms);
        assert_eq!(decay_10, decay_5, "access_count above 5 must cap at 5");
    }

    #[test]
    fn test_fts5_empty_whitespace_and_special_char_queries() {
        let conn = Connection::open_in_memory().unwrap();
        init_facts_fts_schema(&conn).unwrap();

        upsert_fact_fts(
            &conn,
            "f1",
            "user",
            "passport",
            "Chủ sở hữu có hộ chiếu Việt Nam",
        )
        .unwrap();

        // 1. Empty string query
        let q_empty = VietnameseThreeFormNormalizer::build_fts5_query("");
        assert!(q_empty.is_empty(), "Empty query should produce empty string");
        let res_empty = search_facts_fts(&conn, "", 10).unwrap();
        assert!(res_empty.is_empty());

        // 2. Whitespace-only query
        let q_ws = VietnameseThreeFormNormalizer::build_fts5_query("   \t\n  ");
        assert!(q_ws.is_empty(), "Whitespace query should produce empty string");
        let res_ws = search_facts_fts(&conn, "   \t\n  ", 10).unwrap();
        assert!(res_ws.is_empty());

        // 3. Punctuation-only query
        let q_punct = VietnameseThreeFormNormalizer::build_fts5_query("??? !@#$%^&*() - + = < >");
        assert!(q_punct.is_empty(), "Punctuation query should produce empty string");
        let res_punct = search_facts_fts(&conn, "??? !@#$%^&*() - + = < >", 10).unwrap();
        assert!(res_punct.is_empty());

        // 4. Standalone wildcard query
        let res_star = search_facts_fts(&conn, "*", 10).unwrap();
        assert!(res_star.is_empty(), "Standalone '*' query should not crash SQLite FTS5");
    }
}
