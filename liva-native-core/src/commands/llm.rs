//! Miền LLM — nạp/đổi model, embedding, health-check, hội thoại, lập kế hoạch.
//!
//! Tách khỏi `handle_command` 26/07/2026 (B1 bước 5). Năm nhánh:
//! `llm:swap_model` · `llm:embed` · `llm:health_check` · `chat:completion` ·
//! `task_plan_chat`.
//!
//! ## Vì sao miền này nhận thêm `tx`/`req_id`
//!
//! Bốn miền tách trước chỉ cần `(state, payload)`. Miền này là miền **DUY NHẤT
//! biết stream**: `chat:completion` và `task_plan_chat` đẩy từng mẩu chữ ra
//! `tx` trong lúc sinh, và `req_id` là thứ client dùng để ghép mẩu về đúng
//! request. Nên chữ ký của nó rộng hơn — và đó là thông tin, không phải phiền
//! toái: nhìn chữ ký là biết miền nào có thể nói dở chừng.
//!
//! ## `task_plan_chat` nằm ở ĐÂY, không ở miền task
//!
//! Tên bắt đầu bằng `task` nhưng nó không đụng bảng `tasks` để ghi — chỉ ĐỌC
//! `title`/`description` của một task rồi gọi LLM một lượt. Trọng tâm là lượt
//! sinh, không phải CRUD. Xem test `owns` ở `commands/task.rs`.

use crate::{
    AppState, agent, configured_models_dir, handle_chat_completion_scoped, llm,
    verify_model_artifact,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

const OWNED: &[&str] = &[
    "llm:swap_model",
    "llm:embed",
    "llm:health_check",
    "llm:context_info",
    "llm:cancel",
    "chat:completion",
    "task_plan_chat",
    "telemetry:summary",
];

/// Lệnh này có thuộc miền LLM không.
///
/// Không dùng `strip_prefix("llm:")` vì miền gom cả `chat:completion` và
/// `task_plan_chat` — chúng cùng đi qua một engine và một `Mutex`, nên tách ra
/// hai miền chỉ tạo thêm chỗ để lệch.
pub fn owns(command: &str) -> bool {
    OWNED.contains(&command)
}

pub async fn handle(
    state: Arc<AppState>,
    command: &str,
    payload: Value,
    tx: Option<Sender<String>>,
    req_id: Option<String>,
) -> Result<Value, String> {
    match command {
        "llm:swap_model" => swap_model(state, payload).await,
        "llm:embed" => embed(state, payload).await,
        "llm:health_check" => health_check(state).await,
        "llm:context_info" => context_info(state).await,
        "llm:cancel" => cancel(state).await,
        "chat:completion" => {
            let memory_scope = agent::graph::ConversationMemoryScope::new("local", "default")?;
            handle_chat_completion_scoped(state, payload, tx, req_id, memory_scope).await
        }
        "task_plan_chat" => task_plan_chat(state, payload, tx).await,
        "telemetry:summary" => telemetry_summary(state, payload).await,
        _ => Err(format!("Unknown command: {command}")),
    }
}

async fn swap_model(_state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let model_path_str = payload["model_path"]
        .as_str()
        .ok_or_else(|| "Missing 'model_path'".to_string())?;
    let model_path = std::path::Path::new(model_path_str);
    // C2: chỉ cho nạp .gguf trong thư mục model đã cấu hình — không phải
    // đường dẫn tuỳ ý vào parser C++ của llama.cpp.
    let models_dir = configured_models_dir();
    let model_path = verify_model_artifact(&models_dir, model_path)?;

    let n_ctx = payload["n_ctx"]
        .as_u64()
        .map(|v| v as usize)
        .unwrap_or(4096);
    let n_gpu_layers = payload["n_gpu_layers"]
        .as_u64()
        .map(|v| v as u32)
        .unwrap_or(0);

    crate::llm::engine::update_active_metadata(
        &model_path.to_string_lossy(),
        true,
        n_ctx,
        n_gpu_layers,
    );

    Ok(json!({ "success": true }))
}

async fn embed(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let mot_chuoi = payload["input"].is_string();
    let inputs = if let Some(s) = payload["input"].as_str() {
        vec![s.to_string()]
    } else if let Some(arr) = payload["input"].as_array() {
        let mut list = Vec::with_capacity(arr.len());
        for v in arr {
            let s = v
                .as_str()
                .ok_or_else(|| "Invalid string in input list".to_string())?;
            list.push(s.to_string());
        }
        list
    } else {
        return Err("Missing or invalid 'input' parameter".to_string());
    };

    // Decoupled embedding pipeline: computes embeddings via thread-safe ORT
    // without acquiring state.llm lock, preventing contention with chat generation.
    let embeddings = crate::llm::embedding::compute_embeddings(&state, &inputs).await?;

    // Vào là một chuỗi thì ra một vector; vào là mảng thì ra mảng vector.
    if mot_chuoi {
        serde_json::to_value(&embeddings[0]).map_err(|e| e.to_string())
    } else {
        serde_json::to_value(embeddings).map_err(|e| e.to_string())
    }
}

async fn health_check(_state: Arc<AppState>) -> Result<Value, String> {
    let is_generating = crate::llm::engine::is_generating();
    let meta = crate::llm::engine::get_active_metadata();
    if is_generating {
        return Ok(json!({
            "status": "busy",
            "model_loaded": meta.model_loaded || is_generating,
            "model_path": if !meta.model_path.is_empty() {
                meta.model_path
            } else {
                crate::paths::configured_router_model_path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default()
            },
            "n_ctx": meta.n_ctx,
            "n_gpu_layers": meta.n_gpu_layers,
            "is_generating": true
        }));
    }

    let loaded = meta.model_loaded;
    let path = meta.model_path.clone();
    Ok(json!({
        "status": if loaded { "healthy" } else { "offline" },
        "model_loaded": loaded,
        "model_path": path,
        "n_ctx": meta.n_ctx,
        "n_gpu_layers": meta.n_gpu_layers,
        "is_generating": false
    }))
}

async fn context_info(_state: Arc<AppState>) -> Result<Value, String> {
    let is_generating = crate::llm::engine::is_generating();
    let meta = crate::llm::engine::get_active_metadata();
    let (n_ctx, n_gpu_layers, model_path, model_loaded) = (
        meta.n_ctx,
        meta.n_gpu_layers,
        if !meta.model_path.is_empty() {
            meta.model_path
        } else {
            crate::paths::configured_router_model_path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        },
        meta.model_loaded,
    );

    let reserve = llm::engine::RESERVE_FOR_COMPLETION;
    let max_prompt_tokens = n_ctx.saturating_sub(reserve);

    Ok(json!({
        "status": if is_generating { "busy" } else if model_loaded { "ready" } else { "idle" },
        "model_loaded": model_loaded,
        "model_path": model_path,
        "n_ctx": n_ctx,
        "context_size": n_ctx,
        "n_gpu_layers": n_gpu_layers,
        "reserve_for_completion": reserve,
        "max_prompt_tokens": max_prompt_tokens,
        "is_generating": is_generating
    }))
}

async fn cancel(_state: Arc<AppState>) -> Result<Value, String> {
    crate::llm::engine::request_cancel();
    let was_generating = crate::llm::engine::is_generating();
    Ok(json!({
        "success": true,
        "cancelled": true,
        "status": "cancellation_requested",
        "was_generating": was_generating
    }))
}

async fn task_plan_chat(
    state: Arc<AppState>,
    payload: Value,
    tx: Option<Sender<String>>,
) -> Result<Value, String> {
    let task_id = payload["taskId"]
        .as_str()
        .ok_or_else(|| "Missing 'taskId' in payload".to_string())?
        .to_string();

    let message = payload
        .get("message")
        .or_else(|| payload.get("text"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'message' or 'text' in payload".to_string())?
        .to_string();

    let state_clone = state.clone();
    let task_id_clone = task_id.clone();
    let (title, description) = tokio::task::spawn_blocking(move || {
        let conn = state_clone
            .db
            .readers
            .get()
            .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

        conn.query_row(
            "SELECT title, description FROM tasks WHERE id = ?1",
            rusqlite::params![task_id_clone],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                ))
            },
        )
        .map_err(|e| format!("Failed to query task: {}", e))
    })
    .await
    .map_err(|e| format!("Blocking task panicked: {}", e))??;

    // Title/description are user-authored: interpolate them as
    // delimited DATA in the user turn (never into the system prompt),
    // with delimiter sequences neutralized.
    let user_content = format!(
        "<user_task_title>{}</user_task_title>\n<user_task_description>{}</user_task_description>\n\n{}",
        llm::persona::sanitize_untrusted(&title),
        llm::persona::sanitize_untrusted(&description),
        message
    );

    let messages = vec![
        llm::ChatMessage {
            role: "system".to_string(),
            content: llm::persona::SYS_TASK_PLANNER.to_string(),
        },
        llm::ChatMessage {
            role: "user".to_string(),
            content: user_content,
        },
    ];

    let _temperature = payload["temperature"]
        .as_f64()
        .unwrap_or(llm::persona::TEMP_DEFAULT as f64) as f32;
    let _top_p = payload["top_p"]
        .as_f64()
        .unwrap_or(llm::persona::TOP_P_DEFAULT as f64) as f32;
    let stream = payload["stream"].as_bool().unwrap_or(tx.is_some());

    let task_id_clone = task_id.clone();

    #[derive(serde::Serialize)]
    struct TaskStreamChunk<'a> {
        #[serde(rename = "taskId")]
        task_id: &'a str,
        message: &'a str,
        done: bool,
    }

    let prompt = crate::llm::prompt::compile_prompt(&messages)?;
    let text = state
        .llm
        .generate_text(prompt, liva_llm::Priority::Normal)
        .await
        .map_err(|e| format!("LLM generation failed: {e}"))?;

    if stream {
        if let Some(tx_inner) = tx {
            let chunk = TaskStreamChunk {
                task_id: &task_id_clone,
                message: &text,
                done: false,
            };
            let _ = crate::llm::nen_sinh_tiep(&tx_inner, &chunk);
            let done_chunk = TaskStreamChunk {
                task_id: &task_id_clone,
                message: "",
                done: true,
            };
            let _ = crate::llm::nen_sinh_tiep(&tx_inner, &done_chunk);
        }
    }

    Ok(json!({
        "taskId": task_id,
        "message": text,
        "done": true
    }))
}

async fn telemetry_summary(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let since_ts = payload.get("since_ts").and_then(Value::as_i64);
    let summary = state
        .db
        .spawn_reader(move |conn| crate::db::get_telemetry_summary(conn, since_ts))
        .await?;
    serde_json::to_value(&summary)
        .map_err(|e| format!("Failed to serialize telemetry summary: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> Arc<AppState> {
        let pool = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        Arc::new(AppState {
            db: pool,
            crypto: crate::crypto::EncryptionEngine::new("00000000000000000000000000000000"),
            stt: tokio::sync::Mutex::new(crate::stt::SttManager::new(".")),
            tts: tokio::sync::Mutex::new(None),
            tts_player: crate::tts::audio::TtsAudioPlayer::new(None),
            llm: AppState::mock_llm(),
            vad: tokio::sync::Mutex::new(None),
            denoiser: tokio::sync::Mutex::new(None),
            turn_shadow: tokio::sync::Mutex::new(None),
            aec: tokio::sync::Mutex::new(None),
            mcp_server: Arc::new(crate::mcp::server::NativeMcpServer::new("test_vault")),
            vision: tokio::sync::Mutex::new(crate::vision::VisionManager::new(
                Arc::new(crate::vision::capture::MockScreenCapturer::new(
                    64,
                    64,
                    crate::vision::capture::PixelFormat::Rgba,
                )),
                crate::vision::VisionConfig::default(),
            )),
            embedder: AppState::empty_embedder(),
            active_recall: Arc::new(crate::active_recall::ActiveRecallManager::new()),
            cua: AppState::mock_cua(),
        })
    }

    #[test]
    fn owns_dung_sau_lenh_va_khong_om_lenh_khac() {
        assert_eq!(OWNED.len(), 8);
        for name in OWNED {
            assert!(owns(name));
        }
        // Gom cả các tiền tố khác nhau, nên `strip_prefix("llm:")` sẽ bỏ sót:
        assert!(owns("chat:completion"));
        assert!(owns("task_plan_chat"));
        assert!(owns("telemetry:summary"));
        assert!(owns("llm:context_info"));
        assert!(owns("llm:cancel"));
        // Nhưng không được ôm CRUD của miền task:
        assert!(!owns("get_tasks"));
        assert!(!owns("add_task"));
    }

    #[tokio::test]
    async fn test_llm_cancel_sets_atomic_flag() {
        let state = create_test_state();
        let res = handle(state, "llm:cancel", json!({}), None, None)
            .await
            .expect("cancel handle");
        assert_eq!(res["cancelled"], true);
        assert_eq!(res["status"], "cancellation_requested");
        assert!(llm::engine::is_cancel_requested());

        // Reset so subsequent tests start clean
        let guard = llm::engine::GeneratingGuard::enter();
        drop(guard);
        assert!(!llm::engine::is_cancel_requested());
    }

    #[tokio::test]
    async fn test_llm_context_info_non_blocking() {
        let state = create_test_state();
        let res = handle(state, "llm:context_info", json!({}), None, None)
            .await
            .expect("context_info handle");
        assert_eq!(res["status"], "idle");
        assert_eq!(res["is_generating"], false);
        assert!(res["context_size"].as_u64().is_some());
    }

    #[tokio::test]
    async fn test_llm_health_check_non_blocking_when_locked() {
        let state = create_test_state();

        // When lock is free (and no model is loaded into memory yet):
        let res_free = handle(state.clone(), "llm:health_check", json!({}), None, None)
            .await
            .expect("health_check when free");
        assert_eq!(res_free["status"], "offline");

        // When generating guard is entered:
        let guard = llm::engine::GeneratingGuard::enter();
        // health_check must not hang/block; it must return immediately using metadata
        let res_busy = handle(state.clone(), "llm:health_check", json!({}), None, None)
            .await
            .expect("health_check when busy");
        assert_eq!(res_busy["status"], "busy");
        assert_eq!(res_busy["is_generating"], true);
        drop(guard);

        // After releasing, health_check is offline/idle again without blocking
        let res_after = handle(state.clone(), "llm:health_check", json!({}), None, None)
            .await
            .expect("health_check after release");
        assert_eq!(res_after["status"], "offline");
    }

    #[tokio::test]
    async fn test_llm_embed_empty_input() {
        let state = create_test_state();
        let res = handle(state, "llm:embed", json!({"input": []}), None, None)
            .await
            .expect("embed empty");
        assert_eq!(res, json!([]));
    }
}
