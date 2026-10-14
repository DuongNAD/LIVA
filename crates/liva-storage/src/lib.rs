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

impl From<String> for StorageError {
    fn from(s: String) -> Self {
        Self::Actor(s)
    }
}

impl From<&str> for StorageError {
    fn from(s: &str) -> Self {
        Self::Actor(s.to_string())
    }
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
    /// Constructs a new DbActorHandle backed by the given command channel.
    pub fn new(tx: mpsc::Sender<DbWriteCommand>) -> Self {
        Self { tx }
    }

    /// Submits a write command to the DbActor.
    pub async fn send(&self, cmd: DbWriteCommand) -> Result<()> {
        self.tx
            .send(cmd)
            .await
            .map_err(|_| StorageError::Actor("Actor channel closed".into()))
    }

    /// Asynchronously wait for all queued writes to be committed to SQLite WAL.
    pub async fn flush(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(DbWriteCommand::Flush {
            result_tx: Some(tx),
        })
        .await?;
        rx.await
            .map_err(|_| StorageError::Actor("Response dropped".to_string()))?
            .map_err(StorageError::Actor)
    }

    /// Submits an arbitrary write operation and awaits its execution.
    pub async fn execute<F>(&self, op: F) -> Result<()>
    where
        F: FnOnce(&Connection) -> std::result::Result<(), String> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.send(DbWriteCommand::Execute {
            op: Box::new(op),
            result_tx: Some(tx),
        })
        .await?;
        rx.await
            .map_err(|_| StorageError::Actor("Response dropped".to_string()))?
            .map_err(StorageError::Actor)
    }

    /// Asynchronously triggers a passive WAL checkpoint.
    pub async fn checkpoint_wal(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(DbWriteCommand::CheckpointWal {
            result_tx: Some(tx),
        })
        .await?;
        rx.await
            .map_err(|_| StorageError::Actor("Response dropped".to_string()))?
            .map_err(StorageError::Actor)
    }
}

pub mod pragmas;
pub mod sqlite_vec;

pub use sqlite_vec::{register_connection_sqlite_vec, register_sqlite_vec, sqlite3_vec_init};

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
                Ok(c) => persistent_conn.insert(c),
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

    let mut iter = cmds.into_iter();
    while let Some(cmd) = iter.next() {
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
                    for remaining in iter {
                        reject_cmd(remaining, &rb_msg);
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

/// A registered challenge associated with a fact key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactChallenge {
    pub fact_key: String,
    pub question: Option<String>,
    pub expected_answer: String,
}

/// Manages active recall sessions using the Trie for fast lookups.
pub struct ActiveRecallManager {
    trie: FactTrie,
    challenges: HashMap<String, FactChallenge>,
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
            challenges: HashMap::new(),
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

    /// Insert a fact key mapping directly into the internal Trie and register default challenge.
    pub fn insert_fact(&mut self, key: &str, fact_key: &str) {
        self.trie.insert(key, fact_key);
        self.challenges
            .entry(fact_key.to_string())
            .or_insert_with(|| FactChallenge {
                fact_key: fact_key.to_string(),
                question: None,
                expected_answer: fact_key.to_string(),
            });
    }

    /// Registers a rich active recall challenge with an explicit question and expected answer.
    pub fn register_challenge(
        &mut self,
        trigger: &str,
        fact_key: &str,
        question: Option<String>,
        expected_answer: &str,
    ) {
        self.trie.insert(trigger, fact_key);
        self.challenges.insert(
            fact_key.to_string(),
            FactChallenge {
                fact_key: fact_key.to_string(),
                question,
                expected_answer: expected_answer.to_string(),
            },
        );
    }

    /// Update the Trie with the latest facts from an iterator of (key, fact_key) pairs.
    pub fn refresh_trie<I, S1, S2>(&mut self, facts: I)
    where
        I: IntoIterator<Item = (S1, S2)>,
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        for (k, fk) in facts {
            self.insert_fact(k.as_ref(), fk.as_ref());
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
            let norm_expected = challenge.expected_answer.trim().to_lowercase();
            // Critical safeguard: empty expected_answer must never trigger a match.
            if norm_expected.is_empty() {
                return None;
            }

            let norm_user = user_text.trim().to_lowercase();
            let is_correct = norm_user.contains(&norm_expected)
                || norm_expected
                    .split_whitespace()
                    .all(|word| norm_user.contains(word));

            let reply = if is_correct {
                format!("Chính xác! Đáp án là: {}.", challenge.expected_answer)
            } else {
                format!(
                    "Chưa chính xác. Đáp án đúng là: {}.",
                    challenge.expected_answer
                )
            };
            return Some(reply);
        }

        // 2. Fast-match user_text against Trie index
        let matches = self.trie.search(user_text);
        if let Some(first_match) = matches.into_iter().next() {
            let (question, expected_answer) = if let Some(ch) = self.challenges.get(&first_match) {
                let q = ch.question.clone().unwrap_or_else(|| {
                    format!(
                        "Trước khi trả lời, bạn thử nhớ lại xem: {} là gì?",
                        first_match
                    )
                });
                (q, ch.expected_answer.clone())
            } else {
                let q = format!(
                    "Trước khi trả lời, bạn thử nhớ lại xem: {} là gì?",
                    first_match
                );
                (q, first_match.clone())
            };

            // Only issue a challenge if the expected answer is non-empty
            if expected_answer.trim().is_empty() {
                return None;
            }

            let now_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            pending.insert(
                session_id.to_string(),
                PendingRecall {
                    fact_key: first_match,
                    expected_answer,
                    asked_at_ts: now_ts,
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

    #[test]
    fn test_active_recall_manager_custom_challenge_and_answers() {
        let mut arm = ActiveRecallManager::new();
        arm.register_challenge(
            "phở",
            "fav_food",
            Some("Món ăn yêu thích của bạn là gì?".to_string()),
            "Phở bò",
        );

        // Turn 1: keyword triggers question challenge
        let q = arm.try_intercept_turn("Trưa nay mình đi ăn phở nhé", "sess_food");
        assert_eq!(q.as_deref(), Some("Món ăn yêu thích của bạn là gì?"));

        // Turn 2: User responds correctly
        let correct_rep = arm.try_intercept_turn("Mình rất thích phở bò", "sess_food");
        assert_eq!(
            correct_rep.as_deref(),
            Some("Chính xác! Đáp án là: Phở bò.")
        );

        // Turn 3: Challenge resolved, no trigger words
        assert!(
            arm.try_intercept_turn("Thời tiết hôm nay đẹp nhỉ", "sess_food")
                .is_none()
        );

        // Turn 4: Trigger again
        let q2 = arm.try_intercept_turn("Phở quán này ngon ghê", "sess_food");
        assert_eq!(q2.as_deref(), Some("Món ăn yêu thích của bạn là gì?"));

        // Turn 5: User responds incorrectly
        let incorrect_rep = arm.try_intercept_turn("Chắc là bún đậu mắm tôm", "sess_food");
        assert_eq!(
            incorrect_rep.as_deref(),
            Some("Chưa chính xác. Đáp án đúng là: Phở bò.")
        );
    }

    #[test]
    fn test_active_recall_manager_empty_answer_never_triggers_false_positive() {
        let arm = ActiveRecallManager::new();
        // Manually inject a malformed pending challenge with empty answer
        {
            let mut pending = arm.pending_challenges.lock().unwrap();
            pending.insert(
                "sess_empty".to_string(),
                PendingRecall {
                    fact_key: "dummy".to_string(),
                    expected_answer: String::new(),
                    asked_at_ts: 0,
                },
            );
        }

        // Crucial test: turn should return None and NOT match with empty string
        let reply = arm.try_intercept_turn("Bất kỳ câu nói nào", "sess_empty");
        assert!(reply.is_none(), "Empty expected answer must never match");
    }

    #[test]
    fn test_process_transactional_batch_rollback_notifies_remaining() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch("CREATE TABLE test_ledger (id INTEGER PRIMARY KEY, note TEXT);")
            .expect("create table");

        let (tx1, rx1) = oneshot::channel();
        let (tx2, rx2) = oneshot::channel();
        let (tx3, rx3) = oneshot::channel();

        let cmd1 = DbWriteCommand::Execute {
            op: Box::new(|c| {
                c.execute(
                    "INSERT INTO test_ledger (id, note) VALUES (1, 'entry 1')",
                    [],
                )
                .map(|_| ())
                .map_err(|e| e.to_string())
            }),
            result_tx: Some(tx1),
        };

        let cmd2 = DbWriteCommand::Execute {
            op: Box::new(|_| Err("simulated constraint violation".to_string())),
            result_tx: Some(tx2),
        };

        let cmd3 = DbWriteCommand::Execute {
            op: Box::new(|c| {
                c.execute(
                    "INSERT INTO test_ledger (id, note) VALUES (3, 'entry 3')",
                    [],
                )
                .map(|_| ())
                .map_err(|e| e.to_string())
            }),
            result_tx: Some(tx3),
        };

        process_transactional_batch(&conn, vec![cmd1, cmd2, cmd3]);

        // 1. First command should be notified of rollback
        let res1 = rx1.blocking_recv().expect("rx1 should not be dropped");
        assert!(res1.is_err());
        assert!(res1.unwrap_err().contains("Transaction rolled back"));

        // 2. Failing command should receive its direct error
        let res2 = rx2.blocking_recv().expect("rx2 should not be dropped");
        assert_eq!(res2.unwrap_err(), "simulated constraint violation");

        // 3. Crucial: third command MUST receive rollback error and NOT be dropped!
        let res3 = rx3
            .blocking_recv()
            .expect("rx3 channel must not be dropped");
        assert!(res3.is_err());
        assert!(res3.unwrap_err().contains("Transaction rolled back"));

        // 4. Verify SQLite rollback was genuine: ledger must have 0 rows
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM test_ledger", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0, "Ledger must be empty due to transaction rollback");
    }

    #[test]
    fn test_storage_error_strongly_typed_conversions() {
        let err_str = StorageError::from("actor timeout");
        assert!(matches!(err_str, StorageError::Actor(ref s) if s == "actor timeout"));
        assert_eq!(
            format!("{err_str}"),
            "Actor communication error: actor timeout"
        );

        let err_from_str: StorageError = "channel failed".into();
        assert!(matches!(err_from_str, StorageError::Actor(ref s) if s == "channel failed"));
    }
}
