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
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct PendingRecall {
    pub fact_key: String,
    pub expected_answer: String,
    pub asked_at_ts: i64,
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
        }
    }

    /// Kiểm tra xem Active Recall có đang được kích hoạt hay không.
    pub fn is_enabled(&self) -> bool {
        ActiveRecallConfig::from_env().is_some()
    }

    /// Lấy cấu hình Active Recall hiện tại nếu bật.
    pub fn config(&self) -> Option<ActiveRecallConfig> {
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

        // 1. Kiểm tra xem có pending challenge đang đợi người dùng trả lời không
        let pending = {
            let mut lock = self
                .pending_challenges
                .lock()
                .unwrap_or_else(|e| e.into_inner());
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

        // Lấy fact hiện tại để cập nhật memory_strength
        let current_strength = {
            let reader = db_pool.readers.get().ok();
            reader
                .and_then(|r| db::get_fact(&r, crypto, &pending.fact_key).ok().flatten())
                .map(|f| f.memory_strength)
                .unwrap_or(1.0)
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

        let reader = match db_pool.readers.get() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("ActiveRecall: cannot acquire read connection: {e}");
                return None;
            }
        };

        // Quét các fact hiện có
        let mut stmt = match reader.prepare(
            "SELECT key, value, memory_strength, last_accessed_at, access_count FROM facts",
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("ActiveRecall: prepare query failed: {e}");
                return None;
            }
        };

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .ok()?;

        // Tìm fact khớp với user_text: lọc theo access interval & key trước, chỉ giải mã AES-256-GCM theo nhu cầu
        let mut best_match = None;
        let user_stripped = strip_vietnamese_diacritics(&user_clean);

        for row in rows.flatten() {
            let (key, enc_val, memory_strength, last_accessed_at, access_count) = row;

            // 1. Kiểm tra giãn cách spaced repetition trước (access pattern filter)
            let interval = (config.min_interval_secs as f64 * memory_strength) as i64;
            if now - last_accessed_at < interval {
                continue;
            }

            // 2. So khớp từ khóa trước khi giải mã
            let norm_key = key.replace(['_', '-'], " ").to_lowercase();
            let norm_key_stripped = strip_vietnamese_diacritics(&norm_key);
            let key_lower = key.to_lowercase();
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

            // 3. Giải mã AES-256-GCM theo nhu cầu (on-demand) CHỈ cho fact ứng viên đã khớp
            let fr = crypto.read_fact(&enc_val);
            if fr.is_locked() {
                continue;
            }
            let plain_val = fr.into_value();
            if plain_val.trim().is_empty() {
                continue;
            }

            best_match = Some((key, plain_val, memory_strength, access_count));
            break;
        }

        let (matched_key, expected_answer, _, _) = best_match?;

        // Ghi nhận truy xuất trên DB (touch access)
        db_pool
            .writer_actor
            .touch_fact_access(matched_key.clone(), now);

        // Lưu pending challenge
        {
            let mut lock = self
                .pending_challenges
                .lock()
                .unwrap_or_else(|e| e.into_inner());
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
    }
}
