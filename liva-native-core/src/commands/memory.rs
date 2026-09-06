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
    "memory:delete_conversation",
    "memory:delete_subject",
    "memory:sweep_retention",
    "consolidate_memory",
    "reset_memory",
    "memory:search_hybrid",
    "memory:upsert_vector",
    "memory:list_episodes",
    "memory:get_episode",
    "memory:persist_episode",
    "memory:search_episodes",
    "memory:compress_context",
    "memory:condense_summary_tree",
    "memory:sync_obsidian_vault",
    "memory:run_hipporag_ppr",
    "memory:sanitize_enclave",
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
                let vec_id: String = row.get(0)?;
                let trace_kw_str: Option<String> = row.get(5)?;
                let src_event_ids_str: Option<String> = row.get(7)?;

                let trace_keywords: serde_json::Value = trace_kw_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(serde_json::json!([]));
                let source_event_ids: serde_json::Value = src_event_ids_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(serde_json::json!([]));

                Ok(serde_json::json!({
                    "id": vec_id.clone(),
                    "vecId": vec_id,
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
        "consolidate_memory" => {
            let batch_size = payload
                .get("batchSize")
                .and_then(Value::as_u64)
                .unwrap_or(25)
                .clamp(1, 100) as usize;
            let result =
                crate::memory_consolidation::consume_pending_once(state.db.clone(), state.crypto.clone(), batch_size)
                    .await?;
            Ok(serde_json::json!({
                "processed": result.processed,
                "consolidated": result.consolidated,
                "retried": result.retried,
                "deadLettered": result.dead_lettered,
                "factsExtracted": result.facts_extracted,
                "nodesCreated": result.nodes_created,
                "edgesCreated": result.edges_created,
            }))
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
        "memory:delete_conversation" => {
            let conversation_id = payload["conversationId"]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Missing non-empty 'conversationId'".to_string())?
                .to_string();
            // Mặc định là dry-run. Thao tác phá hủy chỉ chạy khi caller gửi
            // `dryRun: false` rõ ràng; owner bị khóa ở local vì command plane
            // không có identity Telegram đủ tin cậy để xóa hộ dữ liệu kênh khác.
            let dry_run = payload
                .get("dryRun")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let report = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {e}"))?;
                db::delete_conversation(&conn, "local", &conversation_id, dry_run)
                    .map_err(|e| format!("Delete conversation failed: {e}"))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {e}"))??;

            Ok(serde_json::to_value(report)
                .map_err(|e| format!("Serialize deletion report failed: {e}"))?)
        }
        "memory:delete_subject" => {
            let dry_run = payload
                .get("dryRun")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let report = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {e}"))?;
                db::delete_subject(&conn, "local", dry_run)
                    .map_err(|e| format!("Delete subject failed: {e}"))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {e}"))??;

            let success = !report.dry_run && report.wal_truncated;
            let warning = (!report.dry_run && !report.wal_truncated).then_some(
                "Logical deletion completed, but SQLite WAL is still held by a reader; \
                 run maintenance/restart before claiming byte-level erasure.",
            );
            let mut value = serde_json::to_value(report)
                .map_err(|e| format!("Serialize subject deletion report failed: {e}"))?;
            if let Some(object) = value.as_object_mut() {
                object.insert("success".to_string(), Value::Bool(success));
                if let Some(warning) = warning {
                    object.insert("error".to_string(), Value::String(warning.to_string()));
                }
            }
            Ok(value)
        }
        "memory:sweep_retention" => {
            if payload.get("maxAgeDays").is_some() {
                let max_age_days = payload
                    .get("maxAgeDays")
                    .and_then(Value::as_u64)
                    .filter(|days| (1..=36_500).contains(days))
                    .ok_or_else(|| {
                        "'maxAgeDays' must be an integer from 1 through 36500".to_string()
                    })?;
                let batch_limit = payload
                    .get("batchLimit")
                    .and_then(Value::as_u64)
                    .unwrap_or(10)
                    .min(25) as usize;
                let dry_run = payload
                    .get("dryRun")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| format!("System clock is before UNIX epoch: {e}"))?
                    .as_millis() as i64;
                let age_ms = i64::try_from(max_age_days)
                    .ok()
                    .and_then(|days| days.checked_mul(86_400_000))
                    .ok_or_else(|| "Retention age overflows milliseconds".to_string())?;
                let cutoff_ms = now_ms
                    .checked_sub(age_ms)
                    .ok_or_else(|| "Retention cutoff is before supported time range".to_string())?;
                let report = tokio::task::spawn_blocking(move || {
                    let conn = state
                        .db
                        .writer
                        .get()
                        .map_err(|e| format!("Failed to acquire write connection: {e}"))?;
                    db::sweep_conversation_retention(&conn, "local", cutoff_ms, batch_limit, dry_run)
                        .map_err(|e| format!("Retention sweep failed: {e}"))
                })
                .await
                .map_err(|e| format!("Blocking task panicked: {e}"))??;

                return serde_json::to_value(report)
                    .map_err(|e| format!("Serialize retention report failed: {e}"));
            }

            // Phase 3 L2 Episodic exponential decay sweep
            let current_timestamp = payload
                .get("current_timestamp")
                .or_else(|| payload.get("currentTimestamp"))
                .or_else(|| payload.get("currentTime"))
                .or_else(|| payload.get("now"))
                .and_then(Value::as_i64)
                .unwrap_or_else(|| chrono::Utc::now().timestamp());

            let cutoff_purge = payload
                .get("purge_threshold")
                .or_else(|| payload.get("purgeThreshold"))
                .or_else(|| payload.get("cutoff"))
                .and_then(Value::as_f64);

            let state_sweep = state.clone();
            let res = tokio::task::spawn_blocking(move || {
                let enclave = Arc::new(crate::memory::MemoryEnclave::new_from_master_key([0u8; 32]));
                let store = crate::memory::L2EpisodicStore::new(state_sweep.db.clone(), enclave);
                let report = store.sweep_retention(current_timestamp).map_err(|e| format!("Episodic sweep failed: {e}"))?;
                let mut purged_count = 0;
                if let Some(cutoff) = cutoff_purge {
                    purged_count = store.purge_decayed_events(cutoff).unwrap_or(0);
                }

                Ok::<_, String>(serde_json::json!({
                    "success": true,
                    "report": report,
                    "purged_count": purged_count,
                    "current_timestamp": current_timestamp,
                }))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {e}"))??;

            Ok(res)
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
        "reset_memory" => Err(
            "`reset_memory` chưa được cài đặt ở lõi. Xoá từng ký ức bằng \
         `delete_memory_fact` (Dashboard → Memory). Xoá toàn bộ cần thiết kế sao lưu \
         trước — chưa làm, không hoàn tác được."
                .to_string(),
        ),
        "memory:search_hybrid" => {
            let query_text = payload["query_text"]
                .as_str()
                .or_else(|| payload["queryText"].as_str())
                .or_else(|| payload["query"].as_str())
                .ok_or_else(|| "Missing 'query_text'".to_string())?
                .to_string();

            // Đây là command thô, chưa có identity đáng tin cậy phía server. Domain
            // do client tự khai không thể được dùng làm ranh giới conversation memory.
            let filter = parse_untrusted_memory_search_filter(&payload)?;

            // `query_vector` là TUỲ CHỌN từ 22/07/2026. Bắt client tự cấp vector
            // 384 chiều là lý do trực tiếp khiến không client nào gọi được lệnh
            // này (UI không có embedder). Thiếu thì server tự embed query_text —
            // cùng đường `embed_query` mà RAG dùng, nên kết quả nhất quán.
            let query_vector = match payload.get("query_vector").or_else(|| payload.get("queryVector")).and_then(Value::as_array) {
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
                    let opt_vec = tokio::task::spawn_blocking(move || {
                        let mut guard = state_embed.embedder.blocking_lock();
                        if let Some(engine) = guard.as_mut() {
                            engine.embed_query(&q).ok()
                        } else {
                            None
                        }
                    })
                    .await
                    .map_err(|e| format!("Embedding task panicked: {e}"))?;
                    opt_vec.unwrap_or_default()
                }
            };

            let top_k = payload
                .get("top_k")
                .or_else(|| payload.get("topK"))
                .and_then(Value::as_u64)
                .unwrap_or(5) as usize;

            let dense_weight = payload
                .get("dense_weight")
                .or_else(|| payload.get("denseWeight"))
                .and_then(Value::as_f64)
                .unwrap_or(1.0);
            let sparse_weight = payload
                .get("sparse_weight")
                .or_else(|| payload.get("sparseWeight"))
                .and_then(Value::as_f64)
                .unwrap_or(1.0);

            let graph_seeds_val = payload
                .get("graph_seeds")
                .or_else(|| payload.get("graphSeeds"))
                .and_then(Value::as_array);

            let graph_weight = payload
                .get("graph_weight")
                .or_else(|| payload.get("graphWeight"))
                .and_then(Value::as_f64)
                .unwrap_or(0.25);

            if let Some(seeds_arr) = graph_seeds_val {
                if !seeds_arr.is_empty() {
                    let mut seeds = Vec::new();
                    for item in seeds_arr {
                        if let Some(obj) = item.as_object() {
                            let name = obj
                                .get("name")
                                .or_else(|| obj.get("id"))
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            let w = obj.get("weight").and_then(Value::as_f64).unwrap_or(1.0) as f32;
                            if !name.is_empty() {
                                seeds.push((name.to_string(), w));
                            }
                        } else if let Some(s) = item.as_str() {
                            seeds.push((s.to_string(), 1.0f32));
                        }
                    }

                    let results = tokio::task::spawn_blocking(move || {
                        let conn = state
                            .db
                            .readers
                            .get()
                            .map_err(|e| format!("Failed to acquire read connection: {e}"))?;

                        let engine = crate::memory::VirtualMemoryEngine::new(
                            state.db.clone(),
                            Arc::new(crate::memory::MemoryEnclave::new_from_master_key([0u8; 32])),
                            100,
                        );
                        let hippo = engine.build_hipporag_from_db(&conn, true).map_err(|e| e.to_string())?;
                        let seed_refs: Vec<(&str, f32)> = seeds.iter().map(|(n, w)| (n.as_str(), *w)).collect();

                        // Query doc entities mapping from vectors_meta
                        let mut stmt_meta = conn.prepare(
                            "SELECT rowid, vec_id, content, trace_keywords FROM vectors_meta ORDER BY rowid ASC"
                        ).map_err(|e| e.to_string())?;
                        let meta_rows = stmt_meta.query_map([], |row| {
                            let rowid: i64 = row.get(0)?;
                            let vec_id: String = row.get(1)?;
                            let enc_content: String = row.get(2)?;
                            let kw_str: Option<String> = row.get(3)?;
                            Ok((rowid, vec_id, enc_content, kw_str))
                        }).map_err(|e| e.to_string())?;

                        let mut doc_entities = Vec::new();
                        for mr in meta_rows {
                            let (rowid, vec_id, enc_content, kw_str) = mr.map_err(|e| e.to_string())?;
                            let dec_content = state.crypto.read_fact(&enc_content).into_value();
                            let keywords: Vec<String> = kw_str
                                .and_then(|s| serde_json::from_str(&s).ok())
                                .unwrap_or_default();
                            let entity_weights: Vec<(String, f32)> = keywords.into_iter().map(|k| (k, 1.0f32)).collect();
                            doc_entities.push((rowid, vec_id, dec_content, entity_weights));
                        }

                        let query_vec_slice = if query_vector.is_empty() { None } else { Some(query_vector.as_slice()) };
                        let rrf_config = crate::memory::search::RrfConfig {
                            k: 60.0,
                            weight_bm25: sparse_weight,
                            weight_dense: dense_weight,
                            weight_graph: graph_weight,
                        };
                        let search_engine = crate::memory::search::HybridSearchEngine::with_rrf_config(rrf_config);

                        search_engine.search(
                            &conn,
                            &state.crypto,
                            &query_text,
                            query_vec_slice,
                            Some(&hippo),
                            Some(&seed_refs),
                            Some(&doc_entities),
                            top_k,
                            Some(&filter),
                        ).map_err(|e| format!("3-Way Hybrid search failed: {e}"))
                    })
                    .await
                    .map_err(|e| format!("Blocking task panicked: {e}"))??;

                    return Ok(serde_json::to_value(results).unwrap());
                }
            }

            let results = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                db::search_hybrid_vectors(
                    &conn,
                    &state.crypto,
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
                    &state.crypto,
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
        "memory:persist_episode" => {
            let episode: db::Episode = serde_json::from_value(payload.clone())
                .map_err(|e| format!("Invalid episode payload: {e}"))?;
            let turn_ids: Vec<String> = payload
                .get("turnIds")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {e}"))?;
                db::persist_episode(&conn, &state.crypto, &episode, None, &turn_ids)
                    .map_err(|e| format!("Failed to persist episode: {e}"))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {e}"))??;

            Ok(serde_json::json!({ "success": true }))
        }
        "memory:get_episode" => {
            let episode_id = payload
                .get("episodeId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing 'episodeId' in payload".to_string())?
                .to_string();

            let episode = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {e}"))?;
                db::get_episode(&conn, &episode_id)
                    .map_err(|e| format!("Failed to get episode: {e}"))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {e}"))??;

            Ok(serde_json::json!({ "episode": episode }))
        }
        "memory:list_episodes" => {
            let session_id = payload
                .get("sessionId")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let domain = payload
                .get("domain")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let limit = payload
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as usize;

            let episodes = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {e}"))?;
                db::list_episodes(&conn, session_id.as_deref(), domain.as_deref(), limit)
                    .map_err(|e| format!("Failed to list episodes: {e}"))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {e}"))??;

            Ok(serde_json::json!({ "episodes": episodes }))
        }
        "memory:search_episodes" => {
            let query_text = payload
                .get("queryText")
                .or_else(|| payload.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let domain = payload
                .get("domain")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let category = payload
                .get("category")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let top_k = payload
                .get("topK")
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as usize;

            // Compute vector if query is present
            let mut vector = vec![0.0f32; db::MEMORY_VECTOR_DIM];
            {
                let mut guard = state.embedder.lock().await;
                if let Some(ref mut emb) = *guard {
                    if let Ok(v) = emb.embed_query(&query_text) {
                        vector = v;
                    }
                }
            }

            let results = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {e}"))?;
                db::search_hybrid_episodes(
                    &conn,
                    &state.crypto,
                    &query_text,
                    &vector,
                    top_k,
                    domain.as_deref(),
                    category.as_deref(),
                    0.5,
                    0.5,
                )
                .map_err(|e| format!("Failed to search episodes: {e}"))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {e}"))??;

            Ok(serde_json::json!({ "results": results }))
        }
        "memory:compress_context" => {
            let text = payload
                .get("text")
                .or_else(|| payload.get("prompt"))
                .or_else(|| payload.get("content"))
                .and_then(Value::as_str)
                .ok_or_else(|| "Missing 'text' in payload for memory:compress_context".to_string())?
                .to_string();

            let target_ratio = payload
                .get("target_compression_ratio")
                .or_else(|| payload.get("targetCompressionRatio"))
                .or_else(|| payload.get("ratio"))
                .and_then(Value::as_f64)
                .unwrap_or(3.5);

            let target_reduction = payload
                .get("target_reduction_ratio")
                .or_else(|| payload.get("targetReductionRatio"))
                .or_else(|| payload.get("reduction"))
                .and_then(Value::as_f64);

            let preserve_xml = payload
                .get("preserve_xml")
                .or_else(|| payload.get("preserve_xml_tags"))
                .or_else(|| payload.get("preserveXml"))
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let preserve_code = payload
                .get("preserve_code")
                .or_else(|| payload.get("preserve_code_blocks"))
                .or_else(|| payload.get("preserveCode"))
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let preserve_json = payload
                .get("preserve_json")
                .or_else(|| payload.get("preserve_json_delimiters"))
                .or_else(|| payload.get("preserveJson"))
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let preserve_entities = payload
                .get("preserve_entities")
                .or_else(|| payload.get("preserve_named_entities"))
                .or_else(|| payload.get("preserveEntities"))
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let preserve_wikilinks = payload
                .get("preserve_wikilinks")
                .or_else(|| payload.get("preserveWikilinks"))
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let preserve_numbers_dates = payload
                .get("preserve_numbers_dates")
                .or_else(|| payload.get("preserve_numbers_and_dates"))
                .or_else(|| payload.get("preserveNumbersAndDates"))
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let preserve_system_prompts = payload
                .get("preserve_system_prompts")
                .or_else(|| payload.get("preserveSystemPrompts"))
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let custom_patterns = payload
                .get("custom_patterns")
                .or_else(|| payload.get("customProtectedPatterns"))
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            let custom_keywords = payload
                .get("custom_keywords")
                .or_else(|| payload.get("customProtectedKeywords"))
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            let max_loss = payload
                .get("max_information_loss")
                .or_else(|| payload.get("maxInformationLoss"))
                .and_then(Value::as_f64)
                .unwrap_or(0.015);

            let config = crate::memory::compression::LLMLinguaConfig {
                target_compression_ratio: target_ratio,
                target_reduction_ratio: target_reduction,
                preserve_xml_tags: preserve_xml,
                preserve_code_blocks: preserve_code,
                preserve_json_delimiters: preserve_json,
                preserve_named_entities: preserve_entities,
                preserve_wikilinks: preserve_wikilinks,
                preserve_numbers_and_dates: preserve_numbers_dates,
                preserve_system_prompts,
                custom_protected_patterns: custom_patterns,
                custom_protected_keywords: custom_keywords,
                max_information_loss: max_loss,
                smoothing_factor: 1e-5,
            };

            let compressor = crate::memory::compression::LLMLinguaCompressor::with_config(config);
            let result = compressor.compress(&text);

            Ok(serde_json::json!({
                "success": true,
                "compressed_text": result.compressed_text,
                "original_tokens": result.metrics.original_tokens,
                "compressed_tokens": result.metrics.compressed_tokens,
                "compression_ratio": result.metrics.compression_ratio,
                "reduction_ratio": result.metrics.reduction_ratio,
                "entity_preservation_ratio": result.metrics.entity_preservation_ratio,
                "estimated_semantic_loss": result.metrics.estimated_semantic_loss,
                "duration_us": result.metrics.duration_us,
                "metrics": result.metrics,
            }))
        }
        "memory:condense_summary_tree" => {
            let session_id = payload
                .get("session_id")
                .or_else(|| payload.get("sessionId"))
                .and_then(Value::as_str)
                .unwrap_or("default_session")
                .to_string();

            let turns_val = payload
                .get("turns")
                .and_then(Value::as_array)
                .ok_or_else(|| "Missing 'turns' array in payload".to_string())?;

            let mut config = crate::memory::compression::SummaryTreeConfig::default();
            if let Some(cfg_obj) = payload.get("config").and_then(Value::as_object) {
                if let Some(v) = cfg_obj.get("max_context_tokens").and_then(Value::as_u64) {
                    config.max_context_tokens = v as usize;
                }
                if let Some(v) = cfg_obj.get("overflow_threshold_ratio").and_then(Value::as_f64) {
                    config.overflow_threshold_ratio = v;
                }
                if let Some(v) = cfg_obj.get("chunk_target_tokens").and_then(Value::as_u64) {
                    config.chunk_target_tokens = v as usize;
                }
                if let Some(v) = cfg_obj.get("min_turns_per_chunk").and_then(Value::as_u64) {
                    config.min_turns_per_chunk = v as usize;
                }
                if let Some(v) = cfg_obj.get("max_turns_per_chunk").and_then(Value::as_u64) {
                    config.max_turns_per_chunk = v as usize;
                }
                if let Some(v) = cfg_obj.get("meta_summary_fanout").and_then(Value::as_u64) {
                    config.meta_summary_fanout = v as usize;
                }
            }

            let persist_db = payload
                .get("persist_to_db")
                .or_else(|| payload.get("persistToDb"))
                .or_else(|| payload.get("persist"))
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let mut tree = crate::memory::compression::RecursiveSummaryTree::new(&session_id, config);
            if persist_db {
                tree = tree.with_db_pool(state.db.clone());
            }

            let mut reports = Vec::new();
            let now_ms = chrono::Utc::now().timestamp_millis();

            for (idx, turn_val) in turns_val.iter().enumerate() {
                let role = turn_val
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("user");
                let content = turn_val
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let turn_id = turn_val
                    .get("turn_id")
                    .or_else(|| turn_val.get("turnId"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{session_id}_turn_{idx}"));
                let ts = turn_val
                    .get("timestamp")
                    .and_then(Value::as_i64)
                    .unwrap_or(now_ms + idx as i64);

                let turn = crate::memory::compression::DialogueTurn::with_id(&turn_id, &session_id, role, content, ts);
                if let Some(report) = tree.add_turn(turn) {
                    reports.push(report);
                }
            }

            let force_condense = payload
                .get("force_condense")
                .or_else(|| payload.get("forceCondense"))
                .and_then(Value::as_bool)
                .unwrap_or(false);

            if force_condense && reports.is_empty() {
                let rep = tree.condense_overflow();
                if rep.triggered {
                    reports.push(rep);
                }
            }

            let system_prompt = payload
                .get("system_prompt")
                .or_else(|| payload.get("systemPrompt"))
                .and_then(Value::as_str)
                .unwrap_or("");

            let persisted_episodes = reports
                .iter()
                .map(|r| r.new_level1_summaries.len() + r.new_meta_summaries.len())
                .sum::<usize>();

            Ok(serde_json::json!({
                "success": true,
                "session_id": session_id,
                "active_turns_count": tree.active_turn_count(),
                "active_summaries_count": tree.active_summary_count(),
                "working_tokens": tree.total_working_tokens(),
                "is_overflow": tree.is_overflow(),
                "overflow_threshold": tree.overflow_threshold(),
                "rendered_prompt": tree.render_working_prompt(system_prompt),
                "reports": reports,
                "persisted_episodes": persisted_episodes,
            }))
        }
        "memory:sync_obsidian_vault" => {
            let vault_path_str = payload
                .get("vault_path")
                .or_else(|| payload.get("vaultPath"))
                .or_else(|| payload.get("path"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| std::env::var("LIVA_VAULT_PATH").ok())
                .unwrap_or_else(|| "teamwork_projects/obsidian_llm_wiki/vault".to_string());

            let vault_path = std::path::PathBuf::from(&vault_path_str);
            if !vault_path.exists() {
                return Err(format!("Vault path does not exist: {vault_path_str}"));
            }

            let sync_to_db = payload
                .get("sync_to_db")
                .or_else(|| payload.get("syncToDb"))
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let bidirectional = payload
                .get("bidirectional")
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let state_sync = state.clone();
            let res = tokio::task::spawn_blocking(move || {
                let mut sync = crate::memory::l3_semantic::ObsidianVaultSync::new(
                    vault_path,
                    Some(state_sync.db.clone()),
                )?;
                let report = sync.scan_vault()?;

                let mut nodes_synced = 0;
                let mut edges_synced = 0;
                if sync_to_db {
                    let (ns, es) = sync.sync_to_db()?;
                    nodes_synced = ns;
                    edges_synced = es;
                }

                let csr = sync.build_csr_graph(bidirectional);

                Ok::<_, String>(serde_json::json!({
                    "success": true,
                    "vault_path": vault_path_str,
                    "report": report,
                    "nodes_synced": nodes_synced,
                    "edges_synced": edges_synced,
                    "csr_num_nodes": csr.num_nodes,
                    "csr_num_edges": csr.col_indices.len(),
                }))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {e}"))??;

            Ok(res)
        }
        "memory:run_hipporag_ppr" => {
            let seeds_val = payload
                .get("seeds")
                .and_then(Value::as_array)
                .ok_or_else(|| "Missing 'seeds' array in payload".to_string())?;

            let mut seeds = Vec::new();
            for item in seeds_val {
                if let Some(obj) = item.as_object() {
                    let name = obj
                        .get("name")
                        .or_else(|| obj.get("id"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let weight = obj
                        .get("weight")
                        .and_then(Value::as_f64)
                        .unwrap_or(1.0) as f32;
                    if !name.is_empty() {
                        seeds.push((name.to_string(), weight));
                    }
                } else if let Some(arr) = item.as_array() {
                    if let (Some(name), Some(w)) = (arr.get(0).and_then(Value::as_str), arr.get(1).and_then(Value::as_f64)) {
                        seeds.push((name.to_string(), w as f32));
                    }
                } else if let Some(name) = item.as_str() {
                    seeds.push((name.to_string(), 1.0f32));
                }
            }

            let top_k = payload
                .get("top_k")
                .or_else(|| payload.get("topK"))
                .and_then(Value::as_u64)
                .unwrap_or(10) as usize;

            let damping_factor = payload
                .get("damping_factor")
                .or_else(|| payload.get("dampingFactor"))
                .or_else(|| payload.get("alpha"))
                .and_then(Value::as_f64)
                .unwrap_or(0.15) as f32;

            let max_iterations = payload
                .get("max_iterations")
                .or_else(|| payload.get("maxIterations"))
                .and_then(Value::as_u64)
                .unwrap_or(20) as usize;

            let tolerance = payload
                .get("tolerance")
                .and_then(Value::as_f64)
                .unwrap_or(1e-6) as f32;

            let bidirectional = payload
                .get("bidirectional")
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let vault_path_opt = payload
                .get("vault_path")
                .or_else(|| payload.get("vaultPath"))
                .and_then(Value::as_str)
                .map(str::to_string);

            let state_graph = state.clone();
            let ppr_config = crate::memory::graph::hipporag::PprConfig {
                damping_factor,
                max_iterations,
                tolerance,
                chunk_size: 512,
            };

            let res = tokio::task::spawn_blocking(move || {
                let csr_graph = if let Some(ref vp) = vault_path_opt {
                    let p = std::path::Path::new(vp);
                    if p.exists() {
                        let mut sync = crate::memory::l3_semantic::ObsidianVaultSync::new(
                            p.to_path_buf(),
                            Some(state_graph.db.clone()),
                        )?;
                        let _ = sync.scan_vault();
                        sync.build_csr_graph(bidirectional)
                    } else {
                        let conn = state_graph.db.readers.get().map_err(|e| e.to_string())?;
                        let engine = crate::memory::VirtualMemoryEngine::new(
                            state_graph.db.clone(),
                            Arc::new(crate::memory::MemoryEnclave::new_from_master_key([0u8; 32])),
                            100,
                        );
                        let hippo = engine.build_hipporag_from_db(&conn, bidirectional).map_err(|e| e.to_string())?;
                        hippo.graph
                    }
                } else {
                    let conn = state_graph.db.readers.get().map_err(|e| e.to_string())?;
                    let engine = crate::memory::VirtualMemoryEngine::new(
                        state_graph.db.clone(),
                        Arc::new(crate::memory::MemoryEnclave::new_from_master_key([0u8; 32])),
                        100,
                    );
                    let hippo = engine.build_hipporag_from_db(&conn, bidirectional).map_err(|e| e.to_string())?;
                    hippo.graph
                };

                let hippo_engine = crate::memory::graph::hipporag::HippoRagEngine::with_config(csr_graph, ppr_config);
                let seed_pairs: Vec<(&str, f32)> = seeds.iter().map(|(n, w)| (n.as_str(), *w)).collect();
                let seed_indices: Vec<u32> = seed_pairs.iter().filter_map(|(s, _)| hippo_engine.graph.node_index(s)).collect();
                let seed_weights: Vec<f32> = seed_pairs.iter().filter_map(|(s, w)| {
                    if hippo_engine.graph.node_index(s).is_some() {
                        Some(*w)
                    } else {
                        None
                    }
                }).collect();

                let ppr_result = hippo_engine.run_ppr(&seed_indices, &seed_weights);
                let top_ranked = hippo_engine.rank_top_k(&ppr_result.probabilities, top_k);

                Ok::<_, String>(serde_json::json!({
                    "success": true,
                    "top_k_rankings": top_ranked.iter().map(|(name, score)| {
                        serde_json::json!({ "name": name, "score": score })
                    }).collect::<Vec<_>>(),
                    "num_graph_nodes": hippo_engine.graph.num_nodes,
                    "num_graph_edges": hippo_engine.graph.col_indices.len(),
                    "iterations": ppr_result.iterations,
                    "residual": ppr_result.residual,
                    "elapsed_ms": ppr_result.elapsed_ms,
                }))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {e}"))??;

            Ok(res)
        }
        "memory:sanitize_enclave" => {
            let state_enclave = state.clone();
            let res = tokio::task::spawn_blocking(move || {
                let conn = state_enclave
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {e}"))?;

                crate::memory::MemoryEnclave::sanitize_wal_checkpoint(&conn)
                    .map_err(|e| format!("WAL sanitization failed: {e}"))?;

                let _ = db::purge_personal_data_plaintext_remnants(&conn);

                let now_ms = chrono::Utc::now().timestamp_millis();
                Ok::<_, String>(serde_json::json!({
                    "success": true,
                    "wal_sanitized": true,
                    "timestamp": now_ms,
                }))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {e}"))??;

            Ok(res)
        }
        _ => Err(format!("Unknown command: {command}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owns_dung_tam_lenh_va_khong_om_lenh_khac() {
        assert_eq!(OWNED.len(), 20);
        for name in OWNED {
            assert!(owns(name));
        }
        // Bốn tên phẳng KHÔNG có tiền tố `memory:` — `strip_prefix` sẽ bỏ sót:
        assert!(owns("get_memory_data"));
        assert!(owns("delete_memory_fact"));
        assert!(owns("memory:delete_conversation"));
        assert!(owns("memory:delete_subject"));
        assert!(owns("memory:sweep_retention"));
        assert!(owns("consolidate_memory"));
        assert!(owns("reset_memory"));
        assert!(owns("memory:list_episodes"));
        assert!(owns("memory:get_episode"));
        assert!(owns("memory:persist_episode"));
        assert!(owns("memory:search_episodes"));
        assert!(owns("memory:compress_context"));
        assert!(owns("memory:condense_summary_tree"));
        assert!(owns("memory:sync_obsidian_vault"));
        assert!(owns("memory:run_hipporag_ppr"));
        assert!(owns("memory:sanitize_enclave"));
        // Nhưng không ôm lệnh của miền khác:
        assert!(!owns("get_tasks"));
    }
}
