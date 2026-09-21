use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// The storage result type.
pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("Actor communication error: {0}")]
    Actor(String),
}

// -----------------------------------------------------------------------------
// MICRO-BATCHED DbActor
// -----------------------------------------------------------------------------

/// Type alias for write operation closures executed by the DbActor (fixes clippy::type_complexity).
pub type DbWriteOp = Box<dyn FnOnce(&Connection) -> std::result::Result<(), String> + Send>;

/// Defines the operations that can be sent to the DbActor.
pub enum DbWriteCommand {
    /// Execute an arbitrary write operation
    Execute {
        op: DbWriteOp,
        result_tx: Option<oneshot::Sender<std::result::Result<(), String>>>,
    },
    /// Flush all pending writes
    Flush {
        result_tx: Option<oneshot::Sender<std::result::Result<(), String>>>,
    },
    /// Trigger WAL checkpoint
    CheckpointWal {
        result_tx: Option<oneshot::Sender<std::result::Result<(), String>>>,
    },
    // Future commands for specific operations like PersistTurn, UpdateFact, etc.
}

/// A handle to interact with the micro-batched DbActor.
#[derive(Clone)]
pub struct DbActorHandle {
    tx: mpsc::Sender<DbWriteCommand>,
}

impl DbActorHandle {
    /// Submits a write command to the DbActor.
    pub async fn send(&self, cmd: DbWriteCommand) -> std::result::Result<(), String> {
        self.tx
            .send(cmd)
            .await
            .map_err(|_| "Actor channel closed".into())
    }

    /// Asynchronously wait for all queued writes to be committed to SQLite WAL.
    pub async fn flush(&self) -> std::result::Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.send(DbWriteCommand::Flush {
            result_tx: Some(tx),
        })
        .await?;
        rx.await.map_err(|_| "Response dropped".to_string())?
    }
}

pub mod pragmas;

/// The main loop for the micro-batched DbActor.
/// It collects commands from the channel and executes them in transactions
/// based on a maximum batch size or a time window limit.
pub async fn run_db_actor(
    pool: Pool<SqliteConnectionManager>,
    mut rx: mpsc::Receiver<DbWriteCommand>,
    max_batch_size: usize,
    max_batch_wait: Duration,
) {
    let mut persistent_conn: Option<r2d2::PooledConnection<SqliteConnectionManager>> = None;

    while let Some(first_cmd) = rx.recv().await {
        let conn_guard = match &mut persistent_conn {
            Some(c) => c,
            None => match pool.get() {
                Ok(c) => {
                    persistent_conn = Some(c);
                    persistent_conn.as_mut().unwrap()
                }
                Err(e) => {
                    let err_msg = format!("DB pool checkout error: {}", e);
                    tracing::error!("{}", err_msg);
                    reject_cmd(first_cmd, &err_msg);
                    while let Ok(next_cmd) = rx.try_recv() {
                        reject_cmd(next_cmd, &err_msg);
                    }
                    continue;
                }
            },
        };

        let mut batch = Vec::with_capacity(max_batch_size);
        let is_flush = matches!(
            first_cmd,
            DbWriteCommand::Flush { .. } | DbWriteCommand::CheckpointWal { .. }
        );
        batch.push(first_cmd);

        if !is_flush {
            let start = tokio::time::Instant::now();
            while batch.len() < max_batch_size {
                let elapsed = start.elapsed();
                if elapsed >= max_batch_wait {
                    break;
                }
                match tokio::time::timeout(max_batch_wait - elapsed, rx.recv()).await {
                    Ok(Some(next_cmd)) => {
                        let has_flush = matches!(
                            next_cmd,
                            DbWriteCommand::Flush { .. } | DbWriteCommand::CheckpointWal { .. }
                        );
                        batch.push(next_cmd);
                        if has_flush {
                            break;
                        }
                    }
                    Ok(None) => break, // Channel closed
                    Err(_) => break,   // Timeout
                }
            }
        }

        // Auto-deref fix: pass `conn_guard` directly (fixes clippy::explicit_auto_deref)
        process_write_batch(conn_guard, batch);
    }
}

fn reject_cmd(cmd: DbWriteCommand, err_msg: &str) {
    match cmd {
        DbWriteCommand::Execute {
            result_tx: Some(tx),
            ..
        } => {
            let _ = tx.send(Err(err_msg.to_string()));
        }
        DbWriteCommand::Flush {
            result_tx: Some(tx),
        } => {
            let _ = tx.send(Err(err_msg.to_string()));
        }
        DbWriteCommand::CheckpointWal {
            result_tx: Some(tx),
        } => {
            let _ = tx.send(Err(err_msg.to_string()));
        }
        _ => {}
    }
}

fn process_write_batch(conn: &Connection, cmds: Vec<DbWriteCommand>) {
    if cmds.is_empty() {
        return;
    }

    let mut tx_cmds = Vec::new();
    for cmd in cmds {
        if matches!(cmd, DbWriteCommand::CheckpointWal { .. }) {
            if !tx_cmds.is_empty() {
                process_transactional_batch(conn, std::mem::take(&mut tx_cmds));
            }
            if let DbWriteCommand::CheckpointWal { result_tx } = cmd {
                let res = conn
                    .execute_batch("PRAGMA wal_checkpoint(PASSIVE);")
                    .map_err(|e| e.to_string());
                if let Some(tx) = result_tx {
                    let _ = tx.send(res);
                }
            }
        } else {
            tx_cmds.push(cmd);
        }
    }

    if !tx_cmds.is_empty() {
        process_transactional_batch(conn, tx_cmds);
    }
}

fn process_transactional_batch(conn: &Connection, cmds: Vec<DbWriteCommand>) {
    if let Err(e) = conn.execute_batch("BEGIN IMMEDIATE;") {
        let err_msg = format!("Failed to BEGIN IMMEDIATE: {e}");
        tracing::error!("{err_msg}");
        for cmd in cmds {
            reject_cmd(cmd, &err_msg);
        }
        return;
    }

    let mut pending_notifications: Vec<oneshot::Sender<std::result::Result<(), String>>> =
        Vec::new();

    for cmd in cmds {
        match cmd {
            DbWriteCommand::Execute { op, result_tx } => {
                let res = op(conn);
                if let Err(err) = &res {
                    let _ = conn.execute_batch("ROLLBACK;");
                    let rb_msg = format!("Transaction rolled back due to error in batch: {err}");
                    tracing::warn!("{rb_msg}");
                    if let Some(tx) = result_tx {
                        let _ = tx.send(Err(err.clone()));
                    }
                    for prev_tx in pending_notifications {
                        let _ = prev_tx.send(Err(rb_msg.clone()));
                    }
                    return; // abort rest of batch
                } else if let Some(tx) = result_tx {
                    pending_notifications.push(tx);
                }
            }
            // Collapsed pattern match (fixes clippy::collapsible_match)
            DbWriteCommand::Flush {
                result_tx: Some(tx),
            } => {
                pending_notifications.push(tx);
            }
            _ => {}
        }
    }

    match conn.execute_batch("COMMIT;") {
        Ok(()) => {
            for tx in pending_notifications {
                let _ = tx.send(Ok(()));
            }
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            let err_msg = format!("Transaction COMMIT failed: {e}");
            tracing::error!("{err_msg}");
            for tx in pending_notifications {
                let _ = tx.send(Err(err_msg.clone()));
            }
        }
    }
}

// -----------------------------------------------------------------------------
// TRIE-BASED ACTIVE RECALL
// -----------------------------------------------------------------------------

/// Represents a pending recall challenge
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRecall {
    pub fact_key: String,
    pub expected_answer: String,
    pub asked_at_ts: i64,
}

/// A node in the Trie for fast prefix/substring matching of fact keys.
#[derive(Default, Debug, Clone)]
pub struct TrieNode {
    pub children: HashMap<char, TrieNode>,
    pub is_end_of_word: bool,
    // Store associated fact keys at this node
    pub fact_keys: Vec<String>,
}

impl TrieNode {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recursively collects all fact key string references stored in this node and its descendants.
    pub fn collect_all_fact_key_refs<'a>(&'a self, out: &mut Vec<&'a str>) {
        if self.is_end_of_word {
            for fk in &self.fact_keys {
                out.push(fk.as_str());
            }
        }
        match self.children.len() {
            0 => {}
            1 => {
                if let Some(child) = self.children.values().next() {
                    child.collect_all_fact_key_refs(out);
                }
            }
            _ => {
                for child in self.children.values() {
                    child.collect_all_fact_key_refs(out);
                }
            }
        }
    }

    /// Recursively collects all unique fact keys stored in this node and its descendants.
    pub fn collect_all_fact_keys(&self, out: &mut Vec<String>) {
        if self.children.is_empty() {
            if out.is_empty() {
                out.reserve(self.fact_keys.len());
                for k in &self.fact_keys {
                    out.push(k.clone());
                }
            } else {
                for k in &self.fact_keys {
                    if !out.contains(k) {
                        out.push(k.clone());
                    }
                }
            }
            return;
        }

        let mut refs = Vec::with_capacity(256);
        self.collect_all_fact_key_refs(&mut refs);
        if refs.len() > 1 {
            refs.sort_unstable();
            refs.dedup();
        }
        out.reserve(refs.len());
        if out.is_empty() {
            for r in refs {
                out.push(r.to_owned());
            }
        } else {
            for r in refs {
                if !out.iter().any(|existing| existing == r) {
                    out.push(r.to_owned());
                }
            }
        }
    }
}

/// A Trie structure optimized for matching user text against stored facts.
#[derive(Default, Debug, Clone)]
pub struct FactTrie {
    root: TrieNode,
}

impl FactTrie {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears all entries in the Trie.
    pub fn clear(&mut self) {
        self.root = TrieNode::default();
    }

    /// Returns the root node reference (for inspection or traversal).
    pub fn root(&self) -> &TrieNode {
        &self.root
    }

    /// Insert a fact key into the Trie.
    /// Characters of `key` are inserted in normalized lowercase, and `fact_key` is stored at the terminal node.
    pub fn insert(&mut self, key: &str, fact_key: &str) {
        let norm_key = key.trim().to_lowercase();
        if norm_key.is_empty() {
            return;
        }

        let mut curr = &mut self.root;
        for ch in norm_key.chars() {
            curr = curr.children.entry(ch).or_default();
        }
        curr.is_end_of_word = true;
        if !curr.fact_keys.iter().any(|k| k == fact_key) {
            curr.fact_keys.push(fact_key.to_string());
        }
    }

    /// Search the Trie for any fact keys that match substrings or words in the user text.
    /// Traverses the user text using sliding-window prefix matching against Trie nodes.
    pub fn search(&self, text: &str) -> Vec<String> {
        let norm_text = text.trim().to_lowercase();
        if norm_text.is_empty() {
            return vec![];
        }

        let chars: Vec<char> = norm_text.chars().collect();
        let mut matched = Vec::new();

        for start_idx in 0..chars.len() {
            let mut curr = &self.root;
            for &ch in &chars[start_idx..] {
                if let Some(next_node) = curr.children.get(&ch) {
                    curr = next_node;
                    if curr.is_end_of_word {
                        for fk in &curr.fact_keys {
                            if !matched.contains(fk) {
                                matched.push(fk.clone());
                            }
                        }
                    }
                } else {
                    break;
                }
            }
        }

        matched
    }

    /// Search all fact keys that match a given key prefix.
    /// Navigates to the prefix node and traverses the subtree to collect all fact keys.
    pub fn search_prefix(&self, prefix: &str) -> Vec<String> {
        let trimmed = prefix.trim();
        if trimmed.is_empty() {
            let mut all = Vec::new();
            self.root.collect_all_fact_keys(&mut all);
            return all;
        }

        if trimmed.is_ascii() {
            let mut curr = &self.root;
            for &b in trimmed.as_bytes() {
                let lc = b.to_ascii_lowercase() as char;
                if let Some(next_node) = curr.children.get(&lc) {
                    curr = next_node;
                } else {
                    return vec![];
                }
            }
            let mut results = Vec::new();
            curr.collect_all_fact_keys(&mut results);
            return results;
        }

        let mut curr = &self.root;
        for ch in trimmed.chars() {
            if ch.is_uppercase() {
                for lc in ch.to_lowercase() {
                    if let Some(next_node) = curr.children.get(&lc) {
                        curr = next_node;
                    } else {
                        return vec![];
                    }
                }
            } else if let Some(next_node) = curr.children.get(&ch) {
                curr = next_node;
            } else {
                return vec![];
            }
        }

        let mut results = Vec::new();
        curr.collect_all_fact_keys(&mut results);
        results
    }
}

/// Manages active recall sessions using the Trie for fast lookups.
pub struct ActiveRecallManager {
    trie: FactTrie,
    pending_challenges: std::sync::Mutex<HashMap<String, PendingRecall>>,
}

// Added Default implementation (fixes clippy::new_without_default)
impl Default for ActiveRecallManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ActiveRecallManager {
    pub fn new() -> Self {
        Self {
            trie: FactTrie::new(),
            pending_challenges: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Access reference to internal FactTrie.
    pub fn trie(&self) -> &FactTrie {
        &self.trie
    }

    /// Mutable reference to internal FactTrie.
    pub fn trie_mut(&mut self) -> &mut FactTrie {
        &mut self.trie
    }

    /// Insert a fact key mapping directly into the internal Trie.
    pub fn insert_fact(&mut self, key: &str, fact_key: &str) {
        self.trie.insert(key, fact_key);
    }

    /// Update the Trie with the latest facts from an iterator of (key, fact_key) pairs.
    pub fn refresh_trie<I, S1, S2>(&mut self, facts: I)
    where
        I: IntoIterator<Item = (S1, S2)>,
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        for (k, fk) in facts {
            self.trie.insert(k.as_ref(), fk.as_ref());
        }
    }

    /// Intercept a conversation turn to evaluate or initiate active recall.
    pub fn try_intercept_turn(&self, user_text: &str, session_id: &str) -> Option<String> {
        let mut pending = self
            .pending_challenges
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // 1. Check if there's a pending challenge for this session.
        if let Some(challenge) = pending.remove(session_id) {
            let is_correct = user_text
                .trim()
                .to_lowercase()
                .contains(&challenge.expected_answer.trim().to_lowercase());
            let reply = if is_correct {
                format!("Chính xác! {}.", challenge.expected_answer)
            } else {
                format!("Đáp án là: {}.", challenge.expected_answer)
            };
            return Some(reply);
        }

        // 2. Fast-match user_text against Trie index
        let matches = self.trie.search(user_text);
        if let Some(first_match) = matches.into_iter().next() {
            let question = format!(
                "Trước khi trả lời, bạn thử nhớ lại xem: {} là gì?",
                first_match
            );
            pending.insert(
                session_id.to_string(),
                PendingRecall {
                    fact_key: first_match,
                    expected_answer: String::new(),
                    asked_at_ts: 0,
                },
            );
            return Some(question);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fact_trie_insert_and_search() {
        let mut trie = FactTrie::new();
        trie.insert("thủ đô việt nam", "thu_do_viet_nam");
        trie.insert("hà nội", "thu_do_viet_nam");
        trie.insert("ngôn ngữ lập trình", "lang_rust");

        // Match substring in conversational text
        let results = trie.search("Bạn có biết thủ đô Việt Nam ở đâu không?");
        assert_eq!(results, vec!["thu_do_viet_nam"]);

        let results_lang = trie.search("Tôi đang học ngôn ngữ lập trình mới");
        assert_eq!(results_lang, vec!["lang_rust"]);

        // No match
        let no_match = trie.search("Thời tiết hôm nay thế nào?");
        assert!(no_match.is_empty());
    }

    #[test]
    fn test_fact_trie_prefix_traversal() {
        let mut trie = FactTrie::new();
        trie.insert("user_preference_theme", "theme_pref");
        trie.insert("user_preference_font", "font_pref");
        trie.insert("system_status", "sys_stat");

        let mut prefs = trie.search_prefix("user_preference_");
        prefs.sort();
        assert_eq!(prefs, vec!["font_pref", "theme_pref"]);

        let all = trie.search_prefix("");
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_active_recall_manager_trie_integration() {
        let mut arm = ActiveRecallManager::new();
        arm.insert_fact("món ăn yêu thích", "favorite_food");

        // Unintercepted turn when no match
        assert!(arm.try_intercept_turn("Xin chào LIVA", "sess_1").is_none());

        // Intercepted turn when matching fact
        let intercepted = arm.try_intercept_turn("Món ăn yêu thích của mình là gì?", "sess_1");
        assert!(intercepted.is_some());
        assert!(intercepted.unwrap().contains("favorite_food"));
    }
}
