pub mod compression;
pub mod enclave;
pub mod graph;
pub mod l1_working;
pub mod l2_episodic;
pub mod l3_semantic;
pub mod l4_procedural;
pub mod search;

pub use compression::{
    CompressionMetrics, CompressionResult, CondensationReport, DialogueTurn,
    LLMLinguaCompressor, LLMLinguaConfig, ProtectionReason, RecursiveSummaryTree,
    SummaryNode, SummaryTreeConfig, TokenMetadata,
};
pub use enclave::{EnclaveError, MemoryEnclave};
pub use graph::{CsrGraph, GraphEdge, GraphNode, HippoRagEngine, PprConfig, PprResult};
pub use l1_working::{BlockId, PrefixCacheStats, RadixNode, RadixPrefixCache, TokenId, BLOCK_SIZE};
pub use l2_episodic::{
    EpisodicEvent, L2EpisodicStore, RetentionSweepReport, RetentionTier,
    ACTIVE_RETENTION_THRESHOLD, ARCHIVE_RETENTION_THRESHOLD, DEFAULT_BASE_HALF_LIFE_SECS,
    DORMANT_RETENTION_THRESHOLD, classify_retention, compute_half_life_secs, compute_retention,
};
pub use l3_semantic::{
    extract_wikilinks, normalize_string, parse_frontmatter, percent_decode_path,
    validate_and_resolve_path, validate_frontmatter, FileValidationResult, Frontmatter,
    ObsidianVaultSync, VaultNote, VaultScanReport, VaultSecurityError, WikiLink,
};
pub use l4_procedural::{
    ExecutionOutcome, FailureType, L4ProceduralRegistry, ProceduralSkill, RankedSkill,
    compute_bayesian_expectation, compute_ranking_score,
};
pub use search::{
    aggregate_graph_activations, cosine_distance_to_similarity, cosine_similarity_f32,
    cosine_similarity_int8, dot_product_int8, normalize_vietnamese_query,
    prepare_fts5_query, quantize_int8_scaled, quantize_unit_int8, remove_diacritics,
    tokenize_normalized, Bm25Config, DenseCandidate, DenseCandidateInt8,
    DenseVecEngine, HybridSearchEngine, HybridSearchResult, RrfConfig, SearchHit,
    SparseBm25Engine, TriModalRrfEngine,
};

use std::path::Path;
use std::sync::Arc;
use crate::db::DatabasePool;

/// Unified Virtual Memory Engine Facade
/// Cohesively coordinates L1 Working Prefix Cache, L2 Episodic Memory SQLite Store,
/// L4 Procedural Memory Registry, AES-256-GCM v2 Cryptographic Enclave, and 3-Way Hybrid Search.
pub struct VirtualMemoryEngine {
    pub l1: Arc<RadixPrefixCache>,
    pub l2: Arc<L2EpisodicStore>,
    pub l4: Arc<L4ProceduralRegistry>,
    pub enclave: Arc<MemoryEnclave>,
    pub search_engine: Arc<HybridSearchEngine>,
}

impl VirtualMemoryEngine {
    /// Construct a new unified VirtualMemoryEngine with designated components.
    pub fn new(pool: DatabasePool, enclave: Arc<MemoryEnclave>, max_l1_blocks: usize) -> Self {
        let l1 = Arc::new(RadixPrefixCache::new(max_l1_blocks));
        let l2 = Arc::new(L2EpisodicStore::new(pool, enclave.clone()));
        let l4 = Arc::new(L4ProceduralRegistry::new());
        let search_engine = Arc::new(HybridSearchEngine::new());

        Self {
            l1,
            l2,
            l4,
            enclave,
            search_engine,
        }
    }

    /// Construct a VirtualMemoryEngine using Argon2id master key derivation.
    pub fn new_with_argon2id(
        pool: DatabasePool,
        passphrase: &[u8],
        salt: &[u8],
        max_l1_blocks: usize,
    ) -> Result<Self, EnclaveError> {
        let enclave = Arc::new(MemoryEnclave::new_with_argon2id(passphrase, salt)?);
        Ok(Self::new(pool, enclave, max_l1_blocks))
    }

    /// Access the L1 Working Memory Radix Prefix Cache.
    pub fn l1(&self) -> &RadixPrefixCache {
        &self.l1
    }

    /// Access the L2 Episodic Memory Store.
    pub fn l2(&self) -> &L2EpisodicStore {
        &self.l2
    }

    /// Access the L4 Procedural Skill Registry.
    pub fn l4(&self) -> &L4ProceduralRegistry {
        &self.l4
    }

    /// Access the 3-Way Hybrid Search Engine.
    pub fn search_engine(&self) -> &HybridSearchEngine {
        &self.search_engine
    }

    /// Access the Cryptographic Memory Enclave.
    pub fn enclave(&self) -> &MemoryEnclave {
        &self.enclave
    }

    /// Normalize Vietnamese text by stripping diacritics and lowercasing.
    pub fn normalize_vietnamese(&self, text: &str) -> String {
        normalize_vietnamese_query(text)
    }

    /// Execute 3-Way Reciprocal Rank Fusion over provided candidate hit channels.
    pub fn rrf_fuse(
        &self,
        bm25_hits: &[SearchHit],
        dense_hits: &[SearchHit],
        graph_hits: &[SearchHit],
        top_k: usize,
    ) -> Vec<SearchHit> {
        self.search_engine.rrf_engine.fuse(bm25_hits, dense_hits, graph_hits, top_k)
    }

    /// Execute full 3-Way Hybrid Search against database and optional knowledge graph.
    #[allow(clippy::too_many_arguments)]
    pub fn search_hybrid(
        &self,
        conn: &rusqlite::Connection,
        crypto_engine: &crate::crypto::EncryptionEngine,
        query_text: &str,
        query_vector: Option<&[f32]>,
        graph_engine: Option<&HippoRagEngine>,
        graph_seed_entities: Option<&[(&str, f32)]>,
        doc_entity_mappings: Option<&[(i64, String, String, Vec<(String, f32)>)]>,
        top_k: usize,
        filter: Option<&crate::db::MetadataFilter>,
    ) -> Result<Vec<SearchHit>, rusqlite::Error> {
        self.search_engine.search(
            conn,
            crypto_engine,
            query_text,
            query_vector,
            graph_engine,
            graph_seed_entities,
            doc_entity_mappings,
            top_k,
            filter,
        )
    }

    /// Match a token sequence against the L1 Working Memory prefix cache.
    pub async fn match_working_prefix(&self, tokens: &[TokenId]) -> (usize, Vec<BlockId>) {
        self.l1.match_prefix(tokens).await
    }

    /// Insert a token sequence into the L1 Working Memory prefix cache.
    pub async fn insert_working_prefix(&self, tokens: &[TokenId], block_id: BlockId, is_pinned: bool) {
        self.l1.insert_prefix(tokens, block_id, is_pinned).await;
    }

    /// Pin a token sequence in the L1 Working Memory prefix cache.
    pub async fn pin_working_prefix(&self, tokens: &[TokenId]) -> bool {
        self.l1.pin_prefix(tokens).await
    }

    /// Evict LRU unpinned blocks from the L1 Working Memory prefix cache.
    pub async fn evict_working_lru(&self, count: usize) -> usize {
        self.l1.evict_lru(count).await
    }

    /// Retrieve statistics for L1 Working Memory.
    pub fn working_prefix_stats(&self) -> PrefixCacheStats {
        self.l1.stats()
    }

    /// Insert a new episodic event into encrypted L2 storage.
    pub fn record_episodic_event(&self, event: &EpisodicEvent) -> Result<String, String> {
        self.l2.insert_event(event)
    }

    /// Retrieve active episodic memories for a domain above retention threshold.
    pub fn recall_episodic_context(&self, domain: &str, threshold: f64) -> Result<Vec<EpisodicEvent>, String> {
        self.l2.get_active_events(domain, threshold)
    }

    /// Execute a retention decay sweep across all episodic memories.
    pub fn sweep_episodic_retention(&self, current_timestamp: i64) -> Result<RetentionSweepReport, String> {
        self.l2.sweep_retention(current_timestamp)
    }

    /// Purge decayed episodic events below cutoff retention score.
    pub fn purge_decayed_episodic_events(&self, cutoff_score: f64) -> Result<usize, String> {
        self.l2.purge_decayed_events(cutoff_score)
    }

    /// Scan and validate Obsidian markdown vault, populating internal notes index.
    pub fn sync_obsidian_vault(&self, vault_path: &Path) -> Result<(ObsidianVaultSync, VaultScanReport), String> {
        let mut sync = ObsidianVaultSync::new(vault_path.to_path_buf(), Some(self.l2.pool().clone()))?;
        let report = sync.scan_vault()?;
        Ok((sync, report))
    }

    /// Build a HippoRAG engine directly from an active ObsidianVaultSync instance.
    pub fn build_hipporag_from_vault(&self, vault_sync: &ObsidianVaultSync, bidirectional: bool) -> HippoRagEngine {
        let graph = vault_sync.build_csr_graph(bidirectional);
        HippoRagEngine::new(graph)
    }

    /// Build a HippoRAG engine directly from SQLite l3_edges and l3_nodes tables.
    pub fn build_hipporag_from_db(
        &self,
        conn: &rusqlite::Connection,
        bidirectional: bool,
    ) -> Result<HippoRagEngine, rusqlite::Error> {
        let mut stmt_nodes = conn.prepare("SELECT id, label, properties FROM l3_nodes")?;
        let node_rows = stmt_nodes.query_map([], |row| {
            let id: String = row.get(0)?;
            let label: String = row.get(1)?;
            let props: String = row.get(2)?;
            Ok((id, label, props))
        })?;
        let mut nodes = Vec::new();
        for nr in node_rows {
            nodes.push(nr?);
        }

        let mut stmt_edges = conn.prepare("SELECT source, target, relation, weight, obsolete FROM l3_edges WHERE obsolete = 0")?;
        let edge_rows = stmt_edges.query_map([], |row| {
            let src: String = row.get(0)?;
            let dst: String = row.get(1)?;
            let rel: String = row.get(2)?;
            let w: f64 = row.get(3)?;
            let obs: i32 = row.get(4)?;
            Ok((src, dst, rel, w as f32, obs))
        })?;
        let mut edges = Vec::new();
        for er in edge_rows {
            edges.push(er?);
        }

        let graph = CsrGraph::from_db_records(&nodes, &edges, bidirectional);
        Ok(HippoRagEngine::new(graph))
    }

    /// Rank procedural skills against query similarity scores with Bayesian failure penalties.
    pub async fn rank_procedural_skills(&self, query_similarities: &[(String, f64)]) -> Vec<RankedSkill> {
        self.l4.rank_skills(query_similarities).await
    }

    /// Record an execution outcome for a procedural skill.
    pub async fn record_skill_outcome(
        &self,
        skill_id: &str,
        outcome: &ExecutionOutcome,
    ) -> Result<(f64, f64), String> {
        self.l4.record_outcome(skill_id, outcome).await
    }

    /// Sanitize SQLite WAL to eradicate plaintext residues.
    pub fn sanitize_wal(&self, conn: &rusqlite::Connection) -> Result<(), EnclaveError> {
        MemoryEnclave::sanitize_wal_checkpoint(conn)
    }

    /// Encrypt plaintext string using AES-256-GCM v2 enclave.
    pub fn encrypt_str(&self, plaintext: &str) -> Result<String, EnclaveError> {
        self.enclave.encrypt_string(plaintext)
    }

    /// Decrypt ciphertext string using AES-256-GCM v2 enclave.
    pub fn decrypt_str(&self, ciphertext: &str) -> Result<zeroize::Zeroizing<String>, EnclaveError> {
        self.enclave.decrypt_string(ciphertext)
    }

    /// Decrypt fact read with fail-closed semantics.
    pub fn decrypt_read(&self, ciphertext: &str) -> crate::crypto::FactRead {
        self.enclave.read_record(ciphertext)
    }

    /// Compress context text using LLMLingua-2 token entropy pruning.
    pub fn compress_context(&self, text: &str, config: Option<LLMLinguaConfig>) -> CompressionResult {
        let compressor = match config {
            Some(cfg) => LLMLinguaCompressor::with_config(cfg),
            None => LLMLinguaCompressor::new(),
        };
        compressor.compress(text)
    }

    /// Instantiate a RecursiveSummaryTree for a session with SQLite persistence.
    pub fn create_summary_tree(&self, session_id: &str, config: Option<SummaryTreeConfig>) -> RecursiveSummaryTree {
        let cfg = config.unwrap_or_default();
        RecursiveSummaryTree::new(session_id, cfg).with_db_pool(self.l2.pool().clone())
    }
}

