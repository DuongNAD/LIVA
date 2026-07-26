//! Miền bộ nhớ — facts (có mã hoá), sự kiện, vector, và tìm kiếm lai.
//!
//! Tách khỏi `handle_command` 26/07/2026 (B1 bước 6). Bảy nhánh, và là miền
//! **nặng nhất** trong sáu miền đã tách: 414 dòng, gần hết là SQL + lớp mã hoá.
//!
//! ## Vì sao dời NGUYÊN VĂN, không chẻ nhỏ như các miền trước
//!
//! Năm miền trước đủ nhỏ để vừa dời vừa gộp trùng lặp an toàn. Miền này thì
//! khác: nó chứa các bất biến bảo mật đã được vá qua nhiều vòng phản biện —
//! fail-closed khi giải mã hỏng, backup-trước-khi-ghi-đè, guard chiều 384 của
//! vector, và chốt cấm truy vấn `conversation_turn` khi không có owner scope.
//! Vừa dời vừa sửa ở đây là cách chắc chắn nhất để làm hỏng một trong số đó mà
//! test không bắt được ngay.
//!
//! Nên bước này **chỉ dời**. Việc chẻ nhỏ, nếu làm, là một commit riêng có thể
//! đọc diff mà không phải phân biệt "dòng này đổi chỗ" với "dòng này đổi nghĩa".
//!
//! ## `reset_memory` vẫn là một `Err` có chủ đích
//!
//! Xem chú thích tại nhánh: xoá sạch ký ức trải trên 17 bảng, không hoàn tác
//! được, và theo nguyên tắc của dự án thì phải thiết kế sao lưu + escrow trước.
//! Trả lỗi RÕ RÀNG còn hơn để UI quay spinner rồi im lặng hết giờ.

use crate::{AppState, db, parse_untrusted_memory_search_filter};
use serde_json::Value;
use std::sync::Arc;

const OWNED: &[&str] = &[
    "get_memory_data",
    "memory:set_fact",
    "memory:get_fact",
    "delete_memory_fact",
    "reset_memory",
    "memory:search_hybrid",
    "memory:upsert_vector",
];

/// Lệnh này có thuộc miền bộ nhớ không.
///
/// Không dùng `strip_prefix("memory:")`: ba lệnh `get_memory_data`,
/// `delete_memory_fact`, `reset_memory` là tên phẳng do UI đặt và đổi tên chúng
/// sẽ phá hợp đồng với client đang chạy.
pub fn owns(command: &str) -> bool {
    OWNED.contains(&command)
}

pub async fn handle(state: Arc<AppState>, command: &str, payload: Value) -> Result<Value, String> {
    match command {
    "get_memory_data" => {
        let results = tokio::task::spawn_blocking(move || {
            let conn = state
                .db
                .readers
                .get()
                .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

            // 1. Query l0
            let mut stmt_l0 = conn.prepare(
                "SELECT turnId, userMsg, aiReply, temporal_anchor FROM turn_layer_nodes ORDER BY temporal_anchor DESC LIMIT 100"
            ).map_err(|e| format!("Prepare l0 failed: {}", e))?;
            let rows_l0 = stmt_l0.query_map([], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "userMsg": row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    "aiReply": row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    "timestamp": row.get::<_, i64>(3)?,
                }))
            }).map_err(|e| format!("Query l0 failed: {}", e))?;
            let mut l0 = Vec::new();
            for r in rows_l0 {
                l0.push(r.map_err(|e| e.to_string())?);
            }

            // 2. Query facts
            let mut stmt_facts = conn.prepare(
                "SELECT key, value, createdAt, source, category, importance, memory_strength FROM facts"
            ).map_err(|e| format!("Prepare facts failed: {}", e))?;
            let rows_facts = stmt_facts.query_map([], |row| {
                let key: String = row.get(0)?;
                let enc_val: String = row.get(1)?;
                // FAIL-CLOSED có PHÂN LOẠI: locked=true ⇒ value LUÔN "" (không
                // rò ciphertext), UI hiện badge 🔒. Metadata (key/category…)
                // không mã hoá nên vẫn đầy đủ. `locked` là NGUỒN SỰ THẬT về
                // trạng thái khoá — KHÔNG suy từ value=="" (fact hợp lệ cũng rỗng).
                let fr = state.crypto.read_fact(&enc_val);
                let locked = fr.is_locked();
                Ok(serde_json::json!({
                    "key": key,
                    "value": fr.into_value(),
                    "locked": locked,
                    "createdAt": row.get::<_, String>(2)?,
                    "source": row.get::<_, String>(3)?,
                    "category": row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    "importance": row.get::<_, Option<f64>>(5)?.unwrap_or(0.5),
                    "memoryStrength": row.get::<_, Option<f64>>(6)?.unwrap_or(1.0),
                }))
            }).map_err(|e| format!("Query facts failed: {}", e))?;
            let mut facts = Vec::new();
            for r in rows_facts {
                facts.push(r.map_err(|e| e.to_string())?);
            }
            // KHÔNG rớt hàng locked (một fact hỏng không làm trắng viewer).
            // Đếm + WARN gộp để lỗi quan sát được ở tầng app.
            let locked_facts = facts
                .iter()
                .filter(|f| f.get("locked").and_then(|v| v.as_bool()).unwrap_or(false))
                .count();
            if locked_facts > 0 {
                tracing::warn!(
                    "get_memory_data: {locked_facts} ký ức KHÔNG giải mã được bằng khoá hiện tại \
                     (sai LIVA_ENCRYPTION_KEY?). Dữ liệu gốc còn nguyên; đặt đúng khoá để đọc lại."
                );
            }

            // 3. Query events
            let mut stmt_events = conn.prepare(
                "SELECT eventId, timestamp, phi_facts, phi_entities, psi_sentiment, psi_intent, psi_relational, rawUserMsg, rawAiReply, consolidation_status, domain, category, trace_keywords FROM events ORDER BY timestamp DESC LIMIT 100"
            ).map_err(|e| format!("Prepare events failed: {}", e))?;
            let rows_events = stmt_events.query_map([], |row| {
                let phi_facts_str: Option<String> = row.get(2)?;
                let phi_entities_str: Option<String> = row.get(3)?;
                let trace_kw_str: Option<String> = row.get(12)?;

                let phi_facts: serde_json::Value = phi_facts_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(serde_json::json!([]));
                let phi_entities: serde_json::Value = phi_entities_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(serde_json::json!([]));
                let trace_keywords: serde_json::Value = trace_kw_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(serde_json::json!([]));

                Ok(serde_json::json!({
                    "eventId": row.get::<_, String>(0)?,
                    "timestamp": row.get::<_, i64>(1)?,
                    "rawUserMsg": row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                    "rawAiReply": row.get::<_, Option<String>>(8)?.unwrap_or_default(),
                    "phi": {
                        "facts": phi_facts,
                        "entities": phi_entities
                    },
                    "psi": {
                        "sentiment": row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        "intent": row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        "relational": row.get::<_, Option<String>>(6)?.unwrap_or_default()
                    },
                    "consolidationStatus": row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "pending".to_string()),
                    "domain": row.get::<_, Option<String>>(10)?.unwrap_or_else(|| "General".to_string()),
                    "category": row.get::<_, Option<String>>(11)?.unwrap_or_else(|| "Uncategorized".to_string()),
                    "traceKeywords": trace_keywords,
                }))
            }).map_err(|e| format!("Query events failed: {}", e))?;
            let mut events = Vec::new();
            for r in rows_events {
                events.push(r.map_err(|e| e.to_string())?);
            }

            // 4. Query vectors
            let mut stmt_vectors = conn.prepare(
                "SELECT vec_id, type, content, domain, category, trace_keywords, created_at, source_event_ids FROM vectors_meta ORDER BY created_at DESC LIMIT 100"
            ).map_err(|e| format!("Prepare vectors failed: {}", e))?;
            let rows_vectors = stmt_vectors.query_map([], |row| {
                let trace_kw_str: Option<String> = row.get(5)?;
                let src_event_ids_str: Option<String> = row.get(7)?;

                let trace_keywords: serde_json::Value = trace_kw_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(serde_json::json!([]));
                let source_event_ids: serde_json::Value = src_event_ids_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(serde_json::json!([]));

                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "type": row.get::<_, String>(1)?,
                    "content": row.get::<_, String>(2)?,
                    "domain": row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "General".to_string()),
                    "category": row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "Uncategorized".to_string()),
                    "traceKeywords": trace_keywords,
                    "createdAt": row.get::<_, i64>(6)?,
                    "sourceEventIds": source_event_ids,
                }))
            }).map_err(|e| format!("Query vectors failed: {}", e))?;
            let mut vectors = Vec::new();
            for r in rows_vectors {
                vectors.push(r.map_err(|e| e.to_string())?);
            }

            Ok::<_, String>(serde_json::json!({
                "l0": l0,
                "l0_5": "",
                "facts": facts,
                "lockedFactsCount": locked_facts,
                "events": events,
                "vectors": vectors
            }))
        })
        .await
        .map_err(|e| format!("Blocking task panicked: {}", e))??;

        Ok(results)
    }
    "memory:set_fact" => {
        let fact: db::Fact = serde_json::from_value(payload)
            .map_err(|e| format!("Invalid fact payload: {}", e))?;

        tokio::task::spawn_blocking(move || {
            let conn = state
                .db
                .writer
                .get()
                .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

            db::set_fact(&conn, &state.crypto, &fact)
                .map_err(|e| format!("Failed to set fact: {}", e))?;
            Ok::<_, String>(())
        })
        .await
        .map_err(|e| format!("Blocking task panicked: {}", e))??;

        Ok(serde_json::json!({ "success": true }))
    }
    "memory:get_fact" => {
        let key = payload["key"]
            .as_str()
            .ok_or_else(|| "Missing 'key' in payload".to_string())?
            .to_string();

        let fact = tokio::task::spawn_blocking(move || {
            let conn = state
                .db
                .readers
                .get()
                .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

            db::get_fact(&conn, &state.crypto, &key)
                .map_err(|e| format!("Failed to get fact: {}", e))
        })
        .await
        .map_err(|e| format!("Blocking task panicked: {}", e))??;

        match fact {
            Some(f) => Ok(serde_json::to_value(f).unwrap()),
            None => Ok(serde_json::Value::Null),
        }
    }
    "delete_memory_fact" => {
        let key = payload["key"]
            .as_str()
            .ok_or_else(|| "Missing 'key' in payload".to_string())?
            .to_string();

        // FAIL-CLOSED (quyết định người dùng): backend TỪ CHỐI xoá một fact
        // KHÔNG giải mã được (locked) — không cho xoá thứ mình không đọc được
        // để biết nó là gì (có thể là dữ liệu quan trọng sau lưng khoá sai).
        // Guard đặt ở TẦNG LỆNH, không dựa vào confirm() của UI, nên caller
        // tự động (agent/pruning) cũng bị chặn.
        tokio::task::spawn_blocking(move || {
            use rusqlite::OptionalExtension;
            let conn = state
                .db
                .writer
                .get()
                .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

            let existing: Option<String> = conn
                .query_row("SELECT value FROM facts WHERE key = ?1", [&key], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(|e| format!("Query fact failed: {}", e))?;

            match existing {
                None => Ok(serde_json::json!({ "success": true, "note": "không tồn tại" })),
                Some(v) if state.crypto.read_fact(&v).is_locked() => Err(format!(
                    "Không xoá được ký ức '{key}' vì đang KHOÁ (không giải mã được bằng \
                     khoá hiện tại). Đặt đúng LIVA_ENCRYPTION_KEY để đọc/xoá, hoặc dữ liệu \
                     gốc vẫn còn nguyên."
                )),
                Some(_) => {
                    conn.execute("DELETE FROM facts WHERE key = ?1", [&key])
                        .map_err(|e| format!("Delete fact failed: {}", e))?;
                    Ok(serde_json::json!({ "success": true }))
                }
            }
        })
        .await
        .map_err(|e| format!("Blocking task panicked: {}", e))?
    }
    // Hai màn hình (SettingsView, SystemView) gửi lệnh này và chờ
    // `{success, error}`, nhưng lõi chưa từng có nhánh nào cho nó — nên UI
    // chỉ quay spinner rồi im lặng hết giờ. Trả lỗi RÕ RÀNG còn hơn im
    // lặng: người dùng biết nút không làm gì, thay vì tưởng đã xoá xong.
    //
    // Cố ý CHƯA cài thật: xoá sạch ký ức là thao tác không hoàn tác được,
    // trải trên 17 bảng (facts, vectors_meta, vec_idx, vectors_fts, events,
    // turn_layer_nodes, l3_*, agent_checkpoints, facts_locked_backup…) và
    // theo đúng nguyên tắc của dự án thì phải sao lưu + escrow trước. Đó là
    // một quyết định về mất dữ liệu, không phải một mục dọn dẹp.
    "reset_memory" => Err("`reset_memory` chưa được cài đặt ở lõi. Xoá từng ký ức bằng \
         `delete_memory_fact` (Dashboard → Memory). Xoá toàn bộ cần thiết kế sao lưu \
         trước — chưa làm, không hoàn tác được."
        .to_string()),
    "memory:search_hybrid" => {
        let query_text = payload["query_text"]
            .as_str()
            .ok_or_else(|| "Missing 'query_text'".to_string())?
            .to_string();

        // Đây là command thô, chưa có identity đáng tin cậy phía server. Domain
        // do client tự khai không thể được dùng làm ranh giới conversation memory.
        let filter = parse_untrusted_memory_search_filter(&payload)?;

        // `query_vector` là TUỲ CHỌN từ 22/07/2026. Bắt client tự cấp vector
        // 384 chiều là lý do trực tiếp khiến không client nào gọi được lệnh
        // này (UI không có embedder). Thiếu thì server tự embed query_text —
        // cùng đường `embed_query` mà RAG dùng, nên kết quả nhất quán.
        let query_vector = match payload["query_vector"].as_array() {
            Some(arr) if !arr.is_empty() => {
                let mut v = Vec::with_capacity(arr.len());
                for x in arr {
                    v.push(
                        x.as_f64()
                            .ok_or_else(|| "Invalid float in query_vector".to_string())?
                            as f32,
                    );
                }
                v
            }
            _ => {
                let state_embed = state.clone();
                let q = query_text.clone();
                tokio::task::spawn_blocking(move || {
                    let mut guard = state_embed.embedder.blocking_lock();
                    let engine = guard.as_mut().ok_or_else(|| {
                        "Thieu 'query_vector' va khong co model embedding de tu tinh. \
                         Tai model vao models/embedding/ (node scripts/fetch-embedding-model.mjs) \
                         hoac tu cap vector 384 chieu."
                            .to_string()
                    })?;
                    engine.embed_query(&q)
                })
                .await
                .map_err(|e| format!("Embedding task panicked: {}", e))??
            }
        };

        let top_k = payload["top_k"].as_u64().unwrap_or(5) as usize;

        let dense_weight = payload["dense_weight"].as_f64().unwrap_or(1.0);
        let sparse_weight = payload["sparse_weight"].as_f64().unwrap_or(1.0);

        let results = tokio::task::spawn_blocking(move || {
            let conn = state
                .db
                .readers
                .get()
                .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

            db::search_hybrid_vectors(
                &conn,
                &query_text,
                &query_vector,
                top_k,
                &filter,
                dense_weight,
                sparse_weight,
            )
            .map_err(|e| format!("Hybrid search failed: {}", e))
        })
        .await
        .map_err(|e| format!("Blocking task panicked: {}", e))??;

        Ok(serde_json::to_value(results).unwrap())
    }
    "memory:upsert_vector" => {
        let vec_id = payload["vecId"]
            .as_str()
            .ok_or_else(|| "Missing 'vecId'".to_string())?
            .to_string();
        let r#type = payload["type"]
            .as_str()
            .ok_or_else(|| "Missing 'type'".to_string())?
            .to_string();
        let content = payload["content"]
            .as_str()
            .ok_or_else(|| "Missing 'content'".to_string())?
            .to_string();

        let vector_val = payload["vector"]
            .as_array()
            .ok_or_else(|| "Missing 'vector'".to_string())?;
        let mut vector = Vec::with_capacity(vector_val.len());
        for v in vector_val {
            let f = v
                .as_f64()
                .ok_or_else(|| "Invalid float in vector".to_string())?
                as f32;
            vector.push(f);
        }

        let domain = payload
            .get("domain")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let category = payload
            .get("category")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let trace_keywords_val = payload.get("traceKeywords").and_then(|v| v.as_array());
        let mut trace_keywords = Vec::new();
        if let Some(arr) = trace_keywords_val {
            for v in arr {
                if let Some(s) = v.as_str() {
                    trace_keywords.push(s.to_string());
                }
            }
        }

        let file_target = payload
            .get("fileTarget")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let source_event_ids_val = payload.get("sourceEventIds").and_then(|v| v.as_array());
        let mut source_event_ids = Vec::new();
        if let Some(arr) = source_event_ids_val {
            for v in arr {
                if let Some(s) = v.as_str() {
                    source_event_ids.push(s.to_string());
                }
            }
        }

        tokio::task::spawn_blocking(move || {
            let conn = state
                .db
                .writer
                .get()
                .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

            db::upsert_vector(
                &conn,
                &vec_id,
                &r#type,
                &content,
                &vector,
                domain.as_deref(),
                category.as_deref(),
                Some(&trace_keywords),
                file_target.as_deref(),
                Some(&source_event_ids),
            )
            .map_err(|e| format!("Failed to upsert vector: {}", e))
        })
        .await
        .map_err(|e| format!("Blocking task panicked: {}", e))??;

        Ok(serde_json::json!({ "success": true }))
    }
        _ => Err(format!("Unknown command: {command}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owns_dung_bay_lenh_va_khong_om_lenh_khac() {
        assert_eq!(OWNED.len(), 7);
        for name in OWNED {
            assert!(owns(name));
        }
        // Ba tên phẳng KHÔNG có tiền tố `memory:` — `strip_prefix` sẽ bỏ sót:
        assert!(owns("get_memory_data"));
        assert!(owns("delete_memory_fact"));
        assert!(owns("reset_memory"));
        // Nhưng không ôm lệnh của miền khác:
        assert!(!owns("consolidate_memory"), "chưa có arm nào cho lệnh này");
        assert!(!owns("get_tasks"));
    }
}
