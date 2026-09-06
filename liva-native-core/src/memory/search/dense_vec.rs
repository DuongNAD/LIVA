use serde::{Deserialize, Serialize};
use rusqlite::{Connection, ToSql};
use rusqlite::types::Value;
use crate::crypto::EncryptionEngine;
use crate::db::MetadataFilter;
use crate::memory::search::sparse_bm25::SearchHit;

/// Convert a unit-normalized float32 vector into an INT8 quantized byte vector.
/// Maps range [-1.0, 1.0] to [-127, 127].
pub fn quantize_unit_int8(vec: &[f32]) -> Vec<i8> {
    vec.iter()
        .map(|&x| {
            let scaled = (x * 127.0).round();
            scaled.clamp(-128.0, 127.0) as i8
        })
        .collect()
}

/// Compute scalar INT8 quantization with dynamic mean and standard deviation.
pub fn quantize_int8_scaled(vec: &[f32]) -> Vec<i8> {
    if vec.is_empty() {
        return Vec::new();
    }

    let mean = vec.iter().sum::<f32>() / (vec.len() as f32);
    let variance = vec.iter().map(|&x| (x - mean).powi(2)).sum::<f32>() / (vec.len() as f32);
    let std_dev = variance.sqrt().max(1e-6);

    vec.iter()
        .map(|&x| {
            let normalized = ((x - mean) / std_dev) * 64.0;
            normalized.round().clamp(-128.0, 127.0) as i8
        })
        .collect()
}

/// Fast dot product for INT8 vectors.
#[inline]
pub fn dot_product_int8(a: &[i8], b: &[i8]) -> i32 {
    debug_assert_eq!(a.len(), b.len());
    let mut sum = 0i32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        sum += (x as i32) * (y as i32);
    }
    sum
}

/// Cosine similarity between two INT8 quantized vectors in [-1.0, 1.0].
pub fn cosine_similarity_int8(a: &[i8], b: &[i8]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let dot = dot_product_int8(a, b) as f32;
    let norm_a: f32 = a.iter().map(|&x| (x as f32).powi(2)).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|&x| (x as f32).powi(2)).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
    }
}

/// Float32 cosine similarity between two float vectors in [-1.0, 1.0].
pub fn cosine_similarity_f32(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        (dot / denom).clamp(-1.0, 1.0)
    }
}

/// Convert sqlite-vec L2/cosine distance metric into a normalized similarity score in [0.0, 1.0].
/// Matches LIVA sqlite-vec scaling: similarity = (1.0 - (dist_f32 * dist_f32) / 2.0).max(0.0) where dist_f32 = distance / 120.0.
#[inline]
pub fn cosine_distance_to_similarity(distance: f64) -> f64 {
    let dist_f32 = distance / 120.0;
    (1.0 - (dist_f32 * dist_f32) / 2.0).max(0.0)
}

/// Candidate document vector for in-memory dense retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenseCandidate {
    pub id: i64,
    pub vec_id: String,
    pub content: String,
    pub r#type: String,
    pub domain: String,
    pub category: String,
    pub vector: Vec<f32>,
    pub decay_weight: f64,
    pub created_at: i64,
}

impl DenseCandidate {
    pub fn new(
        id: i64,
        vec_id: impl Into<String>,
        content: impl Into<String>,
        vector: Vec<f32>,
    ) -> Self {
        Self {
            id,
            vec_id: vec_id.into(),
            content: content.into(),
            r#type: "fact".to_string(),
            domain: "local".to_string(),
            category: "general".to_string(),
            vector,
            decay_weight: 1.0,
            created_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// INT8 Quantized Candidate for memory-constrained retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenseCandidateInt8 {
    pub id: i64,
    pub vec_id: String,
    pub content: String,
    pub r#type: String,
    pub domain: String,
    pub category: String,
    pub vector_int8: Vec<i8>,
    pub decay_weight: f64,
    pub created_at: i64,
}

/// Dense Vector Search Engine for sqlite-vec and in-memory retrieval.
#[derive(Debug, Clone, Default)]
pub struct DenseVecEngine;

impl DenseVecEngine {
    /// Create a new DenseVecEngine.
    pub fn new() -> Self {
        Self
    }

    /// Search an in-memory collection of float candidates using cosine similarity.
    pub fn search_in_memory(
        &self,
        query_vector: &[f32],
        candidates: &[DenseCandidate],
        top_k: usize,
    ) -> Vec<SearchHit> {
        if query_vector.is_empty() || candidates.is_empty() {
            return Vec::new();
        }

        let mut scored_hits = Vec::with_capacity(candidates.len());

        for cand in candidates {
            let sim = cosine_similarity_f32(query_vector, &cand.vector) as f64;
            let score = (sim.max(0.0)) * cand.decay_weight;

            scored_hits.push(SearchHit {
                id: cand.id,
                vec_id: cand.vec_id.clone(),
                content: cand.content.clone(),
                r#type: cand.r#type.clone(),
                domain: cand.domain.clone(),
                category: cand.category.clone(),
                score,
                distance: ((1.0 - sim) * 120.0).max(0.0),
                trace_keywords: Vec::new(),
                source_event_ids: Vec::new(),
                created_at: cand.created_at,
                source_channel: "dense_vec".to_string(),
            });
        }

        scored_hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });

        scored_hits.truncate(top_k);
        scored_hits
    }

    /// Search an in-memory collection of INT8 quantized candidates using quantized dot product.
    pub fn search_in_memory_int8(
        &self,
        query_vector: &[i8],
        candidates: &[DenseCandidateInt8],
        top_k: usize,
    ) -> Vec<SearchHit> {
        if query_vector.is_empty() || candidates.is_empty() {
            return Vec::new();
        }

        let mut scored_hits = Vec::with_capacity(candidates.len());

        for cand in candidates {
            let sim = cosine_similarity_int8(query_vector, &cand.vector_int8) as f64;
            let score = (sim.max(0.0)) * cand.decay_weight;

            scored_hits.push(SearchHit {
                id: cand.id,
                vec_id: cand.vec_id.clone(),
                content: cand.content.clone(),
                r#type: cand.r#type.clone(),
                domain: cand.domain.clone(),
                category: cand.category.clone(),
                score,
                distance: ((1.0 - sim) * 120.0).max(0.0),
                trace_keywords: Vec::new(),
                source_event_ids: Vec::new(),
                created_at: cand.created_at,
                source_channel: "dense_vec".to_string(),
            });
        }

        scored_hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });

        scored_hits.truncate(top_k);
        scored_hits
    }

    /// Execute sqlite-vec dense vector search on database connection pool.
    pub fn search_dense(
        &self,
        conn: &Connection,
        engine: &EncryptionEngine,
        query_vector: &[f32],
        top_k: usize,
        filter: Option<&MetadataFilter>,
    ) -> Result<Vec<SearchHit>, rusqlite::Error> {
        let blob = bytemuck::cast_slice::<f32, u8>(query_vector);
        let mut conditions = Vec::new();
        let mut filter_params: Vec<Value> = Vec::new();

        if let Some(f) = filter {
            if let Some(t) = &f.r#type {
                conditions.push("m.type = ?");
                filter_params.push(Value::Text(t.clone()));
            }
            if let Some(d) = &f.domain {
                conditions.push("m.domain = ?");
                filter_params.push(Value::Text(d.clone()));
            }
            if let Some(c) = &f.category {
                conditions.push("m.category = ?");
                filter_params.push(Value::Text(c.clone()));
            }
            if let Some(after) = f.created_after {
                conditions.push("m.created_at >= ?");
                filter_params.push(Value::Integer(after));
            }
            if let Some(before) = f.created_before {
                conditions.push("m.created_at <= ?");
                filter_params.push(Value::Integer(before));
            }
        }

        let has_filter = !conditions.is_empty();
        let fetch_k = if has_filter { top_k * 3 } else { top_k };

        let (sql, params) = if has_filter {
            let sql = format!(
                "SELECT v.rowid, v.distance, m.vec_id, m.content, m.type, m.domain, m.category, \
                        m.trace_keywords, m.source_event_ids, m.decay_weight, m.created_at \
                 FROM vec_idx v \
                 INNER JOIN vectors_meta m ON m.id = v.rowid \
                 WHERE v.embedding MATCH vec_quantize_int8(?, 'unit') \
                   AND v.k = ? \
                   AND {} \
                 ORDER BY v.distance ASC",
                conditions.join(" AND ")
            );
            let mut p = vec![Value::Blob(blob.to_vec()), Value::Integer(fetch_k as i64)];
            p.extend(filter_params);
            (sql, p)
        } else {
            let sql = "SELECT v.rowid, v.distance, m.vec_id, m.content, m.type, m.domain, m.category, \
                              m.trace_keywords, m.source_event_ids, m.decay_weight, m.created_at \
                       FROM vec_idx v \
                       INNER JOIN vectors_meta m ON m.id = v.rowid \
                       WHERE v.embedding MATCH vec_quantize_int8(?, 'unit') \
                         AND v.k = ? \
                       ORDER BY v.distance ASC"
                .to_string();
            let p = vec![Value::Blob(blob.to_vec()), Value::Integer(fetch_k as i64)];
            (sql, p)
        };

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
        let mut rows = stmt.query(&params_refs[..])?;
        let mut results = Vec::new();

        while let Some(row) = rows.next()? {
            let rowid: i64 = row.get(0)?;
            let distance: f64 = row.get::<_, Option<f64>>(1)?.unwrap_or(f64::INFINITY);
            let vec_id: String = row.get(2)?;
            let stored_content: String = row.get(3)?;
            let r#type: String = row.get(4)?;
            let domain: String = row.get(5)?;
            let category: String = row.get(6)?;
            let trace_keywords_raw: String = row.get(7)?;
            let source_event_ids_raw: String = row.get(8)?;
            let decay_weight: f64 = row.get(9)?;
            let created_at: i64 = row.get(10)?;

            let content = if r#type == "conversation_turn" {
                match engine.read_fact(&stored_content) {
                    crate::crypto::FactRead::Ok(plain) => plain,
                    crate::crypto::FactRead::Locked { .. } => {
                        continue;
                    }
                }
            } else {
                stored_content
            };

            let trace_keywords = serde_json::from_str(&trace_keywords_raw).unwrap_or_default();
            let source_event_ids = serde_json::from_str(&source_event_ids_raw).unwrap_or_default();

            let similarity = cosine_distance_to_similarity(distance);
            let score = similarity * decay_weight;

            results.push(SearchHit {
                id: rowid,
                vec_id,
                content,
                r#type,
                domain,
                category,
                distance,
                score,
                trace_keywords,
                source_event_ids,
                created_at,
                source_channel: "dense_vec".to_string(),
            });
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });

        if results.len() > top_k {
            results.truncate(top_k);
        }

        Ok(results)
    }
}
