pub mod dense_vec;
pub mod hybrid_fusion;
pub mod sparse_bm25;

pub use dense_vec::{
    cosine_distance_to_similarity, cosine_similarity_f32, cosine_similarity_int8,
    dot_product_int8, quantize_int8_scaled, quantize_unit_int8, DenseCandidate,
    DenseCandidateInt8, DenseVecEngine,
};
pub use hybrid_fusion::{
    aggregate_graph_activations, HybridSearchEngine, HybridSearchResult, RrfConfig,
    TriModalRrfEngine,
};
pub use sparse_bm25::{
    normalize_vietnamese_query, prepare_fts5_query, remove_diacritics, tokenize_normalized,
    Bm25Config, SearchHit, SparseBm25Engine,
};
