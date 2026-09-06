use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use rusqlite::Connection;
use crate::crypto::EncryptionEngine;
use crate::db::MetadataFilter;
use crate::memory::graph::hipporag::{HippoRagEngine, PprConfig};
use crate::memory::search::dense_vec::DenseVecEngine;
use crate::memory::search::sparse_bm25::{SearchHit, SparseBm25Engine};

/// Configuration for 3-Way Reciprocal Rank Fusion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RrfConfig {
    /// RRF smoothing constant K (default = 60.0)
    pub k: f64,
    /// Weight for Sparse Lexical BM25 channel (default = 0.30)
    pub weight_bm25: f64,
    /// Weight for Dense Vector channel (default = 0.45)
    pub weight_dense: f64,
    /// Weight for HippoRAG Graph PPR channel (default = 0.25)
    pub weight_graph: f64,
}

impl Default for RrfConfig {
    fn default() -> Self {
        Self {
            k: 60.0,
            weight_bm25: 0.30,
            weight_dense: 0.45,
            weight_graph: 0.25,
        }
    }
}

/// Detailed Hybrid Search Result with per-channel breakdown and RRF rank lineage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HybridSearchResult {
    pub hit: SearchHit,
    pub rrf_score: f64,
    pub bm25_rank: Option<usize>,
    pub dense_rank: Option<usize>,
    pub graph_rank: Option<usize>,
    pub bm25_score: f64,
    pub dense_score: f64,
    pub graph_score: f64,
}

/// 3-Way Reciprocal Rank Fusion Engine combining BM25, Dense Vector, and HippoRAG PPR activations.
#[derive(Debug, Clone)]
pub struct TriModalRrfEngine {
    pub config: RrfConfig,
}

impl Default for TriModalRrfEngine {
    fn default() -> Self {
        Self {
            config: RrfConfig::default(),
        }
    }
}

impl TriModalRrfEngine {
    /// Create a new TriModalRrfEngine with default configuration (K=60.0, w_bm25=0.30, w_dense=0.45, w_graph=0.25).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a TriModalRrfEngine with explicit configuration.
    pub fn with_config(config: RrfConfig) -> Self {
        Self { config }
    }

    /// Create a TriModalRrfEngine with custom weights and K constant.
    pub fn with_weights(k: f64, weight_bm25: f64, weight_dense: f64, weight_graph: f64) -> Self {
        Self {
            config: RrfConfig {
                k,
                weight_bm25,
                weight_dense,
                weight_graph,
            },
        }
    }

    /// Fuse candidate hit lists from 3 channels (BM25, Dense Vector, and Graph Activation) into ranked SearchHits.
    pub fn fuse(
        &self,
        bm25_hits: &[SearchHit],
        dense_hits: &[SearchHit],
        graph_hits: &[SearchHit],
        top_k: usize,
    ) -> Vec<SearchHit> {
        let detailed = self.fuse_detailed(bm25_hits, dense_hits, graph_hits, top_k);
        detailed
            .into_iter()
            .map(|d| {
                let mut h = d.hit;
                h.score = d.rrf_score;
                h.source_channel = "3way_rrf".to_string();
                h
            })
            .collect()
    }

    /// Fuse candidate hit lists and preserve rich per-channel rank/score attribution details.
    pub fn fuse_detailed(
        &self,
        bm25_hits: &[SearchHit],
        dense_hits: &[SearchHit],
        graph_hits: &[SearchHit],
        top_k: usize,
    ) -> Vec<HybridSearchResult> {
        let mut candidates: HashMap<String, (SearchHit, Option<usize>, Option<usize>, Option<usize>, f64, f64, f64, f64)> =
            HashMap::new();

        let k = self.config.k.max(1.0);
        let w_bm25 = self.config.weight_bm25;
        let w_dense = self.config.weight_dense;
        let w_graph = self.config.weight_graph;

        // 1. Channel 1: Sparse BM25 Ranks
        for (index, hit) in bm25_hits.iter().enumerate() {
            let rank = index + 1;
            let contribution = w_bm25 * (1.0 / (k + rank as f64));
            let entry = candidates.entry(hit.vec_id.clone()).or_insert_with(|| {
                (hit.clone(), None, None, None, 0.0, 0.0, 0.0, 0.0)
            });
            entry.1 = Some(rank);
            entry.4 = hit.score;
            entry.7 += contribution;
        }

        // 2. Channel 2: Dense Vector Ranks
        for (index, hit) in dense_hits.iter().enumerate() {
            let rank = index + 1;
            let contribution = w_dense * (1.0 / (k + rank as f64));
            let entry = candidates.entry(hit.vec_id.clone()).or_insert_with(|| {
                (hit.clone(), None, None, None, 0.0, 0.0, 0.0, 0.0)
            });
            entry.2 = Some(rank);
            entry.5 = hit.score;
            entry.7 += contribution;
            // Prefer dense hit's metadata or distance if available
            if entry.0.distance == 0.0 && hit.distance > 0.0 {
                entry.0.distance = hit.distance;
            }
        }

        // 3. Channel 3: HippoRAG Graph PPR Ranks
        for (index, hit) in graph_hits.iter().enumerate() {
            let rank = index + 1;
            let contribution = w_graph * (1.0 / (k + rank as f64));
            let entry = candidates.entry(hit.vec_id.clone()).or_insert_with(|| {
                (hit.clone(), None, None, None, 0.0, 0.0, 0.0, 0.0)
            });
            entry.3 = Some(rank);
            entry.6 = hit.score;
            entry.7 += contribution;
        }

        let mut results: Vec<HybridSearchResult> = candidates
            .into_values()
            .map(|(hit, bm25_rank, dense_rank, graph_rank, bm25_score, dense_score, graph_score, rrf_score)| {
                HybridSearchResult {
                    hit,
                    rrf_score,
                    bm25_rank,
                    dense_rank,
                    graph_rank,
                    bm25_score,
                    dense_score,
                    graph_score,
                }
            })
            .collect();

        // Deterministic sort: RRF score descending, tie-break by vec_id ascending
        results.sort_by(|a, b| {
            b.rrf_score
                .partial_cmp(&a.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.hit.vec_id.cmp(&b.hit.vec_id))
        });

        results.truncate(top_k);
        results
    }
}

/// Compute document graph activation scores from HippoRAG stationary node probabilities.
/// Aggregates: S_graph(d) = sum_{v in Entities(d)} pi*(v) * weight(v, d).
pub fn aggregate_graph_activations(
    ppr_distribution: &[f32],
    node_to_idx: &HashMap<String, u32>,
    doc_entities: &[(i64, String, String, Vec<(String, f32)>)], // (id, vec_id, content, [(entity_id, weight)])
    top_k: usize,
) -> Vec<SearchHit> {
    if ppr_distribution.is_empty() || doc_entities.is_empty() {
        return Vec::new();
    }

    let mut scored_docs = Vec::new();

    for (id, vec_id, content, entities) in doc_entities {
        let mut total_activation = 0.0f64;

        for (entity_name, entity_weight) in entities {
            if let Some(&idx) = node_to_idx.get(entity_name) {
                if (idx as usize) < ppr_distribution.len() {
                    let prob = ppr_distribution[idx as usize] as f64;
                    total_activation += prob * (*entity_weight as f64);
                }
            }
        }

        if total_activation > 0.0 {
            scored_docs.push(SearchHit {
                id: *id,
                vec_id: vec_id.clone(),
                content: content.clone(),
                r#type: "fact".to_string(),
                domain: "local".to_string(),
                category: "general".to_string(),
                score: total_activation,
                distance: 0.0,
                trace_keywords: entities.iter().map(|(e, _)| e.clone()).collect(),
                source_event_ids: Vec::new(),
                created_at: chrono::Utc::now().timestamp_millis(),
                source_channel: "hipporag_graph".to_string(),
            });
        }
    }

    scored_docs.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.vec_id.cmp(&b.vec_id))
    });

    scored_docs.truncate(top_k);
    scored_docs
}

/// High-Level 3-Way Hybrid Search Engine coordinating Sparse BM25, Dense Vector, and HippoRAG PPR.
pub struct HybridSearchEngine {
    pub sparse_engine: SparseBm25Engine,
    pub dense_engine: DenseVecEngine,
    pub rrf_engine: TriModalRrfEngine,
}

impl Default for HybridSearchEngine {
    fn default() -> Self {
        Self {
            sparse_engine: SparseBm25Engine::new(),
            dense_engine: DenseVecEngine::new(),
            rrf_engine: TriModalRrfEngine::new(),
        }
    }
}

impl HybridSearchEngine {
    /// Create a new HybridSearchEngine with default components.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a HybridSearchEngine with custom RRF configuration.
    pub fn with_rrf_config(config: RrfConfig) -> Self {
        Self {
            sparse_engine: SparseBm25Engine::new(),
            dense_engine: DenseVecEngine::new(),
            rrf_engine: TriModalRrfEngine::with_config(config),
        }
    }

    /// Execute 3-way hybrid search against an active SQLite database pool and optional HippoRAG Graph Engine.
    #[allow(clippy::too_many_arguments)]
    pub fn search(
        &self,
        conn: &Connection,
        crypto_engine: &EncryptionEngine,
        query_text: &str,
        query_vector: Option<&[f32]>,
        graph_engine: Option<&HippoRagEngine>,
        graph_seed_entities: Option<&[(&str, f32)]>,
        doc_entity_mappings: Option<&[(i64, String, String, Vec<(String, f32)>)]>,
        top_k: usize,
        filter: Option<&MetadataFilter>,
    ) -> Result<Vec<SearchHit>, rusqlite::Error> {
        let fusion_pool_size = top_k.max(1) * 3;

        // 1. Channel 1: Sparse BM25 Lexical Retrieval
        let bm25_hits = if !query_text.trim().is_empty() {
            self.sparse_engine
                .search_fts5(conn, query_text, fusion_pool_size, filter)
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // 2. Channel 2: Dense Vector Retrieval (sqlite-vec)
        let dense_hits = if let Some(q_vec) = query_vector {
            if !q_vec.is_empty() {
                self.dense_engine
                    .search_dense(conn, crypto_engine, q_vec, fusion_pool_size, filter)
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // 3. Channel 3: HippoRAG Graph Activation Retrieval
        let graph_hits = if let (Some(g_engine), Some(seeds), Some(doc_entities)) =
            (graph_engine, graph_seed_entities, doc_entity_mappings)
        {
            if !seeds.is_empty() {
                let ppr_result = g_engine.run_ppr_configured(
                    &seeds
                        .iter()
                        .filter_map(|(s, _)| g_engine.graph.node_index(s))
                        .collect::<Vec<u32>>(),
                    &seeds
                        .iter()
                        .filter_map(|(s, w)| {
                            if g_engine.graph.node_index(s).is_some() {
                                Some(*w)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<f32>>(),
                    &PprConfig::default(),
                );

                aggregate_graph_activations(
                    &ppr_result.probabilities,
                    &g_engine.graph.node_to_idx,
                    doc_entities,
                    fusion_pool_size,
                )
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // 4. Execute 3-Way Reciprocal Rank Fusion
        Ok(self.rrf_engine.fuse(&bm25_hits, &dense_hits, &graph_hits, top_k))
    }
}
