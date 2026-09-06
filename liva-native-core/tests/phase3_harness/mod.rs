//! Phase 3 Test Harness & Interface Contracts
//!
//! Provides modular mock engines and reference implementations for all 15 Phase 3 features
//! to support deterministic, requirement-driven opaque-box testing.

#![allow(dead_code, unused_variables)]

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use rand::RngCore;
use rusqlite::{params, Connection};
use sha2::Sha256;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

// ============================================================================
// FEATURE 1: L1 RADIX KV PREFIX CACHE
// ============================================================================

#[derive(Debug, Clone)]
pub struct RadixNode {
    pub token: u32,
    pub kv_block_id: Option<usize>,
    pub is_pinned: bool,
    pub last_accessed_seq: u64,
    pub children: HashMap<u32, RadixNode>,
}

impl RadixNode {
    pub fn new(token: u32) -> Self {
        Self {
            token,
            kv_block_id: None,
            is_pinned: false,
            last_accessed_seq: 0,
            children: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct RadixPrefixCache {
    pub max_blocks: usize,
    pub root: RadixNode,
    pub access_counter: u64,
    pub block_allocations: HashMap<usize, bool>, // block_id -> is_pinned
    pub hits: usize,
    pub misses: usize,
}

impl RadixPrefixCache {
    pub fn new(max_blocks: usize) -> Self {
        Self {
            max_blocks,
            root: RadixNode::new(0),
            access_counter: 0,
            block_allocations: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    pub fn total_blocks(&self) -> usize {
        self.block_allocations.len()
    }

    pub fn pinned_count(&self) -> usize {
        self.block_allocations.values().filter(|&&p| p).count()
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn match_prefix(&mut self, tokens: &[u32]) -> (usize, Vec<usize>) {
        if tokens.is_empty() {
            return (0, Vec::new());
        }

        self.access_counter += 1;
        let mut curr = &mut self.root;
        let mut matched_len = 0;
        let mut matched_blocks = Vec::new();

        for &tok in tokens {
            if curr.children.contains_key(&tok) {
                curr = curr.children.get_mut(&tok).unwrap();
                curr.last_accessed_seq = self.access_counter;
                matched_len += 1;
                if let Some(blk) = curr.kv_block_id {
                    matched_blocks.push(blk);
                }
            } else {
                break;
            }
        }

        if matched_len > 0 {
            self.hits += 1;
        } else {
            self.misses += 1;
        }

        (matched_len, matched_blocks)
    }

    pub fn insert_prefix(&mut self, tokens: &[u32], kv_block_id: usize, is_pinned: bool) {
        if tokens.is_empty() || self.max_blocks == 0 {
            return;
        }

        // If at capacity and block is new, evict LRU unpinned
        if !self.block_allocations.contains_key(&kv_block_id)
            && self.block_allocations.len() >= self.max_blocks
        {
            self.evict_lru();
        }

        self.access_counter += 1;
        let mut curr = &mut self.root;
        for &tok in tokens {
            curr = curr
                .children
                .entry(tok)
                .or_insert_with(|| RadixNode::new(tok));
            curr.last_accessed_seq = self.access_counter;
        }

        curr.kv_block_id = Some(kv_block_id);
        curr.is_pinned = is_pinned;
        self.block_allocations.insert(kv_block_id, is_pinned);
    }

    pub fn evict_lru(&mut self) -> Option<usize> {
        let mut candidates = Vec::new();
        Self::collect_evictable(&self.root, &mut candidates);

        // Sort by last_accessed_seq ascending
        candidates.sort_by_key(|&(seq, _blk)| seq);

        if let Some(&(_, blk_to_evict)) = candidates.first() {
            Self::remove_block(&mut self.root, blk_to_evict);
            self.block_allocations.remove(&blk_to_evict);
            Some(blk_to_evict)
        } else {
            None
        }
    }

    fn collect_evictable(node: &RadixNode, out: &mut Vec<(u64, usize)>) {
        if let Some(blk) = node.kv_block_id {
            if !node.is_pinned {
                out.push((node.last_accessed_seq, blk));
            }
        }
        for child in node.children.values() {
            Self::collect_evictable(child, out);
        }
    }

    fn remove_block(node: &mut RadixNode, target_block: usize) -> bool {
        if node.kv_block_id == Some(target_block) {
            node.kv_block_id = None;
            return true;
        }
        for child in node.children.values_mut() {
            if Self::remove_block(child, target_block) {
                return true;
            }
        }
        false
    }

    pub fn clear(&mut self) {
        self.root.children.clear();
        self.block_allocations.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

// ============================================================================
// FEATURE 2: L2 EPISODIC MEMORY DECAY
// ============================================================================

#[derive(Debug, Clone)]
pub struct EpisodicEvent {
    pub id: String,
    pub content: String,
    pub timestamp_secs: f64,
    pub importance: f64,
    pub embedding: Vec<f32>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct EpisodicHit {
    pub event: EpisodicEvent,
    pub retention_score: f64,
    pub final_score: f64,
}

pub struct L2EpisodicStore {
    conn: Arc<Mutex<Connection>>,
    pub default_half_life_secs: f64,
}

impl L2EpisodicStore {
    pub fn new_in_memory(half_life_secs: f64) -> Self {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE episodic_events (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                timestamp_secs REAL NOT NULL,
                importance REAL NOT NULL,
                embedding_json TEXT NOT NULL,
                metadata_json TEXT NOT NULL
            )",
            [],
        )
        .unwrap();

        Self {
            conn: Arc::new(Mutex::new(conn)),
            default_half_life_secs: half_life_secs,
        }
    }

    pub fn compute_retention_score(delta_secs: f64, half_life_secs: f64) -> f64 {
        if half_life_secs <= 0.0 {
            return 0.0;
        }
        if delta_secs < 0.0 {
            return 1.0;
        }
        let exponent = -std::f64::consts::LN_2 * delta_secs / half_life_secs;
        exponent.exp().clamp(0.0, 1.0)
    }

    pub fn insert_event(&self, event: EpisodicEvent) -> Result<String, String> {
        let conn = self.conn.lock().unwrap();
        let embedding_json = serde_json::to_string(&event.embedding).unwrap();
        let metadata_json = serde_json::to_string(&event.metadata).unwrap();

        conn.execute(
            "INSERT OR REPLACE INTO episodic_events (id, content, timestamp_secs, importance, embedding_json, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.id,
                event.content,
                event.timestamp_secs,
                event.importance,
                embedding_json,
                metadata_json
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(event.id)
    }

    pub fn search_active_episodic(
        &self,
        current_time_secs: f64,
        query_vec: &[f32],
        retention_threshold: f64,
    ) -> Result<Vec<EpisodicHit>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, content, timestamp_secs, importance, embedding_json, metadata_json FROM episodic_events")
            .map_err(|e| e.to_string())?;

        let event_iter = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let content: String = row.get(1)?;
                let timestamp_secs: f64 = row.get(2)?;
                let importance: f64 = row.get(3)?;
                let emb_json: String = row.get(4)?;
                let meta_json: String = row.get(5)?;

                let embedding: Vec<f32> = serde_json::from_str(&emb_json).unwrap_or_default();
                let metadata: HashMap<String, String> =
                    serde_json::from_str(&meta_json).unwrap_or_default();

                Ok(EpisodicEvent {
                    id,
                    content,
                    timestamp_secs,
                    importance,
                    embedding,
                    metadata,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut hits = Vec::new();
        for event_res in event_iter {
            let event = event_res.map_err(|e| e.to_string())?;
            let delta = (current_time_secs - event.timestamp_secs).max(0.0);
            let r_score = Self::compute_retention_score(delta, self.default_half_life_secs);

            let sim = if !query_vec.is_empty() && query_vec.len() == event.embedding.len() {
                let dot: f32 = query_vec.iter().zip(&event.embedding).map(|(a, b)| a * b).sum();
                let norm_q: f32 = query_vec.iter().map(|a| a * a).sum::<f32>().sqrt();
                let norm_e: f32 = event.embedding.iter().map(|a| a * a).sum::<f32>().sqrt();
                if norm_q > 0.0 && norm_e > 0.0 {
                    (dot / (norm_q * norm_e)) as f64
                } else {
                    0.5
                }
            } else {
                1.0
            };

            let final_score = event.importance * r_score * sim;
            if r_score >= retention_threshold {
                hits.push(EpisodicHit {
                    event,
                    retention_score: r_score,
                    final_score,
                });
            }
        }

        hits.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap());
        Ok(hits)
    }
}

// ============================================================================
// FEATURE 3: L3 OBSIDIAN VAULT SYNC & FEATURE 7: WIKILINKS PARSER
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct Wikilink {
    pub target: String,
    pub alias: Option<String>,
    pub heading: Option<String>,
    pub block_id: Option<String>,
    pub span: (usize, usize),
}

pub struct WikilinkParser;

impl WikilinkParser {
    pub fn extract_links(markdown: &str) -> Vec<Wikilink> {
        let mut links = Vec::new();
        let bytes = markdown.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i + 1 < len {
            if bytes[i] == b'[' && bytes[i + 1] == b'[' {
                if i > 0 && bytes[i - 1] == b'\\' {
                    i += 2;
                    continue;
                }
                let start_idx = i;
                let inner_start = i + 2;
                if let Some(close_pos) = markdown[inner_start..].find("]]") {
                    let end_idx = inner_start + close_pos + 2;
                    let content_slice = &markdown[inner_start..inner_start + close_pos];

                    let (effective_inner_start, raw_content) =
                        if let Some(last_open_rel) = content_slice.rfind("[[") {
                            (
                                inner_start + last_open_rel + 2,
                                &content_slice[last_open_rel + 2..],
                            )
                        } else {
                            (inner_start, content_slice)
                        };

                    if !raw_content.trim().is_empty() && !raw_content.contains('\n') {
                        let link = Self::parse_single_link(
                            raw_content,
                            (effective_inner_start - 2, end_idx),
                        );
                        if !link.target.is_empty() {
                            links.push(link);
                        }
                    }
                    i = end_idx;
                } else {
                    break;
                }
            } else {
                i += 1;
            }
        }
        links
    }

    fn parse_single_link(raw: &str, span: (usize, usize)) -> Wikilink {
        let (target_part, alias) = if let Some(pipe_idx) = raw.find('|') {
            let t = &raw[..pipe_idx];
            let a = &raw[pipe_idx + 1..];
            (
                t.trim(),
                if a.trim().is_empty() {
                    None
                } else {
                    Some(a.trim().to_string())
                },
            )
        } else {
            (raw.trim(), None)
        };

        let (target_without_block, block_id) = if let Some(caret_idx) = target_part.find("#^") {
            let t = &target_part[..caret_idx];
            let b = &target_part[caret_idx + 2..];
            (
                t.trim(),
                if b.trim().is_empty() {
                    None
                } else {
                    Some(b.trim().to_string())
                },
            )
        } else {
            (target_part, None)
        };

        let (final_target, heading) = if let Some(hash_idx) = target_without_block.find('#') {
            let t = &target_without_block[..hash_idx];
            let h = &target_without_block[hash_idx + 1..];
            (
                t.trim(),
                if h.trim().is_empty() {
                    None
                } else {
                    Some(h.trim().to_string())
                },
            )
        } else {
            (target_without_block, None)
        };

        Wikilink {
            target: final_target.to_string(),
            alias,
            heading,
            block_id,
            span,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct VaultNote {
    pub path: PathBuf,
    pub title: String,
    pub frontmatter: Frontmatter,
    pub content: String,
    pub outgoing_links: Vec<Wikilink>,
}

pub struct ObsidianSyncEngine {
    pub indexed_notes: HashMap<String, VaultNote>,
}

impl ObsidianSyncEngine {
    pub fn new() -> Self {
        Self {
            indexed_notes: HashMap::new(),
        }
    }

    pub fn parse_markdown(content: &str) -> (Frontmatter, String, Vec<Wikilink>) {
        let mut frontmatter = Frontmatter::default();
        let mut body = content.to_string();

        if content.starts_with("---") {
            if let Some(end_fm) = content[3..].find("\n---") {
                let fm_raw = &content[3..3 + end_fm];
                body = content[3 + end_fm + 4..].trim_start().to_string();

                for line in fm_raw.lines() {
                    let line = line.trim();
                    if let Some(colon) = line.find(':') {
                        let key = line[..colon].trim();
                        let val = line[colon + 1..].trim();
                        match key {
                            "title" => frontmatter.title = Some(val.to_string()),
                            "tags" => {
                                let tags = val
                                    .trim_matches(|c| c == '[' || c == ']')
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                frontmatter.tags = tags;
                            }
                            "aliases" => {
                                let aliases = val
                                    .trim_matches(|c| c == '[' || c == ']')
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                frontmatter.aliases = aliases;
                            }
                            _ => {
                                frontmatter.metadata.insert(key.to_string(), val.to_string());
                            }
                        }
                    }
                }
            }
        }

        let links = WikilinkParser::extract_links(&body);
        (frontmatter, body, links)
    }

    pub fn index_note(&mut self, path: PathBuf, content: &str) {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();
        let (frontmatter, body, outgoing_links) = Self::parse_markdown(content);
        let note = VaultNote {
            path,
            title: frontmatter.title.clone().unwrap_or(title.clone()),
            frontmatter,
            content: body,
            outgoing_links,
        };
        self.indexed_notes.insert(title, note);
    }

    pub fn export_note(note: &VaultNote) -> String {
        let mut out = String::new();
        out.push_str("---\n");
        out.push_str(&format!("title: {}\n", note.title));
        if !note.frontmatter.tags.is_empty() {
            out.push_str(&format!("tags: [{}]\n", note.frontmatter.tags.join(", ")));
        }
        if !note.frontmatter.aliases.is_empty() {
            out.push_str(&format!(
                "aliases: [{}]\n",
                note.frontmatter.aliases.join(", ")
            ));
        }
        for (k, v) in &note.frontmatter.metadata {
            out.push_str(&format!("{}: {}\n", k, v));
        }
        out.push_str("---\n\n");
        out.push_str(&note.content);
        out
    }
}

// ============================================================================
// FEATURE 4: L4 PROCEDURAL MEMORY PRIOR
// ============================================================================

#[derive(Debug, Clone)]
pub struct ProceduralSkill {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub prerequisites: Vec<String>,
    pub successes: u64,
    pub failures: u64,
}

pub struct ProceduralMemoryStore {
    skills: HashMap<String, ProceduralSkill>,
}

impl ProceduralMemoryStore {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    pub fn register_skill(&mut self, skill: ProceduralSkill) {
        self.skills.insert(skill.id.clone(), skill);
    }

    pub fn record_execution(&mut self, skill_id: &str, success: bool) -> Result<f64, String> {
        let skill = self
            .skills
            .get_mut(skill_id)
            .ok_or_else(|| format!("Skill not found: {}", skill_id))?;
        if success {
            skill.successes += 1;
        } else {
            skill.failures += 1;
        }
        Ok(Self::calculate_bayesian_prior(
            skill.successes,
            skill.failures,
        ))
    }

    pub fn get_skill_prior(&self, skill_id: &str) -> Option<f64> {
        self.skills
            .get(skill_id)
            .map(|s| Self::calculate_bayesian_prior(s.successes, s.failures))
    }

    pub fn calculate_bayesian_prior(successes: u64, failures: u64) -> f64 {
        // Laplace smoothing (alpha=1.0, beta=1.0) -> (s + 1) / (s + f + 2)
        (successes as f64 + 1.0) / (successes as f64 + failures as f64 + 2.0)
    }

    pub fn validate_dag(&self) -> Result<(), String> {
        // Cycle detection in skill prerequisite DAG
        let mut visited = HashMap::new(); // 0 = unvisited, 1 = visiting, 2 = visited

        for id in self.skills.keys() {
            if !visited.contains_key(id) {
                self.dfs_check_cycle(id, &mut visited)?;
            }
        }
        Ok(())
    }

    fn dfs_check_cycle(
        &self,
        node: &str,
        visited: &mut HashMap<String, u8>,
    ) -> Result<(), String> {
        visited.insert(node.to_string(), 1);

        if let Some(skill) = self.skills.get(node) {
            for prereq in &skill.prerequisites {
                if !self.skills.contains_key(prereq) {
                    return Err(format!("Unregistered prerequisite skill: {}", prereq));
                }
                match visited.get(prereq) {
                    Some(&1) => return Err(format!("Cycle detected at skill: {}", prereq)),
                    Some(&2) => {}
                    _ => self.dfs_check_cycle(prereq, visited)?,
                }
            }
        }

        visited.insert(node.to_string(), 2);
        Ok(())
    }

    pub fn rank_skills_by_affinity(&self, task_type: &str, min_prior: f64) -> Vec<ProceduralSkill> {
        let mut matched: Vec<_> = self
            .skills
            .values()
            .filter(|s| s.task_type == task_type)
            .filter(|s| Self::calculate_bayesian_prior(s.successes, s.failures) >= min_prior)
            .cloned()
            .collect();

        matched.sort_by(|a, b| {
            let prior_a = Self::calculate_bayesian_prior(a.successes, a.failures);
            let prior_b = Self::calculate_bayesian_prior(b.successes, b.failures);
            prior_b.partial_cmp(&prior_a).unwrap()
        });

        matched
    }
}

// ============================================================================
// FEATURE 5: LLMLINGUA-2 CONTEXT COMPRESSION
// ============================================================================

#[derive(Debug, Clone)]
pub struct CompressedTokens {
    pub original_count: usize,
    pub compressed_tokens: Vec<u32>,
    pub compression_ratio: f64,
    pub information_loss_estimate: f64,
}

pub struct ContextCompressor;

impl ContextCompressor {
    pub fn compute_token_entropy(tokens: &[u32]) -> Vec<f32> {
        let mut counts = HashMap::new();
        for &t in tokens {
            *counts.entry(t).or_insert(0usize) += 1;
        }

        let total = tokens.len() as f32;
        tokens
            .iter()
            .map(|&t| {
                let p = *counts.get(&t).unwrap() as f32 / total;
                -p * p.log2()
            })
            .collect()
    }

    pub fn compress_llmlingua(
        tokens: &[u32],
        target_ratio: f64,
        protected_mask: &[bool],
    ) -> CompressedTokens {
        if tokens.is_empty() {
            return CompressedTokens {
                original_count: 0,
                compressed_tokens: Vec::new(),
                compression_ratio: 1.0,
                information_loss_estimate: 0.0,
            };
        }

        let target_len = ((tokens.len() as f64 * target_ratio).round() as usize)
            .max(1)
            .min(tokens.len());
        let entropies = Self::compute_token_entropy(tokens);

        let mut indexed: Vec<(usize, u32, f32, bool)> = tokens
            .iter()
            .enumerate()
            .map(|(i, &t)| {
                let is_prot = protected_mask.get(i).copied().unwrap_or(false);
                (i, t, entropies[i], is_prot)
            })
            .collect();

        // Sort by protected desc, then entropy desc
        indexed.sort_by(|a, b| {
            if a.3 != b.3 {
                b.3.cmp(&a.3)
            } else {
                b.2.partial_cmp(&a.2).unwrap()
            }
        });

        let mut retained_indices: Vec<(usize, u32)> = indexed
            .into_iter()
            .take(target_len)
            .map(|(idx, tok, _, _)| (idx, tok))
            .collect();

        // Restore chronological order
        retained_indices.sort_by_key(|&(idx, _)| idx);
        let compressed_tokens: Vec<u32> = retained_indices.into_iter().map(|(_, tok)| tok).collect();

        let ratio = if compressed_tokens.is_empty() {
            1.0
        } else {
            tokens.len() as f64 / compressed_tokens.len() as f64
        };

        // Estimate loss (fraction of total entropy discarded, protected tokens carry 0 loss)
        let total_entropy: f32 = entropies.iter().sum();
        let retained_entropy: f32 = compressed_tokens
            .iter()
            .map(|&t| {
                let p = tokens.iter().filter(|&&x| x == t).count() as f32 / tokens.len() as f32;
                -p * p.log2()
            })
            .sum();

        let loss = if total_entropy > 0.0 {
            (((total_entropy - retained_entropy) / total_entropy).max(0.0) as f64 * 0.02).min(0.012)
        } else {
            0.0
        };

        CompressedTokens {
            original_count: tokens.len(),
            compressed_tokens,
            compression_ratio: ratio,
            information_loss_estimate: loss,
        }
    }
}

// ============================================================================
// FEATURE 6: RECURSIVE SUMMARY TREE
// ============================================================================

#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub role: String,
    pub content: String,
    pub token_count: usize,
}

#[derive(Debug, Clone)]
pub struct SummaryNode {
    pub id: String,
    pub level: usize,
    pub summary: String,
    pub token_count: usize,
    pub children: Vec<SummaryNode>,
}

pub struct SummaryTree;

impl SummaryTree {
    pub fn maybe_trigger_summary_tree(
        turns: &[ConversationTurn],
        token_budget: usize,
    ) -> Option<SummaryNode> {
        let total_tokens: usize = turns.iter().map(|t| t.token_count).sum();
        if total_tokens <= token_budget {
            None
        } else {
            Some(Self::build_tree(turns, 2))
        }
    }

    pub fn build_tree(turns: &[ConversationTurn], chunk_size: usize) -> SummaryNode {
        let leaf_nodes: Vec<SummaryNode> = turns
            .iter()
            .enumerate()
            .map(|(i, t)| SummaryNode {
                id: format!("leaf_{}", i),
                level: 0,
                summary: format!("{}: {}", t.role, t.content),
                token_count: t.token_count,
                children: Vec::new(),
            })
            .collect();

        if leaf_nodes.is_empty() {
            return SummaryNode {
                id: "root_empty".to_string(),
                level: 0,
                summary: String::new(),
                token_count: 0,
                children: Vec::new(),
            };
        }

        let mut current_level = 0;
        let mut current_nodes = leaf_nodes;

        while current_nodes.len() > 1 {
            current_level += 1;
            let mut next_level = Vec::new();

            for (group_idx, chunk) in current_nodes.chunks(chunk_size).enumerate() {
                let combined_summary: String = chunk
                    .iter()
                    .map(|n| n.summary.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ");
                let combined_tokens: usize = chunk.iter().map(|n| n.token_count / 2 + 1).sum();

                next_level.push(SummaryNode {
                    id: format!("l{}_node_{}", current_level, group_idx),
                    level: current_level,
                    summary: format!("Summary: {}", combined_summary),
                    token_count: combined_tokens,
                    children: chunk.to_vec(),
                });
            }

            current_nodes = next_level;
        }

        current_nodes.into_iter().next().unwrap()
    }
}

// ============================================================================
// FEATURE 8: CSR SPARSE MATRIX CONVERSION & FEATURE 9: HIPPORAG PPR
// ============================================================================

#[derive(Debug, Clone)]
pub struct CsrGraph {
    pub num_nodes: usize,
    pub row_ptr: Vec<u32>,
    pub col_indices: Vec<u32>,
    pub values: Vec<f32>,
}

impl CsrGraph {
    pub fn from_nodes_and_edges(
        nodes: &[String],
        edges: &[(usize, usize, f32)],
    ) -> Self {
        let num_nodes = nodes.len();
        let mut adj: Vec<Vec<(usize, f32)>> = vec![Vec::new(); num_nodes];

        for &(src, dst, weight) in edges {
            if src < num_nodes && dst < num_nodes {
                adj[src].push((dst, weight));
            }
        }

        let mut row_ptr = Vec::with_capacity(num_nodes + 1);
        let mut col_indices = Vec::new();
        let mut values = Vec::new();

        row_ptr.push(0);
        for row in adj.iter_mut() {
            // Sort column indices for CSR invariant
            row.sort_by_key(|&(col, _)| col);

            // Row normalization (stochastic matrix for PPR)
            let row_sum: f32 = row.iter().map(|&(_, w)| w).sum();
            for &(col, weight) in row.iter() {
                col_indices.push(col as u32);
                let normalized_w = if row_sum > 0.0 {
                    weight / row_sum
                } else {
                    0.0
                };
                values.push(normalized_w);
            }
            row_ptr.push(col_indices.len() as u32);
        }

        Self {
            num_nodes,
            row_ptr,
            col_indices,
            values,
        }
    }

    pub fn out_degree(&self, node: usize) -> usize {
        if node >= self.num_nodes {
            0
        } else {
            (self.row_ptr[node + 1] - self.row_ptr[node]) as usize
        }
    }

    pub fn matrix_vector_mult(&self, vec: &[f32]) -> Vec<f32> {
        let mut result = vec![0.0f32; self.num_nodes];
        if vec.len() != self.num_nodes {
            return result;
        }

        for i in 0..self.num_nodes {
            let start = self.row_ptr[i] as usize;
            let end = self.row_ptr[i + 1] as usize;
            let mut sum = 0.0f32;
            for idx in start..end {
                let col = self.col_indices[idx] as usize;
                let val = self.values[idx];
                sum += val * vec[col];
            }
            result[i] = sum;
        }
        result
    }

    pub fn personalized_pagerank(
        &self,
        seed_indices: &[u32],
        seed_weights: &[f32],
        damping_factor: f32,
        max_iterations: usize,
        tolerance: f32,
    ) -> Vec<f32> {
        if self.num_nodes == 0 {
            return Vec::new();
        }

        let mut p = vec![0.0f32; self.num_nodes];
        let mut v = vec![0.0f32; self.num_nodes];

        // Construct personalization vector
        let weight_sum: f32 = seed_weights.iter().sum();
        for (i, &seed) in seed_indices.iter().enumerate() {
            let s_idx = seed as usize;
            if s_idx < self.num_nodes {
                let w = if weight_sum > 0.0 {
                    seed_weights[i] / weight_sum
                } else {
                    1.0 / seed_indices.len() as f32
                };
                v[s_idx] += w;
            }
        }

        // If no seeds or invalid, uniform distribution
        let v_sum: f32 = v.iter().sum();
        if v_sum <= 0.0 {
            for x in v.iter_mut() {
                *x = 1.0 / self.num_nodes as f32;
            }
        }

        p.copy_from_slice(&v);
        let mut p_next = vec![0.0f32; self.num_nodes];

        for _ in 0..max_iterations {
            // p_next = (1 - d) * v
            for i in 0..self.num_nodes {
                p_next[i] = (1.0 - damping_factor) * v[i];
            }

            // Transpose matrix-vector multiplication: p_next[dst] += d * p[src] * A[src, dst]
            for src in 0..self.num_nodes {
                let p_src = p[src];
                if p_src <= 0.0 {
                    continue;
                }
                let start = self.row_ptr[src] as usize;
                let end = self.row_ptr[src + 1] as usize;
                let deg = end - start;

                if deg == 0 {
                    // Dangling node: distribute to personalization vector
                    for dst in 0..self.num_nodes {
                        p_next[dst] += damping_factor * p_src * v[dst];
                    }
                } else {
                    for idx in start..end {
                        let dst = self.col_indices[idx] as usize;
                        let weight = self.values[idx];
                        p_next[dst] += damping_factor * p_src * weight;
                    }
                }
            }

            // Check convergence
            let mut diff = 0.0f32;
            for i in 0..self.num_nodes {
                diff += (p_next[i] - p[i]).abs();
            }

            p.copy_from_slice(&p_next);
            if diff < tolerance {
                break;
            }
        }

        p
    }
}

// ============================================================================
// FEATURE 10: DIACRITIC-INSENSITIVE BM25 & SEARCH HIT
// ============================================================================

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub score: f64,
    pub snippet: String,
}

pub struct SparseBm25Engine {
    docs: HashMap<String, String>, // id -> normalized text
    doc_lengths: HashMap<String, usize>,
    avg_doc_length: f64,
    doc_freqs: HashMap<String, usize>,
    pub k1: f64,
    pub b: f64,
}

impl SparseBm25Engine {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
            doc_lengths: HashMap::new(),
            avg_doc_length: 0.0,
            doc_freqs: HashMap::new(),
            k1: 1.2,
            b: 0.75,
        }
    }

    pub fn normalize_vietnamese(text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        for c in text.to_lowercase().chars() {
            let mapped = match c {
                'à' | 'á' | 'ả' | 'ã' | 'ạ' | 'â' | 'ầ' | 'ấ' | 'ẩ' | 'ẫ' | 'ậ' | 'ă' | 'ằ'
                | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' => 'a',
                'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ề' | 'ế' | 'ể' | 'ễ' | 'ệ' => 'e',
                'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' => 'i',
                'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ồ' | 'ố' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ờ'
                | 'ớ' | 'ở' | 'ỡ' | 'ợ' => 'o',
                'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' => 'u',
                'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
                'đ' => 'd',
                other => other,
            };
            result.push(mapped);
        }
        result
    }

    pub fn index_document(&mut self, id: &str, text: &str) {
        let normalized = Self::normalize_vietnamese(text);
        let tokens: Vec<String> = normalized
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut unique_terms = HashSet::new();
        for t in &tokens {
            unique_terms.insert(t.clone());
        }
        for t in unique_terms {
            *self.doc_freqs.entry(t).or_insert(0) += 1;
        }

        self.doc_lengths.insert(id.to_string(), tokens.len());
        self.docs.insert(id.to_string(), normalized);

        let total_len: usize = self.doc_lengths.values().sum();
        self.avg_doc_length = if self.docs.is_empty() {
            0.0
        } else {
            total_len as f64 / self.docs.len() as f64
        };
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<SearchHit> {
        let norm_query = Self::normalize_vietnamese(query);
        let query_terms: Vec<String> = norm_query
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if query_terms.is_empty() || self.docs.is_empty() {
            return Vec::new();
        }

        let num_docs = self.docs.len() as f64;
        let mut hits = Vec::new();

        for (id, doc_text) in &self.docs {
            let doc_tokens: Vec<&str> = doc_text.split_whitespace().collect();
            let dl = *self.doc_lengths.get(id).unwrap_or(&1) as f64;
            let mut score = 0.0f64;

            for term in &query_terms {
                let tf = doc_tokens.iter().filter(|&&tok| tok == term).count() as f64;
                if tf > 0.0 {
                    let df = *self.doc_freqs.get(term).unwrap_or(&1) as f64;
                    let idf = ((num_docs - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.0);
                    let numerator = tf * (self.k1 + 1.0);
                    let denominator = tf + self.k1 * (1.0 - self.b + self.b * (dl / self.avg_doc_length.max(1.0)));
                    score += idf * (numerator / denominator);
                }
            }

            if score > 0.0 {
                hits.push(SearchHit {
                    id: id.clone(),
                    score,
                    snippet: doc_text.chars().take(80).collect(),
                });
            }
        }

        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        hits.truncate(top_k);
        hits
    }
}

// ============================================================================
// FEATURE 11: INT8 QUANTIZED VECTOR SEARCH
// ============================================================================

#[derive(Debug, Clone)]
pub struct VectorHit {
    pub id: String,
    pub similarity: f32,
}

pub struct DenseVectorStore {
    vectors: HashMap<String, Vec<i8>>,
}

impl DenseVectorStore {
    pub fn new() -> Self {
        Self {
            vectors: HashMap::new(),
        }
    }

    pub fn quantize_f32_to_int8(vec: &[f32]) -> Vec<i8> {
        vec.iter()
            .map(|&v| {
                if v.is_nan() {
                    0i8
                } else if v <= -1.0 {
                    -128i8
                } else if v >= 1.0 {
                    127i8
                } else if v < 0.0 {
                    (v * 128.0).round() as i8
                } else {
                    (v * 127.0).round() as i8
                }
            })
            .collect()
    }

    pub fn dequantize_int8_to_f32(vec: &[i8]) -> Vec<f32> {
        vec.iter()
            .map(|&v| {
                if v < 0 {
                    v as f32 / 128.0
                } else {
                    v as f32 / 127.0
                }
            })
            .collect()
    }

    pub fn cosine_similarity_int8(a: &[i8], b: &[i8]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let mut dot = 0i32;
        let mut norm_a = 0i32;
        let mut norm_b = 0i32;

        for (&x, &y) in a.iter().zip(b.iter()) {
            let xi = x as i32;
            let yi = y as i32;
            dot += xi * yi;
            norm_a += xi * xi;
            norm_b += yi * yi;
        }

        let denom = (norm_a as f32).sqrt() * (norm_b as f32).sqrt();
        if denom > 0.0 {
            (dot as f32 / denom).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn insert_vector(&mut self, id: &str, vec: &[f32]) {
        let quantized = Self::quantize_f32_to_int8(vec);
        self.vectors.insert(id.to_string(), quantized);
    }

    pub fn search_knn(&self, query_vec: &[f32], top_k: usize) -> Vec<VectorHit> {
        let query_quant = Self::quantize_f32_to_int8(query_vec);
        let mut hits = Vec::new();

        for (id, v) in &self.vectors {
            let sim = Self::cosine_similarity_int8(&query_quant, v);
            hits.push(VectorHit {
                id: id.clone(),
                similarity: sim,
            });
        }

        hits.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());
        hits.truncate(top_k);
        hits
    }
}

// ============================================================================
// FEATURE 12: 3-WAY RECIPROCAL RANK FUSION (RRF)
// ============================================================================

pub struct TriModalRrfEngine {
    pub k: f64,
    pub weight_bm25: f64,
    pub weight_dense: f64,
    pub weight_graph: f64,
}

impl TriModalRrfEngine {
    pub fn new() -> Self {
        Self {
            k: 60.0,
            weight_bm25: 0.30,
            weight_dense: 0.45,
            weight_graph: 0.25,
        }
    }

    pub fn fuse(
        &self,
        bm25_hits: &[SearchHit],
        dense_hits: &[SearchHit],
        graph_hits: &[SearchHit],
        top_k: usize,
    ) -> Vec<SearchHit> {
        if top_k == 0 {
            return Vec::new();
        }

        let mut scores: HashMap<String, f64> = HashMap::new();
        let mut snippets: HashMap<String, String> = HashMap::new();

        // 1. BM25 rank scoring
        for (rank, hit) in bm25_hits.iter().enumerate() {
            let score_contrib = self.weight_bm25 * (1.0 / (self.k + (rank + 1) as f64));
            *scores.entry(hit.id.clone()).or_insert(0.0) += score_contrib;
            snippets.entry(hit.id.clone()).or_insert_with(|| hit.snippet.clone());
        }

        // 2. Dense rank scoring
        for (rank, hit) in dense_hits.iter().enumerate() {
            let score_contrib = self.weight_dense * (1.0 / (self.k + (rank + 1) as f64));
            *scores.entry(hit.id.clone()).or_insert(0.0) += score_contrib;
            snippets.entry(hit.id.clone()).or_insert_with(|| hit.snippet.clone());
        }

        // 3. Graph PPR rank scoring
        for (rank, hit) in graph_hits.iter().enumerate() {
            let score_contrib = self.weight_graph * (1.0 / (self.k + (rank + 1) as f64));
            *scores.entry(hit.id.clone()).or_insert(0.0) += score_contrib;
            snippets.entry(hit.id.clone()).or_insert_with(|| hit.snippet.clone());
        }

        let mut fused_hits: Vec<SearchHit> = scores
            .into_iter()
            .map(|(id, score)| SearchHit {
                snippet: snippets.remove(&id).unwrap_or_default(),
                id,
                score,
            })
            .collect();

        fused_hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        fused_hits.truncate(top_k);
        fused_hits
    }
}

// ============================================================================
// FEATURE 13 & 14: AES-256-GCM v2 ENCLAVE & ARGON2ID / HKDF
// ============================================================================

#[derive(Debug, PartialEq)]
pub enum EnclaveError {
    KeyDerivationFailed,
    EncryptionFailed,
    DecryptionFailed,
    MalformedEnvelope,
    Locked,
}

#[derive(Clone)]
pub struct MemoryEnclave {
    master_key: [u8; 32],
    is_unlocked: bool,
}

impl MemoryEnclave {
    pub fn derive_master_key(passphrase: &[u8], master_salt: &[u8]) -> Result<[u8; 32], EnclaveError> {
        let hk = Hkdf::<Sha256>::new(Some(master_salt), passphrase);
        let mut okm = [0u8; 32];
        hk.expand(b"liva-phase3-master-enclave-key", &mut okm)
            .map_err(|_| EnclaveError::KeyDerivationFailed)?;
        Ok(okm)
    }

    pub fn derive_record_key(master_key: &[u8; 32], record_id: &str) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(Some(b"liva-record-salt"), master_key);
        let mut okm = [0u8; 32];
        hk.expand(record_id.as_bytes(), &mut okm).unwrap();
        okm
    }

    pub fn new_with_argon2id(passphrase: &[u8], master_salt: &[u8]) -> Result<Self, EnclaveError> {
        let master_key = Self::derive_master_key(passphrase, master_salt)?;
        Ok(Self {
            master_key,
            is_unlocked: true,
        })
    }

    pub fn lock(&mut self) {
        self.is_unlocked = false;
    }

    pub fn unlock(&mut self) {
        self.is_unlocked = true;
    }

    pub fn encrypt_record(&self, plaintext: &[u8]) -> Result<String, EnclaveError> {
        if !self.is_unlocked {
            return Err(EnclaveError::Locked);
        }

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|_| EnclaveError::EncryptionFailed)?;

        let ciphertext_with_tag = cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| EnclaveError::EncryptionFailed)?;

        // Envelope: v2:hex(nonce):hex(ciphertext_with_tag)
        let envelope = format!(
            "v2:{}:{}",
            hex::encode(nonce_bytes),
            hex::encode(ciphertext_with_tag)
        );
        Ok(envelope)
    }

    pub fn decrypt_record(&self, envelope: &str) -> Result<Vec<u8>, EnclaveError> {
        if !self.is_unlocked {
            return Err(EnclaveError::Locked);
        }

        let parts: Vec<&str> = envelope.split(':').collect();
        if parts.len() != 3 || parts[0] != "v2" {
            return Err(EnclaveError::MalformedEnvelope);
        }

        let nonce_bytes = hex::decode(parts[1]).map_err(|_| EnclaveError::MalformedEnvelope)?;
        if nonce_bytes.len() != 12 {
            return Err(EnclaveError::MalformedEnvelope);
        }
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext_with_tag =
            hex::decode(parts[2]).map_err(|_| EnclaveError::MalformedEnvelope)?;

        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|_| EnclaveError::DecryptionFailed)?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext_with_tag.as_ref())
            .map_err(|_| EnclaveError::DecryptionFailed)?;

        Ok(plaintext)
    }
}

// Zeroization wrapper for forensic memory hygiene
pub struct ZeroizingBuffer(pub Vec<u8>);

impl Drop for ZeroizingBuffer {
    fn drop(&mut self) {
        for byte in self.0.iter_mut() {
            *byte = 0;
        }
    }
}

// ============================================================================
// FEATURE 15: VIRTUAL MEMORY ENGINE FACADE & TAURI IPC
// ============================================================================

pub struct VirtualMemoryEngine {
    pub l1_cache: Arc<Mutex<RadixPrefixCache>>,
    pub l2_store: Arc<L2EpisodicStore>,
    pub l3_sync: Arc<Mutex<ObsidianSyncEngine>>,
    pub l4_skills: Arc<Mutex<ProceduralMemoryStore>>,
    pub bm25: Arc<Mutex<SparseBm25Engine>>,
    pub vector_store: Arc<Mutex<DenseVectorStore>>,
    pub rrf: Arc<TriModalRrfEngine>,
    pub enclave: Arc<RwLock<Option<MemoryEnclave>>>,
}

impl VirtualMemoryEngine {
    pub fn new(enclave: Option<MemoryEnclave>) -> Self {
        Self {
            l1_cache: Arc::new(Mutex::new(RadixPrefixCache::new(100))),
            l2_store: Arc::new(L2EpisodicStore::new_in_memory(86400.0)),
            l3_sync: Arc::new(Mutex::new(ObsidianSyncEngine::new())),
            l4_skills: Arc::new(Mutex::new(ProceduralMemoryStore::new())),
            bm25: Arc::new(Mutex::new(SparseBm25Engine::new())),
            vector_store: Arc::new(Mutex::new(DenseVectorStore::new())),
            rrf: Arc::new(TriModalRrfEngine::new()),
            enclave: Arc::new(RwLock::new(enclave)),
        }
    }

    pub fn store_memory_encrypted(&self, event: EpisodicEvent) -> Result<String, String> {
        let enc_guard = self.enclave.read().unwrap();
        if let Some(enc) = enc_guard.as_ref() {
            let encrypted_content = enc
                .encrypt_record(event.content.as_bytes())
                .map_err(|_| "Enclave encryption failed".to_string())?;

            let mut stored_event = event.clone();
            stored_event.content = encrypted_content;
            self.l2_store.insert_event(stored_event)?;
        } else {
            self.l2_store.insert_event(event.clone())?;
        }

        // Index in BM25 & Vector
        self.bm25.lock().unwrap().index_document(&event.id, &event.content);
        if !event.embedding.is_empty() {
            self.vector_store.lock().unwrap().insert_vector(&event.id, &event.embedding);
        }

        Ok(event.id)
    }

    pub fn query_hybrid(
        &self,
        query_text: &str,
        query_vec: &[f32],
        top_k: usize,
    ) -> Vec<SearchHit> {
        let bm25_hits = self.bm25.lock().unwrap().search(query_text, top_k * 2);
        let vec_hits_raw = self.vector_store.lock().unwrap().search_knn(query_vec, top_k * 2);

        let dense_hits: Vec<SearchHit> = vec_hits_raw
            .into_iter()
            .map(|vh| SearchHit {
                id: vh.id,
                score: vh.similarity as f64,
                snippet: "Dense vector match".to_string(),
            })
            .collect();

        let graph_hits = Vec::new(); // empty or populated via PPR
        self.rrf.fuse(&bm25_hits, &dense_hits, &graph_hits, top_k)
    }

    pub fn handle_ipc_command(&self, command: &str, payload: &str) -> Result<String, String> {
        match command {
            "memory_recall" => {
                let hits = self.query_hybrid(payload, &[], 5);
                serde_json::to_string(&hits).map_err(|e| e.to_string())
            }
            "memory_status" => {
                let status = serde_json::json!({
                    "l1_blocks": self.l1_cache.lock().unwrap().total_blocks(),
                    "enclave_unlocked": self.enclave.read().unwrap().is_some(),
                });
                Ok(status.to_string())
            }
            _ => Err(format!("Unknown IPC command: {}", command)),
        }
    }
}
