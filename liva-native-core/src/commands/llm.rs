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
    "chat:completion",
    "task_plan_chat",
    "agent:react_step",
    "agent:plan_and_execute",
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
        "chat:completion" => {
            let memory_scope = agent::graph::ConversationMemoryScope::new("local", "default")?;
            handle_chat_completion_scoped(state, payload, tx, req_id, memory_scope).await
        }
        "task_plan_chat" => task_plan_chat(state, payload, tx).await,
        "agent:react_step" => agent_react_step(state, payload).await,
        "agent:plan_and_execute" => agent_plan_and_execute(state, payload).await,
        _ => Err(format!("Unknown command: {command}")),
    }
}

async fn swap_model(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let model_path_str = payload["model_path"]
        .as_str()
        .ok_or_else(|| "Missing 'model_path'".to_string())?;
    let model_path = std::path::Path::new(model_path_str);
    // C2: chỉ cho nạp .gguf trong thư mục model đã cấu hình — không phải
    // đường dẫn tuỳ ý vào parser C++ của llama.cpp.
    let models_dir = configured_models_dir();
    let model_path = verify_model_artifact(&models_dir, model_path)?;

    let n_ctx = payload["n_ctx"].as_u64().map(|v| v as usize);
    let n_gpu_layers = payload["n_gpu_layers"].as_u64().map(|v| v as u32);
    let vocab_only = payload["vocab_only"].as_bool();

    let mut llm_manager = state.llm.lock().await;
    llm_manager
        .swap_model(&model_path, n_ctx, n_gpu_layers, vocab_only)
        .await?;

    Ok(json!({ "success": true }))
}

async fn embed(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let mot_chuoi = payload["input"].is_string();
    let inputs = if let Some(s) = payload["input"].as_str() {
        vec![s.to_string()]
    } else if let Some(arr) = payload["input"].as_array() {
        arr.iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "Invalid string in input list".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        return Err("Missing or invalid 'input' parameter".to_string());
    };

    let mut llm_manager = state.llm.lock().await;
    if llm_manager.vocab_only {
        return Err("Cannot compute embeddings on a vocab-only model".to_string());
    }
    let engine = llm_manager
        .engine
        .as_mut()
        .ok_or_else(|| crate::llm::engine::ERR_NO_MODEL.to_string())?;
    let mut embeddings = Vec::new();
    for text in inputs {
        let emb = llm::get_embedding(&engine.model, &mut engine.context, &text)?;
        embeddings.push(emb);
    }

    // Vào là một chuỗi thì ra một vector; vào là mảng thì ra mảng vector.
    if mot_chuoi {
        Ok(serde_json::to_value(&embeddings[0]).unwrap())
    } else {
        Ok(serde_json::to_value(embeddings).unwrap())
    }
}

async fn health_check(state: Arc<AppState>) -> Result<Value, String> {
    let llm_manager = state.llm.lock().await;
    Ok(json!({
        "status": "healthy",
        "model_loaded": llm_manager.engine.is_some(),
        "model_path": llm_manager.current_model_path.to_string_lossy().to_string(),
        "n_ctx": llm_manager.n_ctx,
        "n_gpu_layers": llm_manager.n_gpu_layers
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

    let temperature = payload["temperature"]
        .as_f64()
        .unwrap_or(llm::persona::TEMP_DEFAULT as f64) as f32;
    let top_p = payload["top_p"]
        .as_f64()
        .unwrap_or(llm::persona::TOP_P_DEFAULT as f64) as f32;
    let stream = payload["stream"].as_bool().unwrap_or(tx.is_some());

    let compiled_prompt = llm::compile_prompt(&messages)?;
    let task_id_clone = task_id.clone();

    let completion_output = tokio::task::spawn_blocking(move || {
        let mut llm_manager = state.llm.blocking_lock();

        if stream {
            let tx_inner =
                tx.ok_or_else(|| "IPC output channel missing for streaming".to_string())?;

            llm_manager.generate_completion(&compiled_prompt, temperature, top_p, |piece| {
                if piece.is_empty() {
                    return true;
                }
                let chunk = json!({
                    "taskId": task_id_clone.clone(),
                    "message": piece,
                    "done": false
                });
                if let Ok(chunk_str) = serde_json::to_string(&chunk) {
                    let _ = tx_inner.blocking_send(chunk_str);
                }
                true
            })
        } else {
            llm_manager.generate_completion(&compiled_prompt, temperature, top_p, |_| true)
        }
    })
    .await
    .map_err(|e| format!("Blocking task panicked: {}", e))??;

    Ok(json!({
        "taskId": task_id,
        "message": completion_output.text,
        "done": true
    }))
}

pub async fn build_channel_tool_dispatcher(state: &AppState) -> crate::skills::dispatcher::UnifiedToolDispatcher {
    let dispatcher = crate::skills::dispatcher::UnifiedToolDispatcher::new();

    // 1. Smart home control
    dispatcher.register_native_handler(
        crate::skills::manifest::SkillToolDefinition {
            name: "control_smarthome".to_string(),
            description: "Control smart home devices (light, fan, ac)".to_string(),
            risk_level: crate::skills::manifest::RiskLevel::ReadOnlySafe,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "device": { "type": "string" },
                    "action": { "type": "string" }
                },
                "required": ["device", "action"]
            }),
        },
        |args| {
            Box::pin(async move {
                crate::integrations::smart_home::execute(args)
                    .map(|msg| json!({ "status": "ok", "message": msg }))
            })
        },
    ).await;

    // 2. Obsidian Vault search
    let vault_server = state.mcp_server.clone();
    dispatcher.register_native_handler(
        crate::skills::manifest::SkillToolDefinition {
            name: "search_vault".to_string(),
            description: "Search notes in Obsidian Vault".to_string(),
            risk_level: crate::skills::manifest::RiskLevel::ReadOnlySafe,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
        },
        move |args| {
            let server = vault_server.clone();
            Box::pin(async move {
                let req = crate::mcp::protocol::CallToolRequest {
                    name: "search_vault".to_string(),
                    arguments: args,
                };
                let res = server.call_tool(req).await?;
                let text = res
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        crate::mcp::protocol::ToolContent::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(json!({ "results": text }))
            })
        },
    ).await;

    // 3. Weather lookup
    dispatcher.register_native_handler(
        crate::skills::manifest::SkillToolDefinition {
            name: "get_weather".to_string(),
            description: "Get weather information for a location".to_string(),
            risk_level: crate::skills::manifest::RiskLevel::ReadOnlySafe,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "location": { "type": "string" }
                },
                "required": ["location"]
            }),
        },
        |args| {
            Box::pin(async move {
                let location = args.get("location").and_then(|l| l.as_str()).unwrap_or("Hà Nội");
                Ok(json!({
                    "location": location,
                    "temperature": "28°C",
                    "condition": "Nhiều mây, có mưa rào rải rác",
                    "humidity": "75%"
                }))
            })
        },
    ).await;

    // 4. Web search fallback
    dispatcher.register_native_handler(
        crate::skills::manifest::SkillToolDefinition {
            name: "search_tool".to_string(),
            description: "Fallback web search tool".to_string(),
            risk_level: crate::skills::manifest::RiskLevel::ReadOnlySafe,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
        },
        |args| {
            Box::pin(async move {
                let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
                Ok(json!({
                    "query": query,
                    "results": [
                        format!("Kết quả tìm kiếm cho '{}': dữ liệu hợp lệ", query)
                    ]
                }))
            })
        },
    ).await;

    dispatcher
}

async fn agent_react_step(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let mut agent_state: agent::AgentState = match payload.get("state") {
        Some(s) => serde_json::from_value(s.clone()).map_err(|e| format!("Invalid state: {e}"))?,
        None => {
            let msg = payload.get("goal").or_else(|| payload.get("message")).and_then(|v| v.as_str()).unwrap_or("Default Goal");
            agent::AgentState {
                messages: vec![json!({"role": "user", "content": msg})],
                ..Default::default()
            }
        }
    };

    let dispatcher = build_channel_tool_dispatcher(&state).await;
    let outcome = agent::AgentLoop::step(&mut agent_state, &dispatcher)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({
        "outcome": outcome,
        "state": agent_state
    }))
}

async fn agent_plan_and_execute(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let goal = payload
        .get("goal")
        .or_else(|| payload.get("message"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'goal' or 'message' in payload".to_string())?;

    let max_iterations = payload
        .get("maxIterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;

    let mut agent_state = agent::AgentState {
        messages: vec![json!({"role": "user", "content": goal})],
        ..Default::default()
    };

    let dispatcher = build_channel_tool_dispatcher(&state).await;
    let final_answer = agent::AgentLoop::run(&mut agent_state, &dispatcher, max_iterations)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({
        "final_answer": final_answer,
        "plan": agent_state.get_plan(),
        "step_outputs": agent_state.step_outputs,
        "scratchpad": agent_state.scratchpad
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owns_dung_nam_lenh_va_khong_om_lenh_khac() {
        assert_eq!(OWNED.len(), 7);
        for name in OWNED {
            assert!(owns(name));
        }
        // Gom cả hai tiền tố khác nhau, nên `strip_prefix("llm:")` sẽ bỏ sót:
        assert!(owns("chat:completion"));
        assert!(owns("task_plan_chat"));
        assert!(owns("agent:react_step"));
        assert!(owns("agent:plan_and_execute"));
        // Nhưng không được ôm CRUD của miền task:
        assert!(!owns("get_tasks"));
        assert!(!owns("add_task"));
    }
}
