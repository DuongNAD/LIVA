use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type TokenId = u32;
pub type BlockId = usize;
pub const BLOCK_SIZE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PrefixCacheStats {
    pub total_lookups: u64,
    pub hits: u64,
    pub misses: u64,
    pub total_matched_tokens: u64,
    pub allocated_blocks: usize,
    pub max_blocks: usize,
    pub hit_ratio: f64,
}

#[derive(Debug)]
pub struct RadixNode {
    pub prefix: Vec<TokenId>,
    pub kv_block_id: Option<BlockId>,
    pub children: HashMap<TokenId, Arc<RwLock<RadixNode>>>,
    pub is_pinned: bool,
    pub ref_count: AtomicU32,
    pub last_accessed_ms: AtomicU64,
    pub prev_accessed_ms: AtomicU64,
}

impl RadixNode {
    pub fn new(prefix: Vec<TokenId>, kv_block_id: Option<BlockId>, is_pinned: bool) -> Self {
        let now = Self::current_time_ms();
        Self {
            prefix,
            kv_block_id,
            children: HashMap::new(),
            is_pinned,
            ref_count: AtomicU32::new(0),
            last_accessed_ms: AtomicU64::new(now),
            prev_accessed_ms: AtomicU64::new(now),
        }
    }

    fn current_time_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    pub fn record_access(&self) {
        let now = Self::current_time_ms();
        let prev = self.last_accessed_ms.swap(now, Ordering::Relaxed);
        self.prev_accessed_ms.store(prev, Ordering::Relaxed);
    }

    /// Calculate Two-Tier LRU-K (K=2) score for eviction. Lower score = evicted first.
    pub fn lru_k_score(&self) -> u64 {
        let t_last = self.last_accessed_ms.load(Ordering::Relaxed);
        let t_prev = self.prev_accessed_ms.load(Ordering::Relaxed);
        // Interval between the last two accesses
        let interval = t_last.saturating_sub(t_prev);
        // Score: t_last weighted with interval smoothness
        t_last.saturating_sub(interval / 2)
    }
}

/// Radix Tree Prefix Cache (L1 Working Memory)
/// Optimizes multi-turn KV cache reuse and prefix matching.
pub struct RadixPrefixCache {
    pub root: Arc<RwLock<RadixNode>>,
    pub max_blocks: usize,
    pub allocated_blocks: AtomicU32,
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub total_matched_tokens: AtomicU64,
}

impl RadixPrefixCache {
    pub fn new(max_blocks: usize) -> Self {
        Self {
            root: Arc::new(RwLock::new(RadixNode::new(Vec::new(), None, true))),
            max_blocks,
            allocated_blocks: AtomicU32::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            total_matched_tokens: AtomicU64::new(0),
        }
    }

    /// Match the longest common prefix of `tokens` in the Radix Tree.
    /// Returns (matched_tokens_count, list_of_reusable_kv_block_ids).
    pub async fn match_prefix(&self, tokens: &[TokenId]) -> (usize, Vec<BlockId>) {
        if tokens.is_empty() {
            return (0, Vec::new());
        }

        let mut matched_blocks = Vec::new();
        let mut matched_tokens = 0;
        let mut current_node = self.root.clone();

        loop {
            let node = current_node.read().await;
            node.record_access();

            let remaining_tokens = &tokens[matched_tokens..];
            if remaining_tokens.is_empty() {
                break;
            }

            let first_token = remaining_tokens[0];
            let maybe_child = node.children.get(&first_token).cloned();
            drop(node);

            if let Some(child_arc) = maybe_child {
                let child = child_arc.read().await;
                let child_prefix = &child.prefix;

                let mut common_len = 0;
                while common_len < child_prefix.len()
                    && common_len < remaining_tokens.len()
                    && child_prefix[common_len] == remaining_tokens[common_len]
                {
                    common_len += 1;
                }

                if common_len == child_prefix.len() {
                    // Full child prefix matched
                    matched_tokens += common_len;
                    if let Some(blk) = child.kv_block_id {
                        matched_blocks.push(blk);
                    }
                    drop(child);
                    current_node = child_arc;
                } else {
                    // Partial match inside edge - cannot reuse partial block
                    break;
                }
            } else {
                break;
            }
        }

        if matched_tokens > 0 {
            self.hits.fetch_add(1, Ordering::Relaxed);
            self.total_matched_tokens.fetch_add(matched_tokens as u64, Ordering::Relaxed);
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }

        (matched_tokens, matched_blocks)
    }

    /// Insert a token sequence into the Radix Tree and assign a new KV block.
    pub async fn insert_prefix(
        &self,
        tokens: &[TokenId],
        kv_block_id: BlockId,
        is_pinned: bool,
    ) {
        if tokens.is_empty() {
            return;
        }

        let mut current_node = self.root.clone();
        let mut token_offset = 0;

        while token_offset < tokens.len() {
            let mut node = current_node.write().await;
            node.record_access();
            let first_token = tokens[token_offset];

            if let Some(child_arc) = node.children.get(&first_token).cloned() {
                let mut child = child_arc.write().await;
                let remaining_tokens = &tokens[token_offset..];
                let common_len = child
                    .prefix
                    .iter()
                    .zip(remaining_tokens)
                    .take_while(|(a, b)| a == b)
                    .count();

                if common_len < child.prefix.len() {
                    // Split child node
                    let split_prefix = child.prefix[common_len..].to_vec();
                    let split_child = Arc::new(RwLock::new(RadixNode {
                        prefix: split_prefix,
                        kv_block_id: child.kv_block_id,
                        children: std::mem::take(&mut child.children),
                        is_pinned: child.is_pinned,
                        ref_count: AtomicU32::new(child.ref_count.load(Ordering::Relaxed)),
                        last_accessed_ms: AtomicU64::new(child.last_accessed_ms.load(Ordering::Relaxed)),
                        prev_accessed_ms: AtomicU64::new(child.prev_accessed_ms.load(Ordering::Relaxed)),
                    }));

                    child.prefix.truncate(common_len);
                    child.kv_block_id = None;
                    let split_first_token = split_child.read().await.prefix[0];
                    child.children.insert(split_first_token, split_child);
                }

                token_offset += common_len;

                if token_offset == tokens.len() {
                    // Reached the end of insertion sequence: update block id if absent
                    if child.kv_block_id.is_none() {
                        child.kv_block_id = Some(kv_block_id);
                        if is_pinned {
                            child.is_pinned = true;
                        }
                        self.allocated_blocks.fetch_add(1, Ordering::Relaxed);
                    }
                    break;
                }

                drop(child);
                drop(node);
                current_node = child_arc;
            } else {
                // Insert new leaf branch
                let new_node = Arc::new(RwLock::new(RadixNode::new(
                    tokens[token_offset..].to_vec(),
                    Some(kv_block_id),
                    is_pinned,
                )));
                node.children.insert(first_token, new_node);
                self.allocated_blocks.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }

    /// Pin a prefix to prevent it from ever being evicted (e.g. system prompt).
    pub async fn pin_prefix(&self, tokens: &[TokenId]) -> bool {
        let (matched_tokens, _) = self.match_prefix(tokens).await;
        if matched_tokens == tokens.len() {
            // Traverse and pin all nodes along path
            let mut current_node = self.root.clone();
            let mut offset = 0;
            while offset < tokens.len() {
                let node = current_node.read().await;
                let first_token = tokens[offset];
                if let Some(child_arc) = node.children.get(&first_token).cloned() {
                    let mut child = child_arc.write().await;
                    child.is_pinned = true;
                    offset += child.prefix.len();
                    drop(child);
                    drop(node);
                    current_node = child_arc;
                } else {
                    break;
                }
            }
            true
        } else {
            false
        }
    }

    /// Evict unpinned leaf nodes using LRU-K scores until `target_evictions` blocks are freed.
    pub async fn evict_lru(&self, target_evictions: usize) -> usize {
        let mut evicted_count = 0;

        for _ in 0..target_evictions {
            let mut best_candidate: Option<(Arc<RwLock<RadixNode>>, TokenId, u64)> = None;

            // Collect candidate removable leaves
            let root = self.root.read().await;
            for (&token, child_arc) in root.children.iter() {
                let child = child_arc.read().await;
                if !child.is_pinned && child.ref_count.load(Ordering::Relaxed) == 0 && child.children.is_empty() {
                    let score = child.lru_k_score();
                    if let Some((_, _, best_score)) = &best_candidate {
                        if score < *best_score {
                            best_candidate = Some((self.root.clone(), token, score));
                        }
                    } else {
                        best_candidate = Some((self.root.clone(), token, score));
                    }
                }
            }
            drop(root);

            if let Some((parent_arc, token, _)) = best_candidate {
                let mut parent = parent_arc.write().await;
                if let Some(removed) = parent.children.remove(&token) {
                    let rem_node = removed.read().await;
                    if rem_node.kv_block_id.is_some() {
                        self.allocated_blocks.fetch_sub(1, Ordering::Relaxed);
                        evicted_count += 1;
                    }
                }
            } else {
                // No more evictable leaf nodes
                break;
            }
        }

        evicted_count
    }

    /// Clear all unpinned nodes from the cache.
    pub async fn clear(&self) {
        let mut root = self.root.write().await;
        root.children.retain(|_, child| {
            let ch = child.try_read();
            if let Ok(c) = ch {
                c.is_pinned
            } else {
                true
            }
        });
        self.allocated_blocks.store(0, Ordering::Relaxed);
    }

    /// Calculate overall cache hit ratio.
    pub fn hit_ratio(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed) as f64;
        let misses = self.misses.load(Ordering::Relaxed) as f64;
        let total = hits + misses;
        if total > 0.0 {
            hits / total
        } else {
            0.0
        }
    }

    pub fn allocated_blocks(&self) -> usize {
        self.allocated_blocks.load(Ordering::Relaxed) as usize
    }

    pub fn max_blocks(&self) -> usize {
        self.max_blocks
    }

    pub fn stats(&self) -> PrefixCacheStats {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total_matched_tokens = self.total_matched_tokens.load(Ordering::Relaxed);
        let allocated_blocks = self.allocated_blocks();
        let max_blocks = self.max_blocks;
        let hit_ratio = self.hit_ratio();

        PrefixCacheStats {
            total_lookups: hits + misses,
            hits,
            misses,
            total_matched_tokens,
            allocated_blocks,
            max_blocks,
            hit_ratio,
        }
    }
}
