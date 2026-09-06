use std::collections::HashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use rusqlite::{Connection, ToSql};
use rusqlite::types::Value;
use crate::db::MetadataFilter;

/// Common Search Hit representation returned across BM25, Dense Vector, and HippoRAG channels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    pub id: i64,
    pub vec_id: String,
    pub content: String,
    pub r#type: String,
    pub domain: String,
    pub category: String,
    pub score: f64,
    pub distance: f64,
    pub trace_keywords: Vec<String>,
    pub source_event_ids: Vec<String>,
    pub created_at: i64,
    pub source_channel: String,
}

impl SearchHit {
    pub fn new(
        id: i64,
        vec_id: impl Into<String>,
        content: impl Into<String>,
        r#type: impl Into<String>,
        domain: impl Into<String>,
        category: impl Into<String>,
        score: f64,
        source_channel: impl Into<String>,
    ) -> Self {
        Self {
            id,
            vec_id: vec_id.into(),
            content: content.into(),
            r#type: r#type.into(),
            domain: domain.into(),
            category: category.into(),
            score,
            distance: 0.0,
            trace_keywords: Vec::new(),
            source_event_ids: Vec::new(),
            created_at: chrono::Utc::now().timestamp_millis(),
            source_channel: source_channel.into(),
        }
    }
}

/// Strip Vietnamese diacritics and normalize text to base ASCII.
/// Handles both NFC precomposed characters and NFD decomposed combining diacritics.
pub fn remove_diacritics(text: &str) -> String {
    // 1. Map precomposed NFC Vietnamese and special accented Latin characters to base ASCII
    let mut mapped = String::with_capacity(text.len());
    for c in text.chars() {
        let base = match c {
            // Lowercase A
            'à' | 'á' | 'ả' | 'ã' | 'ạ'
            | 'â' | 'ầ' | 'ấ' | 'ẩ' | 'ẫ' | 'ậ'
            | 'ă' | 'ằ' | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' => 'a',

            // Uppercase A
            'À' | 'Á' | 'Ả' | 'Ã' | 'Ạ'
            | 'Â' | 'Ầ' | 'Ấ' | 'Ẩ' | 'Ẫ' | 'Ậ'
            | 'Ă' | 'Ằ' | 'Ắ' | 'Ẳ' | 'Ẵ' | 'Ặ' => 'A',

            // Lowercase E
            'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ'
            | 'ê' | 'ề' | 'ế' | 'ể' | 'ễ' | 'ệ' => 'e',

            // Uppercase E
            'È' | 'É' | 'Ẻ' | 'Ẽ' | 'Ẹ'
            | 'Ê' | 'Ề' | 'Ế' | 'Ể' | 'Ễ' | 'Ệ' => 'E',

            // Lowercase I
            'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' => 'i',

            // Uppercase I
            'Ì' | 'Í' | 'Ỉ' | 'Ĩ' | 'Ị' => 'I',

            // Lowercase O
            'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ'
            | 'ô' | 'ồ' | 'ố' | 'ổ' | 'ỗ' | 'ộ'
            | 'ơ' | 'ờ' | 'ớ' | 'ở' | 'ỡ' | 'ợ' => 'o',

            // Uppercase O
            'Ò' | 'Ó' | 'Ỏ' | 'Õ' | 'Ọ'
            | 'Ô' | 'Ồ' | 'Ố' | 'Ổ' | 'Ỗ' | 'Ộ'
            | 'Ơ' | 'Ờ' | 'Ớ' | 'Ở' | 'Ỡ' | 'Ợ' => 'O',

            // Lowercase U
            'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ'
            | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' => 'u',

            // Uppercase U
            'Ù' | 'Ú' | 'Ủ' | 'Ũ' | 'Ụ'
            | 'Ư' | 'Ừ' | 'Ứ' | 'Ử' | 'Ữ' | 'Ự' => 'U',

            // Lowercase Y
            'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',

            // Uppercase Y
            'Ỳ' | 'Ý' | 'Ỷ' | 'Ỹ' | 'Ỵ' => 'Y',

            // D with stroke
            'đ' => 'd',
            'Đ' => 'D',

            // Other common Latin accents
            'ç' => 'c', 'Ç' => 'C',
            'ñ' => 'n', 'Ñ' => 'N',
            'ß' => 's',

            other => other,
        };
        mapped.push(base);
    }

    // 2. Regex-strip combining diacritics (U+0300..U+036F, U+1DC0..U+1DFF, U+1AB0..U+1AFF, U+20D0..U+20FF)
    // to handle NFD decomposed strings perfectly
    let re = Regex::new(r"[\u{0300}-\u{036f}\u{1dc0}-\u{1dff}\u{1ab0}-\u{1aff}\u{20d0}-\u{20ff}]").unwrap();
    let stripped = re.replace_all(&mapped, "").to_string();

    stripped
}

/// Normalize Vietnamese search query: remove diacritics, lowercase, and trim excess whitespace.
pub fn normalize_vietnamese_query(query: &str) -> String {
    let clean = remove_diacritics(query);
    clean
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Tokenize text into normalized alphanumeric keywords.
pub fn tokenize_normalized(text: &str) -> Vec<String> {
    let normalized = remove_diacritics(text).to_lowercase();
    normalized
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// Prepare SQLite FTS5 matching query string with prefix wildcards and diacritic dual-matching.
pub fn prepare_fts5_query(query_text: &str) -> String {
    let words: Vec<&str> = query_text.split_whitespace().collect();
    if words.is_empty() {
        return "\"\"*".to_string();
    }

    let mut clauses = Vec::new();
    for word in words {
        let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if clean_word.is_empty() {
            continue;
        }

        let orig = clean_word.replace('"', "\"\"").to_lowercase();
        let stripped = remove_diacritics(&orig).to_lowercase();

        if orig == stripped {
            clauses.push(format!("\"{}\"*", orig));
        } else {
            clauses.push(format!("(\"{}\"* OR \"{}\"*)", orig, stripped));
        }
    }

    if clauses.is_empty() {
        "\"\"*".to_string()
    } else {
        clauses.join(" AND ")
    }
}

/// Configuration parameters for Okapi BM25 scoring.
#[derive(Debug, Clone)]
pub struct Bm25Config {
    /// Term frequency saturation parameter k1 in [1.2, 2.0] (default = 1.2)
    pub k1: f64,
    /// Document length normalization parameter b in [0.0, 1.0] (default = 0.75)
    pub b: f64,
}

impl Default for Bm25Config {
    fn default() -> Self {
        Self {
            k1: 1.2,
            b: 0.75,
        }
    }
}

/// Sparse BM25 Search Engine with Vietnamese diacritic-insensitive normalization.
#[derive(Debug, Clone, Default)]
pub struct SparseBm25Engine {
    pub config: Bm25Config,
}

impl SparseBm25Engine {
    /// Construct a new SparseBm25Engine with default Okapi BM25 parameters (k1=1.2, b=0.75).
    pub fn new() -> Self {
        Self {
            config: Bm25Config::default(),
        }
    }

    /// Construct a SparseBm25Engine with custom configuration.
    pub fn with_config(config: Bm25Config) -> Self {
        Self { config }
    }

    /// Calculate Okapi BM25 score for a document given query tokens and corpus statistics.
    pub fn score_document(
        &self,
        query_tokens: &[String],
        doc_tokens: &[String],
        doc_len: usize,
        avg_doc_len: f64,
        total_docs: usize,
        doc_freqs: &HashMap<String, usize>,
    ) -> f64 {
        if doc_len == 0 || total_docs == 0 || avg_doc_len <= 0.0 {
            return 0.0;
        }

        // Count term frequency within the document
        let mut tf_map: HashMap<&str, usize> = HashMap::new();
        for token in doc_tokens {
            *tf_map.entry(token.as_str()).or_insert(0) += 1;
        }

        let k1 = self.config.k1;
        let b = self.config.b;
        let len_norm = 1.0 - b + b * (doc_len as f64 / avg_doc_len);

        let mut total_score = 0.0f64;

        for q_term in query_tokens {
            let tf = *tf_map.get(q_term.as_str()).unwrap_or(&0) as f64;
            if tf <= 0.0 {
                continue;
            }

            let n_q = *doc_freqs.get(q_term).unwrap_or(&1) as f64;
            // Robertson-Spärck Jones IDF with smoothing
            let idf = ((total_docs as f64 - n_q + 0.5) / (n_q + 0.5) + 1.0).ln();
            let tf_component = (tf * (k1 + 1.0)) / (tf + k1 * len_norm);

            total_score += idf * tf_component;
        }

        total_score
    }

    /// Search an in-memory collection of documents using Vietnamese diacritic-insensitive BM25.
    pub fn search_in_memory(
        &self,
        query: &str,
        documents: &[(i64, String, String, String, String)], // (id, vec_id, content, domain, category)
        top_k: usize,
    ) -> Vec<SearchHit> {
        let query_tokens = tokenize_normalized(query);
        if query_tokens.is_empty() || documents.is_empty() {
            return Vec::new();
        }

        let total_docs = documents.len();
        let mut doc_token_lists: Vec<Vec<String>> = Vec::with_capacity(total_docs);
        let mut total_token_count = 0usize;
        let mut doc_freqs: HashMap<String, usize> = HashMap::new();

        for doc in documents {
            let tokens = tokenize_normalized(&doc.2);
            let mut unique_in_doc: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for token in &tokens {
                if unique_in_doc.insert(token.as_str()) {
                    *doc_freqs.entry(token.clone()).or_insert(0) += 1;
                }
            }
            total_token_count += tokens.len();
            doc_token_lists.push(tokens);
        }

        let avg_doc_len = (total_token_count as f64) / (total_docs as f64).max(1.0);
        let mut scored_hits = Vec::new();

        for (idx, doc) in documents.iter().enumerate() {
            let doc_tokens = &doc_token_lists[idx];
            let score = self.score_document(
                &query_tokens,
                doc_tokens,
                doc_tokens.len(),
                avg_doc_len,
                total_docs,
                &doc_freqs,
            );

            if score > 0.0 {
                scored_hits.push(SearchHit {
                    id: doc.0,
                    vec_id: doc.1.clone(),
                    content: doc.2.clone(),
                    r#type: "fact".to_string(),
                    domain: doc.3.clone(),
                    category: doc.4.clone(),
                    score,
                    distance: 0.0,
                    trace_keywords: doc_tokens.clone(),
                    source_event_ids: Vec::new(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                    source_channel: "sparse_bm25".to_string(),
                });
            }
        }

        // Sort descending by score, tie-break by ID ascending
        scored_hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });

        scored_hits.truncate(top_k);
        scored_hits
    }

    /// Execute SQLite FTS5 Okapi BM25 search against SQLite `vectors_fts` and `vectors_meta` tables.
    pub fn search_fts5(
        &self,
        conn: &Connection,
        query: &str,
        top_k: usize,
        filter: Option<&MetadataFilter>,
    ) -> Result<Vec<SearchHit>, rusqlite::Error> {
        let clean_query = prepare_fts5_query(query);

        let mut conditions = Vec::new();
        let mut params: Vec<Value> = Vec::new();

        if let Some(f) = filter {
            if let Some(t) = &f.r#type {
                conditions.push("m.type = ?");
                params.push(Value::Text(t.clone()));
            }
            if let Some(d) = &f.domain {
                conditions.push("m.domain = ?");
                params.push(Value::Text(d.clone()));
            }
            if let Some(c) = &f.category {
                conditions.push("m.category = ?");
                params.push(Value::Text(c.clone()));
            }
            if let Some(after) = f.created_after {
                conditions.push("m.created_at >= ?");
                params.push(Value::Integer(after));
            }
            if let Some(before) = f.created_before {
                conditions.push("m.created_at <= ?");
                params.push(Value::Integer(before));
            }
        }

        let meta_sql = if conditions.is_empty() {
            "1=1".to_string()
        } else {
            conditions.join(" AND ")
        };

        let sql = format!(
            "SELECT f.rowid, m.vec_id, m.content, m.type, m.domain, m.category, \
                    m.trace_keywords, m.source_event_ids, m.created_at, \
                    bm25(vectors_fts, 1.2, 0.75) AS bm25_rank \
             FROM vectors_fts f \
             INNER JOIN vectors_meta m ON m.id = f.rowid \
             WHERE f.content MATCH ? AND m.type != 'conversation_turn' AND {} \
             ORDER BY bm25_rank ASC \
             LIMIT ?",
            meta_sql
        );

        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => {
                let fallback_sql = format!(
                    "SELECT f.rowid, m.vec_id, m.content, m.type, m.domain, m.category, \
                            m.trace_keywords, m.source_event_ids, m.created_at \
                     FROM vectors_fts f \
                     INNER JOIN vectors_meta m ON m.id = f.rowid \
                     WHERE f.content MATCH ? AND m.type != 'conversation_turn' AND {} \
                     LIMIT ?",
                    meta_sql
                );
                return self.execute_fts_fallback(conn, &fallback_sql, &clean_query, &params, query, top_k, &meta_sql);
            }
        };

        let mut full_params = vec![Value::Text(clean_query.clone())];
        full_params.extend(params.clone());
        full_params.push(Value::Integer(top_k as i64));

        let param_refs: Vec<&dyn ToSql> = full_params.iter().map(|p| p as &dyn ToSql).collect();
        let rows_res = stmt.query(&param_refs[..]);

        let mut results = Vec::new();
        if let Ok(mut rows) = rows_res {
            while let Some(row) = rows.next()? {
                let rowid: i64 = row.get(0)?;
                let vec_id: String = row.get(1)?;
                let content: String = row.get(2)?;
                let r#type: String = row.get(3)?;
                let domain: String = row.get(4)?;
                let category: String = row.get(5)?;
                let trace_keywords_raw: String = row.get(6)?;
                let source_event_ids_raw: String = row.get(7)?;
                let created_at: i64 = row.get(8)?;
                let raw_bm25: f64 = row.get::<_, Option<f64>>(9)?.unwrap_or(0.0);
                let score = (-raw_bm25).max(0.001);

                let trace_keywords = serde_json::from_str(&trace_keywords_raw).unwrap_or_default();
                let source_event_ids = serde_json::from_str(&source_event_ids_raw).unwrap_or_default();

                results.push(SearchHit {
                    id: rowid,
                    vec_id,
                    content,
                    r#type,
                    domain,
                    category,
                    score,
                    distance: 0.0,
                    trace_keywords,
                    source_event_ids,
                    created_at,
                    source_channel: "sparse_bm25".to_string(),
                });
            }
        }

        // If FTS returned results, return them
        if !results.is_empty() {
            return Ok(results);
        }

        // Fallback: in-memory scan with Vietnamese diacritic-insensitive normalization over vectors_meta
        self.execute_meta_diacritic_fallback(conn, query, &meta_sql, &params, top_k)
    }

    fn execute_fts_fallback(
        &self,
        conn: &Connection,
        sql: &str,
        query_fts: &str,
        extra_params: &[Value],
        raw_query: &str,
        top_k: usize,
        meta_sql: &str,
    ) -> Result<Vec<SearchHit>, rusqlite::Error> {
        let mut stmt = conn.prepare(sql)?;
        let mut params = vec![Value::Text(query_fts.to_string())];
        params.extend(extra_params.iter().cloned());
        params.push(Value::Integer(top_k as i64));

        let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
        let mut rows = stmt.query(&param_refs[..])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            let rowid: i64 = row.get(0)?;
            let vec_id: String = row.get(1)?;
            let content: String = row.get(2)?;
            let r#type: String = row.get(3)?;
            let domain: String = row.get(4)?;
            let category: String = row.get(5)?;
            let trace_keywords_raw: String = row.get(6)?;
            let source_event_ids_raw: String = row.get(7)?;
            let created_at: i64 = row.get(8)?;

            let trace_keywords = serde_json::from_str(&trace_keywords_raw).unwrap_or_default();
            let source_event_ids = serde_json::from_str(&source_event_ids_raw).unwrap_or_default();

            results.push(SearchHit {
                id: rowid,
                vec_id,
                content,
                r#type,
                domain,
                category,
                score: 1.0,
                distance: 0.0,
                trace_keywords,
                source_event_ids,
                created_at,
                source_channel: "sparse_bm25".to_string(),
            });
        }

        if !results.is_empty() {
            return Ok(results);
        }

        self.execute_meta_diacritic_fallback(conn, raw_query, meta_sql, extra_params, top_k)
    }

    fn execute_meta_diacritic_fallback(
        &self,
        conn: &Connection,
        query: &str,
        meta_sql: &str,
        params: &[Value],
        top_k: usize,
    ) -> Result<Vec<SearchHit>, rusqlite::Error> {
        let sql = format!(
            "SELECT m.id, m.vec_id, m.content, m.domain, m.category \
             FROM vectors_meta m \
             WHERE m.type != 'conversation_turn' AND {} \
             LIMIT 1000",
            meta_sql
        );

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
        let mut rows = stmt.query(&param_refs[..])?;
        let mut docs = Vec::new();

        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let vec_id: String = row.get(1)?;
            let content: String = row.get(2)?;
            let domain: String = row.get(3)?;
            let category: String = row.get(4)?;
            docs.push((id, vec_id, content, domain, category));
        }

        Ok(self.search_in_memory(query, &docs, top_k))
    }
}

