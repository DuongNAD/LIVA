//! Hierarchical Recursive Summary Tree Engine
//!
//! Provides dynamic context overflow detection and recursive summarization for dialogue turns.
//! When working context exceeds the token budget threshold ($N \ge \rho \cdot W_{\max}$),
//! elder turn clusters are automatically condensed into structured chunk summaries (Level 1)
//! and higher-order meta-summaries (Level 2+) with Merkle cryptographic hash verification
//! and full turn ID lineage tracking in SQLite.

use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use uuid::Uuid;

use crate::db::DatabasePool;

static ENTITY_FINDER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b[A-Z][a-zA-Z0-9_\-]*(?:\s+[A-Z][a-zA-Z0-9_\-]*)*\b|\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|[^\]]*)?\]\]")
        .expect("Valid entity finder regex")
});

/// Configuration for the Recursive Summary Tree.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SummaryTreeConfig {
    /// Maximum context window budget in tokens ($W_{\max}$, default: 16,384).
    pub max_context_tokens: usize,

    /// Context overflow trigger ratio ($\rho \in [0.5, 0.95]$, default: 0.85 $\implies 13,926$ tokens).
    pub overflow_threshold_ratio: f64,

    /// Target token count per chunk summary ($S_{\text{chunk}}$, default: 1024).
    pub chunk_target_tokens: usize,

    /// Minimum turns required to form a chunk summary (default: 2).
    pub min_turns_per_chunk: usize,

    /// Maximum turns in a single chunk summary (default: 16).
    pub max_turns_per_chunk: usize,

    /// Minimum number of Level 1 summaries required to trigger a Level 2 Meta-Summary (default: 3).
    pub meta_summary_fanout: usize,

    /// Default domain for episodic storage (default: "General").
    pub domain: String,

    /// Default category for episodic storage (default: "ConversationSummary").
    pub category: String,
}

impl Default for SummaryTreeConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 16_384,
            overflow_threshold_ratio: 0.85,
            chunk_target_tokens: 1024,
            min_turns_per_chunk: 2,
            max_turns_per_chunk: 16,
            meta_summary_fanout: 3,
            domain: "General".to_string(),
            category: "ConversationSummary".to_string(),
        }
    }
}

/// An individual dialogue turn in working memory (Level 0 leaf).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DialogueTurn {
    pub turn_id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub token_count: usize,
    pub timestamp: i64,
    pub metadata: HashMap<String, String>,
}

impl DialogueTurn {
    pub fn new(session_id: &str, role: &str, content: &str, timestamp: i64) -> Self {
        let estimated_tokens = Self::estimate_tokens(content);
        Self {
            turn_id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            token_count: estimated_tokens,
            timestamp,
            metadata: HashMap::new(),
        }
    }

    pub fn with_id(turn_id: &str, session_id: &str, role: &str, content: &str, timestamp: i64) -> Self {
        let estimated_tokens = Self::estimate_tokens(content);
        Self {
            turn_id: turn_id.to_string(),
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            token_count: estimated_tokens,
            timestamp,
            metadata: HashMap::new(),
        }
    }

    /// Fast token estimation (roughly 1 token ≈ 4 characters or word-based).
    pub fn estimate_tokens(text: &str) -> usize {
        let words = text.split_whitespace().count();
        let chars = text.len();
        ((words as f64 * 1.3).max(chars as f64 / 3.8).round() as usize).max(1)
    }
}

/// A structured summary node in the hierarchical tree (Level 1 or Level 2+).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SummaryNode {
    pub node_id: String,
    pub session_id: String,
    pub level: u32,
    pub title: String,
    pub abstract_text: String,
    pub key_entities: Vec<String>,
    pub source_turn_ids: Vec<String>,
    pub child_node_ids: Vec<String>,
    pub merkle_hash: String,
    pub token_count: usize,
    pub created_at: i64,
    pub vector_embedding: Option<Vec<f32>>,
}

impl SummaryNode {
    /// Compute deterministic SHA-256 Merkle hash for integrity verification.
    pub fn compute_merkle_hash(
        level: u32,
        title: &str,
        abstract_text: &str,
        source_turn_ids: &[String],
        child_node_ids: &[String],
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(level.to_be_bytes());
        hasher.update(title.as_bytes());
        hasher.update(b"|");
        hasher.update(abstract_text.as_bytes());
        hasher.update(b"|");

        let mut sorted_turns = source_turn_ids.to_vec();
        sorted_turns.sort();
        for id in sorted_turns {
            hasher.update(id.as_bytes());
            hasher.update(b",");
        }

        let mut sorted_children = child_node_ids.to_vec();
        sorted_children.sort();
        for id in sorted_children {
            hasher.update(id.as_bytes());
            hasher.update(b";");
        }

        hex::encode(hasher.finalize())
    }

    /// Verify cryptographic Merkle integrity of this node.
    pub fn verify_integrity(&self) -> bool {
        let expected = Self::compute_merkle_hash(
            self.level,
            &self.title,
            &self.abstract_text,
            &self.source_turn_ids,
            &self.child_node_ids,
        );
        self.merkle_hash == expected
    }
}

/// Report produced when context condensation is executed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CondensationReport {
    pub triggered: bool,
    pub initial_working_tokens: usize,
    pub final_working_tokens: usize,
    pub tokens_saved: usize,
    pub new_level1_summaries: Vec<String>,
    pub new_meta_summaries: Vec<String>,
    pub condensed_turn_count: usize,
    pub remaining_active_turns: usize,
}

/// Hierarchical Recursive Summary Tree Engine.
pub struct RecursiveSummaryTree {
    config: SummaryTreeConfig,
    session_id: String,
    active_turns: Vec<DialogueTurn>,
    summary_nodes: HashMap<String, SummaryNode>,
    active_root_summaries: Vec<String>,
    pool: Option<DatabasePool>,
}

impl RecursiveSummaryTree {
    /// Create a new summary tree for a session.
    pub fn new(session_id: &str, config: SummaryTreeConfig) -> Self {
        Self {
            config,
            session_id: session_id.to_string(),
            active_turns: Vec::new(),
            summary_nodes: HashMap::new(),
            active_root_summaries: Vec::new(),
            pool: None,
        }
    }

    /// Attach a DatabasePool for SQLite episode lineage persistence.
    pub fn with_db_pool(mut self, pool: DatabasePool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Get active session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get active configuration.
    pub fn config(&self) -> &SummaryTreeConfig {
        &self.config
    }

    /// Number of active unconsolidated turns.
    pub fn active_turn_count(&self) -> usize {
        self.active_turns.len()
    }

    /// Number of active root summaries.
    pub fn active_summary_count(&self) -> usize {
        self.active_root_summaries.len()
    }

    /// Calculate the dynamic overflow trigger threshold in tokens ($T_{\text{overflow}} = \rho \cdot W_{\max}$).
    pub fn overflow_threshold(&self) -> usize {
        ((self.config.max_context_tokens as f64 * self.config.overflow_threshold_ratio).round() as usize)
            .max(1)
    }

    /// Calculate total working tokens currently in context (active summaries + active turns).
    pub fn total_working_tokens(&self) -> usize {
        let turn_tokens: usize = self.active_turns.iter().map(|t| t.token_count).sum();
        let summary_tokens: usize = self
            .active_root_summaries
            .iter()
            .filter_map(|id| self.summary_nodes.get(id))
            .map(|node| node.token_count)
            .sum();
        turn_tokens + summary_tokens
    }

    /// Check whether active tokens exceed the overflow threshold.
    pub fn is_overflow(&self) -> bool {
        self.total_working_tokens() >= self.overflow_threshold()
    }

    /// Add a dialogue turn to working memory, triggering recursive condensation if overflow occurs.
    pub fn add_turn(&mut self, turn: DialogueTurn) -> Option<CondensationReport> {
        self.active_turns.push(turn);
        if self.is_overflow() {
            Some(self.condense_overflow())
        } else {
            None
        }
    }

    /// Execute condensation on the oldest turns when context overflows.
    pub fn condense_overflow(&mut self) -> CondensationReport {
        let initial_tokens = self.total_working_tokens();
        let threshold = self.overflow_threshold();

        if self.active_turns.len() < self.config.min_turns_per_chunk {
            return CondensationReport {
                triggered: false,
                initial_working_tokens: initial_tokens,
                final_working_tokens: initial_tokens,
                tokens_saved: 0,
                new_level1_summaries: Vec::new(),
                new_meta_summaries: Vec::new(),
                condensed_turn_count: 0,
                remaining_active_turns: self.active_turns.len(),
            };
        }

        let mut new_level1_nodes = Vec::new();
        let mut total_condensed_turns = 0;
        let mut first_iteration = true;

        // Iteratively condense chunks of elder turns until working tokens are safely below threshold
        while (first_iteration || self.total_working_tokens() >= threshold)
            && self.active_turns.len() >= self.config.min_turns_per_chunk
        {
            first_iteration = false;
            // Determine how many turns to include in this chunk
            let mut chunk_turns_count = 0;
            let mut accumulated_tokens = 0;

            for turn in &self.active_turns {
                accumulated_tokens += turn.token_count;
                chunk_turns_count += 1;
                if accumulated_tokens >= self.config.chunk_target_tokens
                    || chunk_turns_count >= self.config.max_turns_per_chunk
                {
                    break;
                }
            }

            // Ensure we take at least min_turns_per_chunk
            let take_count = chunk_turns_count
                .max(self.config.min_turns_per_chunk)
                .min(self.active_turns.len());

            let chunk_turns: Vec<DialogueTurn> = self.active_turns.drain(0..take_count).collect();
            total_condensed_turns += chunk_turns.len();

            let summary_node = self.synthesize_chunk_summary(&chunk_turns);
            let node_id = summary_node.node_id.clone();

            // Persist to SQLite if pool is present
            if let Some(pool) = &self.pool {
                let _ = self.persist_summary_to_sqlite(pool, &summary_node);
            }

            self.summary_nodes.insert(node_id.clone(), summary_node);
            self.active_root_summaries.push(node_id.clone());
            new_level1_nodes.push(node_id);
        }

        // Check if Level 2 Meta-Summarization should be triggered across Level 1 summaries
        let mut new_meta_nodes = Vec::new();
        if self.active_root_summaries.len() >= self.config.meta_summary_fanout {
            let meta_node = self.synthesize_meta_summary();
            let meta_id = meta_node.node_id.clone();

            if let Some(pool) = &self.pool {
                let _ = self.persist_summary_to_sqlite(pool, &meta_node);
            }

            self.summary_nodes.insert(meta_id.clone(), meta_node);
            new_meta_nodes.push(meta_id);
        }

        let final_tokens = self.total_working_tokens();
        let tokens_saved = initial_tokens.saturating_sub(final_tokens);

        CondensationReport {
            triggered: true,
            initial_working_tokens: initial_tokens,
            final_working_tokens: final_tokens,
            tokens_saved,
            new_level1_summaries: new_level1_nodes,
            new_meta_summaries: new_meta_nodes,
            condensed_turn_count: total_condensed_turns,
            remaining_active_turns: self.active_turns.len(),
        }
    }

    /// Synthesize a structured Level 1 Chunk Summary from a slice of turns.
    fn synthesize_chunk_summary(&self, turns: &[DialogueTurn]) -> SummaryNode {
        let node_id = Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let source_turn_ids: Vec<String> = turns.iter().map(|t| t.turn_id.clone()).collect();

        // Extract key entities
        let mut entities_set = HashSet::new();
        for turn in turns {
            for mat in ENTITY_FINDER_REGEX.find_iter(&turn.content) {
                let val = mat.as_str().trim();
                if val.len() >= 3 && !val.starts_with("The ") {
                    entities_set.insert(val.to_string());
                }
            }
        }
        let mut key_entities: Vec<String> = entities_set.into_iter().collect();
        key_entities.sort();

        // Formulate structured abstract
        let mut bullet_points = Vec::new();
        for (idx, turn) in turns.iter().enumerate() {
            let snippet = turn.content.lines().next().unwrap_or(&turn.content);
            let truncated = if snippet.len() > 100 {
                format!("{}...", &snippet[..97])
            } else {
                snippet.to_string()
            };
            bullet_points.push(format!("- [{}] {}: {}", idx + 1, turn.role, truncated));
        }

        let title = if let Some(first_turn) = turns.first() {
            let words: Vec<&str> = first_turn.content.split_whitespace().take(6).collect();
            format!("Dialogue Segment: {}", words.join(" "))
        } else {
            "Dialogue Segment Summary".to_string()
        };

        let abstract_text = format!(
            "Summary of {} turns ({} to {}):\n{}\nKey topics: {}",
            turns.len(),
            turns.first().map_or("", |t| t.turn_id.as_str()),
            turns.last().map_or("", |t| t.turn_id.as_str()),
            bullet_points.join("\n"),
            if key_entities.is_empty() {
                "general discourse".to_string()
            } else {
                key_entities.join(", ")
            }
        );

        let token_count = DialogueTurn::estimate_tokens(&abstract_text);
        let merkle_hash = SummaryNode::compute_merkle_hash(1, &title, &abstract_text, &source_turn_ids, &[]);

        SummaryNode {
            node_id,
            session_id: self.session_id.clone(),
            level: 1,
            title,
            abstract_text,
            key_entities,
            source_turn_ids,
            child_node_ids: Vec::new(),
            merkle_hash,
            token_count,
            created_at: now,
            vector_embedding: None,
        }
    }

    /// Synthesize a Level 2 Meta-Summary by clustering Level 1 summary nodes.
    fn synthesize_meta_summary(&mut self) -> SummaryNode {
        let node_id = Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let child_node_ids: Vec<String> = self.active_root_summaries.clone();
        let mut source_turn_ids = Vec::new();
        let mut all_entities = HashSet::new();
        let mut child_bullets = Vec::new();

        for child_id in &child_node_ids {
            if let Some(child) = self.summary_nodes.get(child_id) {
                source_turn_ids.extend(child.source_turn_ids.clone());
                for ent in &child.key_entities {
                    all_entities.insert(ent.clone());
                }
                child_bullets.push(format!("* [Child #{}] {}: {}", child.level, child.title, child.abstract_text.lines().next().unwrap_or("")));
            }
        }

        source_turn_ids.sort();
        source_turn_ids.dedup();

        let mut key_entities: Vec<String> = all_entities.into_iter().collect();
        key_entities.sort();

        let title = format!("Meta-Summary: Phase Overview ({} chunks)", child_node_ids.len());
        let abstract_text = format!(
            "Comprehensive meta-summary condensing {} child episodes covering {} dialogue turns.\nTopical entities: {}\nChild Highlights:\n{}",
            child_node_ids.len(),
            source_turn_ids.len(),
            key_entities.join(", "),
            child_bullets.join("\n")
        );

        let token_count = DialogueTurn::estimate_tokens(&abstract_text);
        let merkle_hash = SummaryNode::compute_merkle_hash(2, &title, &abstract_text, &source_turn_ids, &child_node_ids);

        // Replace the child summaries in active roots with this unified meta-summary
        self.active_root_summaries.clear();
        self.active_root_summaries.push(node_id.clone());

        SummaryNode {
            node_id,
            session_id: self.session_id.clone(),
            level: 2,
            title,
            abstract_text,
            key_entities,
            source_turn_ids,
            child_node_ids,
            merkle_hash,
            token_count,
            created_at: now,
            vector_embedding: None,
        }
    }

    /// Persist summary node into SQLite `episodes` and `episode_turns` tables.
    fn persist_summary_to_sqlite(&self, pool: &DatabasePool, node: &SummaryNode) -> Result<(), String> {
        let conn = pool.writer.get().map_err(|e| e.to_string())?;
        
        let tags_json = serde_json::to_string(&node.key_entities).unwrap_or_else(|_| "[]".to_string());
        let turn_count = node.source_turn_ids.len();

        conn.execute(
            "INSERT INTO episodes (
                episode_id, session_id, title, summary, turn_count,
                domain, category, tags, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(episode_id) DO UPDATE SET
                title = excluded.title,
                summary = excluded.summary,
                turn_count = excluded.turn_count,
                tags = excluded.tags,
                updated_at = excluded.updated_at",
            rusqlite::params![
                node.node_id,
                node.session_id,
                node.title,
                node.abstract_text,
                turn_count as i64,
                self.config.domain,
                self.config.category,
                tags_json,
                node.created_at,
                node.created_at,
            ],
        ).map_err(|e| format!("SQLite episode insert error: {e}"))?;

        // Insert turn lineage
        for (order, turn_id) in node.source_turn_ids.iter().enumerate() {
            conn.execute(
                "INSERT OR REPLACE INTO episode_turns (episode_id, turn_id, turn_order) VALUES (?1, ?2, ?3)",
                rusqlite::params![node.node_id, turn_id, order as i64],
            ).map_err(|e| format!("SQLite episode_turns insert error: {e}"))?;
        }

        Ok(())
    }

    /// Retrieve all underlying original leaf `turn_id`s by recursively traversing the summary tree.
    pub fn drill_down_turn_ids(&self, node_id: &str) -> Vec<String> {
        let mut turn_ids = Vec::new();
        if let Some(node) = self.summary_nodes.get(node_id) {
            turn_ids.extend(node.source_turn_ids.clone());
        }
        turn_ids.sort();
        turn_ids.dedup();
        turn_ids
    }

    /// Retrieve a summary node by its ID.
    pub fn get_summary(&self, node_id: &str) -> Option<&SummaryNode> {
        self.summary_nodes.get(node_id)
    }

    /// Retrieve all registered summary nodes.
    pub fn all_summaries(&self) -> Vec<&SummaryNode> {
        self.summary_nodes.values().collect()
    }

    /// Retrieve currently active root summary nodes in working context.
    pub fn active_root_summaries(&self) -> Vec<&SummaryNode> {
        self.active_root_summaries
            .iter()
            .filter_map(|id| self.summary_nodes.get(id))
            .collect()
    }

    /// Render the unified working prompt for the LLM.
    /// Combines static system prompt, `<context_memory>` with hierarchical summaries,
    /// and active recent dialogue turns.
    pub fn render_working_prompt(&self, system_prompt: &str) -> String {
        let mut output = String::new();

        // 1. Static System Prompt with XML demarcation
        if !system_prompt.is_empty() {
            output.push_str("<SYSTEM_CONTEXT>\n");
            output.push_str(system_prompt.trim());
            output.push_str("\n</SYSTEM_CONTEXT>\n\n");
        }

        // 2. Active Hierarchical Summaries
        let active_summaries = self.active_root_summaries();
        if !active_summaries.is_empty() {
            output.push_str("<context_memory>\n");
            for summary in active_summaries {
                output.push_str(&format!(
                    "[Summary L{} | {} | Turns: {}]\n{}\n\n",
                    summary.level,
                    summary.title,
                    summary.source_turn_ids.len(),
                    summary.abstract_text.trim()
                ));
            }
            output.push_str("</context_memory>\n\n");
        }

        // 3. Active Unconsolidated Turns
        for turn in &self.active_turns {
            output.push_str(&format!("<turn id=\"{}\" role=\"{}\">\n{}\n</turn>\n", turn.turn_id, turn.role, turn.content.trim()));
        }

        output
    }
}
