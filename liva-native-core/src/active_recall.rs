//! Active Recall (Nhịp 4: Spaced Retrieval) — Giao thức NEO.
//!
//! Khắc phục sự quên lãng ngắt quãng bằng cách can thiệp TRƯỚC khi gọi LLM:
//! - Chi phí 0 token: Đọc fact đã lưu từ SQLite, không phát sinh thêm lượt inference nào.
//! - Mặc định TẮT: Chỉ kích hoạt khi `LIVA_ENABLE_ACTIVE_RECALL` bật (1/true/yes).
//! - Bảo vệ dữ liệu: Fact được làm sạch qua `sanitize_untrusted` chống Prompt Injection.
//! - Lịch ôn tập Spaced Repetition: Nhớ đúng tăng `memory_strength` (giãn khoảng cách),
//!   không nhớ giảm `memory_strength` (rút ngắn khoảng cách).

use crate::crypto::EncryptionEngine;
use crate::db::{self, DatabasePool};
use crate::env_flag;
use crate::llm::persona::sanitize_untrusted;
use liva_storage::FactTrie;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum pending challenge time-to-live (10 minutes = 600 seconds).
pub const CHALLENGE_TTL_SECS: i64 = 600;

#[derive(Debug, Clone)]
pub struct PendingRecall {
    pub fact_key: String,
    pub expected_answer: String,
    pub asked_at_ts: i64,
}

#[derive(Debug, Clone)]
pub struct FactRecord {
    pub key: String,
    pub enc_value: String,
    pub memory_strength: f64,
    pub last_accessed_at: i64,
    pub access_count: i64,
}

#[derive(Debug, Clone)]
pub struct ActiveRecallConfig {
    pub enabled: bool,
    pub min_interval_secs: i64,
}

impl ActiveRecallConfig {
    pub fn from_env() -> Option<Self> {
        if !env_flag("LIVA_ENABLE_ACTIVE_RECALL", false) {
            return None;
        }

        let min_interval = std::env::var("LIVA_ACTIVE_RECALL_MIN_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(0);

        Some(Self {
            enabled: true,
            min_interval_secs: min_interval,
        })
    }
}

pub struct ActiveRecallManager {
    pending_challenges: Mutex<HashMap<String, PendingRecall>>,
    custom_config: Mutex<Option<ActiveRecallConfig>>,
    trie: RwLock<FactTrie>,
    facts: RwLock<HashMap<String, FactRecord>>,
    initialized: AtomicBool,
    cached_db_count: AtomicUsize,
}

impl Default for ActiveRecallManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Loại bỏ dấu tiếng Việt để so khớp ngữ nghĩa linh hoạt (có dấu lẫn không dấu).
pub fn strip_vietnamese_diacritics(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let mapped = match c {
            'a' | 'á' | 'à' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' | 'â' | 'ấ'
            | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' => 'a',
            'A' | 'Á' | 'À' | 'Ả' | 'Ã' | 'Ạ' | 'Ă' | 'Ắ' | 'Ằ' | 'Ẳ' | 'Ẵ' | 'Ặ' | 'Â' | 'Ấ'
            | 'Ầ' | 'Ẩ' | 'Ẫ' | 'Ậ' => 'a',
            'd' | 'đ' => 'd',
            'D' | 'Đ' => 'd',
            'e' | 'é' | 'è' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' => {
                'e'
            }
            'E' | 'É' | 'È' | 'Ẻ' | 'Ẽ' | 'Ẹ' | 'Ê' | 'Ế' | 'Ề' | 'Ể' | 'Ễ' | 'Ệ' => {
                'e'
            }
            'i' | 'í' | 'ì' | 'ỉ' | 'ĩ' | 'ị' => 'i',
            'I' | 'Í' | 'Ì' | 'Ỉ' | 'Ĩ' | 'Ị' => 'i',
            'o' | 'ó' | 'ò' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ớ'
            | 'ờ' | 'ở' | 'ỡ' | 'ợ' => 'o',
            'O' | 'Ó' | 'Ò' | 'Ỏ' | 'Õ' | 'Ọ' | 'Ô' | 'Ố' | 'Ồ' | 'Ổ' | 'Ỗ' | 'Ộ' | 'Ơ' | 'Ớ'
            | 'Ờ' | 'Ở' | 'Ỡ' | 'Ợ' => 'o',
            'u' | 'ú' | 'ù' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => {
                'u'
            }
            'U' | 'Ú' | 'Ù' | 'Ủ' | 'Ũ' | 'Ụ' | 'Ư' | 'Ứ' | 'Ừ' | 'Ử' | 'Ữ' | 'Ự' => {
                'u'
            }
            'y' | 'ý' | 'ỳ' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
            'Y' | 'Ý' | 'Ỳ' | 'Ỷ' | 'Ỹ' | 'Ỵ' => 'y',
            other => other,
        };
        out.push(mapped);
    }
    out
}

impl ActiveRecallManager {
    pub fn new() -> Self {
        Self {
            pending_challenges: Mutex::new(HashMap::new()),
            custom_config: Mutex::new(None),
            trie: RwLock::new(FactTrie::new()),
            facts: RwLock::new(HashMap::new()),
            initialized: AtomicBool::new(false),
            cached_db_count: AtomicUsize::new(0),
        }
    }

    /// Khởi tạo ActiveRecallManager với cấu hình cố định, độc lập với `std::env`.
    pub fn with_config(config: ActiveRecallConfig) -> Self {
        Self {
            pending_challenges: Mutex::new(HashMap::new()),
            custom_config: Mutex::new(Some(config)),
            trie: RwLock::new(FactTrie::new()),
            facts: RwLock::new(HashMap::new()),
            initialized: AtomicBool::new(false),
            cached_db_count: AtomicUsize::new(0),
        }
    }

    /// Cập nhật cấu hình runtime hoặc trong kiểm thử.
    pub fn set_config(&self, config: Option<ActiveRecallConfig>) {
        let mut lock = self.custom_config.lock().unwrap_or_else(|e| e.into_inner());
        *lock = config;
    }

    /// Read guard for in-memory FactTrie (sub-50µs prefix lookup).
    pub fn trie(&self) -> std::sync::RwLockReadGuard<'_, FactTrie> {
        self.trie.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Insert or update a fact directly in the in-memory FactTrie and cache.
    pub fn insert_fact_in_memory(&self, key: &str, record: FactRecord) {
        let norm_key = key.replace(['_', '-'], " ").to_lowercase();
        let norm_key_stripped = strip_vietnamese_diacritics(&norm_key);
        let key_lower = key.to_lowercase();
        let key_stripped = strip_vietnamese_diacritics(&key_lower);

        {
            let mut trie = self.trie.write().unwrap_or_else(|e| e.into_inner());
            trie.insert(&key_lower, key);
            trie.insert(&norm_key, key);
            trie.insert(&key_stripped, key);
            trie.insert(&norm_key_stripped, key);
            for part in norm_key_stripped.split_whitespace() {
                if part.len() >= 2 {
                    trie.insert(part, key);
                }
            }
            for part in norm_key.split_whitespace() {
                if part.len() >= 2 {
                    trie.insert(part, key);
                }
            }
        }
        {
            let mut facts = self.facts.write().unwrap_or_else(|e| e.into_inner());
            facts.insert(key.to_string(), record);
            self.cached_db_count.store(facts.len(), Ordering::Release);
        }
    }

    /// Refresh the in-memory FactTrie and facts index from SQLite WAL.
    pub fn refresh_from_db(&self, db_pool: &DatabasePool) -> Result<(), rusqlite::Error> {
        let facts_vec = db_pool.with_reader(|conn| {
            let mut stmt = conn.prepare(
                "SELECT key, value, memory_strength, last_accessed_at, access_count FROM facts",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(FactRecord {
                    key: row.get(0)?,
                    enc_value: row.get(1)?,
                    memory_strength: row.get(2)?,
                    last_accessed_at: row.get(3)?,
                    access_count: row.get(4)?,
                })
            })?;
            let mut list = Vec::new();
            for r in rows {
                list.push(r?);
            }
            Ok(list)
        })?;

        let mut trie = FactTrie::new();
        let mut facts_map = HashMap::with_capacity(facts_vec.len());

        for f in facts_vec {
            let norm_key = f.key.replace(['_', '-'], " ").to_lowercase();
            let norm_key_stripped = strip_vietnamese_diacritics(&norm_key);
            let key_lower = f.key.to_lowercase();
            let key_stripped = strip_vietnamese_diacritics(&key_lower);

            trie.insert(&key_lower, &f.key);
            trie.insert(&norm_key, &f.key);
            trie.insert(&key_stripped, &f.key);
            trie.insert(&norm_key_stripped, &f.key);

            for part in norm_key_stripped.split_whitespace() {
                if part.len() >= 2 {
                    trie.insert(part, &f.key);
                }
            }
            for part in norm_key.split_whitespace() {
                if part.len() >= 2 {
                    trie.insert(part, &f.key);
                }
            }

            facts_map.insert(f.key.clone(), f);
        }

        self.cached_db_count
            .store(facts_map.len(), Ordering::Release);
        *self.trie.write().unwrap_or_else(|e| e.into_inner()) = trie;
        *self.facts.write().unwrap_or_else(|e| e.into_inner()) = facts_map;
        self.initialized.store(true, Ordering::Release);

        Ok(())
    }

    /// Dọn dẹp các pending challenges đã hết hạn TTL (10 phút = 600 giây).
    /// Trả về số lượng thử thách đã bị loại bỏ.
    pub fn sweep_expired(&self, now: i64) -> usize {
        let mut lock = self
            .pending_challenges
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let initial_len = lock.len();
        lock.retain(|_, c| now.saturating_sub(c.asked_at_ts) < CHALLENGE_TTL_SECS);
        initial_len - lock.len()
    }

    /// Trả về số lượng pending challenges hiện tại (phục vụ metrics/tests).
    pub fn pending_count(&self) -> usize {
        let lock = self
            .pending_challenges
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        lock.len()
    }

    /// Kiểm tra xem Active Recall có đang được kích hoạt hay không.
    pub fn is_enabled(&self) -> bool {
        self.config().is_some()
    }

    /// Lấy cấu hình Active Recall hiện tại nếu bật (ưu tiên custom_config trước khi fallback sang std::env).
    pub fn config(&self) -> Option<ActiveRecallConfig> {
        let lock = self.custom_config.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cfg) = &*lock {
            return Some(cfg.clone());
        }
        ActiveRecallConfig::from_env()
    }

    /// Thử đánh chặn lượt hội thoại trước khi gọi LLM.
    ///
    /// Trả về `Some(câu_trả_lời)` nếu:
    /// 1. Người dùng đang trả lời một câu hỏi Active Recall trước đó (đánh giá đúng/sai).
    /// 2. Câu hỏi của người dùng khớp với một fact đã lưu đến hạn ôn tập (hỏi ngược lại).
    ///
    /// Trả về `None` nếu tính năng tắt hoặc không khớp, để luồng tiếp tục gọi LLM bình thường.
    pub fn try_intercept_turn(
        &self,
        user_text: &str,
        session_id: &str,
        db_pool: &DatabasePool,
        crypto: &EncryptionEngine,
    ) -> Option<String> {
        let config = self.config()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // 1. Quét sạch các challenge đã quá hạn TTL (10 phút) và kiểm tra pending challenge hiện tại
        let pending = {
            let mut lock = self
                .pending_challenges
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            lock.retain(|_, c| now.saturating_sub(c.asked_at_ts) < CHALLENGE_TTL_SECS);
            lock.remove(session_id)
        };

        if let Some(pending) = pending {
            return Some(self.evaluate_user_answer(user_text, &pending, now, db_pool, crypto));
        }

        // 2. Không có pending challenge: Kiểm tra xem câu hỏi của người dùng có khớp fact nào cần ôn không
        self.check_and_create_challenge(user_text, session_id, now, &config, db_pool, crypto)
    }

    fn evaluate_user_answer(
        &self,
        user_text: &str,
        pending: &PendingRecall,
        now: i64,
        db_pool: &DatabasePool,
        crypto: &EncryptionEngine,
    ) -> String {
        let user_clean = user_text.trim().to_lowercase();
        let user_stripped = strip_vietnamese_diacritics(&user_clean);
        let expected_clean = pending.expected_answer.trim().to_lowercase();
        let expected_stripped = strip_vietnamese_diacritics(&expected_clean);

        // Tín hiệu không nhớ / đầu hàng
        let negative_markers = [
            "quen",
            "khong nho",
            "k nho",
            "chiu",
            "khong biet",
            "k biet",
            "chua nho",
            "chiu roi",
            "quen roi",
            "chiu thua",
            "chiu a",
        ];
        let gave_up = negative_markers
            .iter()
            .any(|marker| user_stripped.contains(marker));

        let is_correct = if gave_up || expected_clean.is_empty() {
            false
        } else {
            user_clean.contains(&expected_clean)
                || user_stripped.contains(&expected_stripped)
                || (expected_clean.contains(&user_clean) && user_clean.len() >= 3)
                || (expected_stripped.contains(&user_stripped) && user_stripped.len() >= 3)
        };

        // Lấy fact hiện tại từ RAM cache để cập nhật memory_strength (không block luồng)
        let current_strength = {
            let facts_guard = self.facts.read().unwrap_or_else(|e| e.into_inner());
            if let Some(f) = facts_guard.get(&pending.fact_key) {
                f.memory_strength
            } else {
                db_pool
                    .with_reader(|r| match db::get_fact(r, crypto, &pending.fact_key) {
                        Ok(Some(f)) => Ok(f.memory_strength),
                        _ => Ok(1.0),
                    })
                    .unwrap_or(1.0)
            }
        };

        let (new_strength, reply) = if is_correct {
            (
                (current_strength * 1.5).min(10.0),
                format!(
                    "Chính xác! {}.",
                    sanitize_untrusted(&pending.expected_answer)
                ),
            )
        } else {
            (
                (current_strength * 0.8).max(1.0),
                format!(
                    "Đáp án là: {}.",
                    sanitize_untrusted(&pending.expected_answer)
                ),
            )
        };

        // Cập nhật RAM cache ngay lập tức
        {
            let mut facts_mut = self.facts.write().unwrap_or_else(|e| e.into_inner());
            if let Some(f) = facts_mut.get_mut(&pending.fact_key) {
                f.memory_strength = new_strength;
                f.last_accessed_at = now;
                f.access_count += 1;
            }
        }

        db_pool
            .writer_actor
            .update_fact_recall_stats(pending.fact_key.clone(), new_strength, now);

        reply
    }

    fn check_and_create_challenge(
        &self,
        user_text: &str,
        session_id: &str,
        now: i64,
        config: &ActiveRecallConfig,
        db_pool: &DatabasePool,
        crypto: &EncryptionEngine,
    ) -> Option<String> {
        let user_clean = user_text.trim().to_lowercase();
        if user_clean.is_empty() {
            return None;
        }

        // Đảm bảo nạp dữ liệu từ SQLite vào FactTrie lần đầu tiên
        if !self.initialized.load(Ordering::Acquire) {
            let _ = self.refresh_from_db(db_pool);
        }

        let user_stripped = strip_vietnamese_diacritics(&user_clean);

        // Tra cứu tiền tố / xâu con siêu tốc trong RAM bằng FactTrie (< 50µs SLA)
        let mut candidate_keys = {
            let trie = self.trie.read().unwrap_or_else(|e| e.into_inner());
            let mut keys = trie.search(&user_clean);
            for k in trie.search(&user_stripped) {
                if !keys.contains(&k) {
                    keys.push(k);
                }
            }
            if keys.is_empty() {
                for k in trie.search_prefix(&user_clean) {
                    if !keys.contains(&k) {
                        keys.push(k);
                    }
                }
                for k in trie.search_prefix(&user_stripped) {
                    if !keys.contains(&k) {
                        keys.push(k);
                    }
                }
            }
            keys
        };

        // Nếu không khớp trong RAM, kiểm tra xem DB có fact mới bổ sung không
        if candidate_keys.is_empty() {
            let db_count = db_pool
                .with_reader(|conn| {
                    conn.query_row("SELECT count(*) FROM facts", [], |r| r.get::<_, usize>(0))
                })
                .unwrap_or(0);

            if db_count != self.cached_db_count.load(Ordering::Relaxed) {
                let _ = self.refresh_from_db(db_pool);
                let trie = self.trie.read().unwrap_or_else(|e| e.into_inner());
                candidate_keys = trie.search(&user_clean);
                for k in trie.search(&user_stripped) {
                    if !candidate_keys.contains(&k) {
                        candidate_keys.push(k);
                    }
                }
                if candidate_keys.is_empty() {
                    for k in trie.search_prefix(&user_clean) {
                        if !candidate_keys.contains(&k) {
                            candidate_keys.push(k);
                        }
                    }
                    for k in trie.search_prefix(&user_stripped) {
                        if !candidate_keys.contains(&k) {
                            candidate_keys.push(k);
                        }
                    }
                }
            }
        }

        if candidate_keys.is_empty() {
            // Không khớp bất kỳ fact nào — thoát ngay mà không quét đĩa
            return None;
        }

        let facts_guard = self.facts.read().unwrap_or_else(|e| e.into_inner());
        let mut best_match = None;

        for key in candidate_keys {
            if let Some(fact) = facts_guard.get(&key) {
                // 1. Kiểm tra giãn cách spaced repetition
                let interval = (config.min_interval_secs as f64 * fact.memory_strength) as i64;
                if now - fact.last_accessed_at < interval {
                    continue;
                }

                // 2. So khớp từ khóa trước khi giải mã
                let norm_key = fact.key.replace(['_', '-'], " ").to_lowercase();
                let norm_key_stripped = strip_vietnamese_diacritics(&norm_key);
                let key_lower = fact.key.to_lowercase();
                let key_stripped = strip_vietnamese_diacritics(&key_lower);

                let matches_key = user_clean.contains(&key_lower)
                    || user_clean.contains(&norm_key)
                    || user_stripped.contains(&key_stripped)
                    || user_stripped.contains(&norm_key_stripped)
                    || (norm_key_stripped.len() >= 3
                        && norm_key_stripped
                            .split_whitespace()
                            .all(|part| part.len() >= 2 && user_stripped.contains(part)));

                if !matches_key {
                    continue;
                }

                // 3. Giải mã AES-256-GCM theo nhu cầu CHỈ cho fact ứng viên đã khớp
                let fr = crypto.read_fact(&fact.enc_value);
                if fr.is_locked() {
                    continue;
                }
                let plain_val = fr.into_value();
                if plain_val.trim().is_empty() {
                    continue;
                }

                best_match = Some((
                    fact.key.clone(),
                    plain_val,
                    fact.memory_strength,
                    fact.access_count,
                ));
                break;
            }
        }

        drop(facts_guard);

        let (matched_key, expected_answer, _, _) = best_match?;

        // Cập nhật thống kê truy xuất trong RAM
        {
            let mut facts_mut = self.facts.write().unwrap_or_else(|e| e.into_inner());
            if let Some(f) = facts_mut.get_mut(&matched_key) {
                f.last_accessed_at = now;
                f.access_count += 1;
            }
        }

        // Ghi nhận truy xuất trên DB (touch access)
        db_pool
            .writer_actor
            .touch_fact_access(matched_key.clone(), now);

        // Lưu pending challenge (đồng thời dọn sạch các challenge cũ đã quá hạn TTL)
        {
            let mut lock = self
                .pending_challenges
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            lock.retain(|_, c| now.saturating_sub(c.asked_at_ts) < CHALLENGE_TTL_SECS);
            lock.insert(
                session_id.to_string(),
                PendingRecall {
                    fact_key: matched_key.clone(),
                    expected_answer,
                    asked_at_ts: now,
                },
            );
        }

        let sanitized_key = sanitize_untrusted(&matched_key);
        let key_display = if matched_key.contains('<') {
            sanitized_key
        } else {
            matched_key.replace(['_', '-'], " ")
        };
        let question = format!(
            "Trước khi mình trả lời, bạn thử nhớ lại xem: {} là gì?",
            key_display
        );

        Some(question)
    }

    /// Xóa toàn bộ pending challenges (dùng cho test/reset).
    pub fn clear(&self) {
        let mut lock = self
            .pending_challenges
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        lock.clear();
        self.trie.write().unwrap_or_else(|e| e.into_inner()).clear();
        self.facts
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.initialized.store(false, Ordering::Release);
        self.cached_db_count.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_recall_ttl_sweep() {
        let manager = ActiveRecallManager::new();
        assert_eq!(manager.pending_count(), 0);

        let now = 100_000;
        {
            let mut lock = manager
                .pending_challenges
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            // Challenge 1: asked at now - 700s (expired: 700 > 600)
            lock.insert(
                "sess_old".to_string(),
                PendingRecall {
                    fact_key: "old_key".to_string(),
                    expected_answer: "old_ans".to_string(),
                    asked_at_ts: now - 700,
                },
            );
            // Challenge 2: asked at now - 100s (fresh: 100 < 600)
            lock.insert(
                "sess_fresh".to_string(),
                PendingRecall {
                    fact_key: "fresh_key".to_string(),
                    expected_answer: "fresh_ans".to_string(),
                    asked_at_ts: now - 100,
                },
            );
        }

        assert_eq!(manager.pending_count(), 2);
        manager.sweep_expired(now);
        assert_eq!(manager.pending_count(), 1);

        let lock = manager
            .pending_challenges
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert!(lock.contains_key("sess_fresh"));
        assert!(!lock.contains_key("sess_old"));
    }

    #[test]
    fn test_active_recall_fact_trie_in_memory_sla() {
        let manager = ActiveRecallManager::new();

        // Populate in-memory FactTrie
        for i in 0..100 {
            manager.insert_fact_in_memory(
                &format!("user_fact_{i}"),
                FactRecord {
                    key: format!("user_fact_{i}"),
                    enc_value: format!("encrypted_val_{i}"),
                    memory_strength: 1.0,
                    last_accessed_at: 0,
                    access_count: 0,
                },
            );
        }

        let start = std::time::Instant::now();
        let trie = manager.trie();
        let res = trie.search_prefix("user_fact_5");
        let elapsed = start.elapsed();

        assert!(!res.is_empty(), "Prefix search must find matches");
        assert!(
            elapsed < std::time::Duration::from_micros(50),
            "FactTrie prefix lookup must be sub-50µs SLA, took {:?}",
            elapsed
        );
    }
}
