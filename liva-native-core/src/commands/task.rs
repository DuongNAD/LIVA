//! Miền công việc (task) — CRUD trên bảng `tasks`.
//!
//! Tách khỏi `handle_command` 26/07/2026 (B1 bước 4). Bốn nhánh, đều là SQL
//! thuần trên một bảng duy nhất, đều chạy trong `spawn_blocking` vì `rusqlite`
//! là API chặn.
//!
//! **`task_plan_chat` KHÔNG thuộc đây** dù tên bắt đầu bằng `task`: nó gọi LLM
//! một lượt để sinh kế hoạch, không đụng bảng `tasks`, và sẽ theo miền `llm`.
//! Đây đúng là loại nhầm lẫn mà định tuyến theo tiền tố chuỗi sẽ mắc, còn danh
//! sách tên khai tường minh thì không.

use crate::AppState;
use serde_json::{Value, json};
use std::sync::Arc;

const OWNED: &[&str] = &["get_tasks", "add_task", "delete_task", "update_task"];

/// Lệnh này có thuộc miền task không.
pub fn owns(command: &str) -> bool {
    OWNED.contains(&command)
}

pub async fn handle(state: Arc<AppState>, command: &str, payload: Value) -> Result<Value, String> {
    match command {
        "get_tasks" => get_tasks(state).await,
        "add_task" => add_task(state, payload).await,
        "delete_task" => delete_task(state, payload).await,
        "update_task" => update_task(state, payload).await,
        _ => Err(format!("Unknown command: {command}")),
    }
}

/// Giây UNIX. Gộp từ hai bản sao y hệt trong `add_task`/`update_task`.
fn bay_gio() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Đọc `payload[key]` bắt buộc là chuỗi.
fn chuoi_bat_buoc(payload: &Value, key: &str) -> Result<String, String> {
    payload[key]
        .as_str()
        .ok_or_else(|| format!("Missing '{key}' in payload"))
        .map(str::to_string)
}

async fn get_tasks(state: Arc<AppState>) -> Result<Value, String> {
    let results = tokio::task::spawn_blocking(move || {
        let conn = state
            .db
            .readers
            .get()
            .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, title, description, status, priority, result, created_at, updated_at \
                 FROM tasks",
            )
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1)?,
                    "description": row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    "status": row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "pending".to_string()),
                    "priority": row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "medium".to_string()),
                    "result": row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                    "createdAt": row.get::<_, i64>(6)?,
                    "updatedAt": row.get::<_, i64>(7)?,
                }))
            })
            .map_err(|e| format!("Failed to execute query: {}", e))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| format!("Row extraction failed: {}", e))?);
        }
        Ok::<_, String>(list)
    })
    .await
    .map_err(|e| format!("Blocking task panicked: {}", e))??;

    Ok(json!({ "tasks": results }))
}

async fn add_task(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let title = chuoi_bat_buoc(&payload, "title")?;
    let description = payload["description"].as_str().unwrap_or("").to_string();
    let priority = payload["priority"].as_str().unwrap_or("medium").to_string();
    let status = payload["status"].as_str().unwrap_or("pending").to_string();

    let id = match payload.get("id").and_then(|v| v.as_str()) {
        Some(id_str) => id_str.to_string(),
        None => rand::random::<u64>().to_string(),
    };

    let now = bay_gio();
    let id_clone = id.clone();
    tokio::task::spawn_blocking(move || {
        let conn = state
            .db
            .writer
            .get()
            .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

        conn.execute(
            "INSERT INTO tasks (id, title, description, status, priority, result, created_at, \
             updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![id_clone, title, description, status, priority, "", now, now],
        )
        .map_err(|e| format!("Failed to insert task: {}", e))
    })
    .await
    .map_err(|e| format!("Blocking task panicked: {}", e))??;

    Ok(json!({ "success": true, "id": id }))
}

async fn delete_task(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let id = chuoi_bat_buoc(&payload, "id")?;

    tokio::task::spawn_blocking(move || {
        let conn = state
            .db
            .writer
            .get()
            .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

        conn.execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| format!("Failed to delete task: {}", e))
    })
    .await
    .map_err(|e| format!("Blocking task panicked: {}", e))??;

    Ok(json!({ "success": true }))
}

async fn update_task(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let id = chuoi_bat_buoc(&payload, "id")?;
    let updates = payload["updates"]
        .as_object()
        .cloned()
        .ok_or_else(|| "Missing or invalid 'updates' object".to_string())?;

    tokio::task::spawn_blocking(move || {
        let mut conn = state
            .db
            .writer
            .get()
            .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

        let tx = conn
            .transaction()
            .map_err(|e| format!("Failed to start transaction: {}", e))?;

        // Đọc giá trị hiện tại trong một scope lồng, để `stmt` bị drop trước
        // `tx.commit()`.
        let current: (String, String, String, String, String) = {
            let mut stmt = tx
                .prepare(
                    "SELECT title, description, status, priority, result FROM tasks WHERE id = ?1",
                )
                .map_err(|e| format!("Failed to prepare select query: {}", e))?;

            stmt.query_row(rusqlite::params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(2)?
                        .unwrap_or_else(|| "pending".to_string()),
                    row.get::<_, Option<String>>(3)?
                        .unwrap_or_else(|| "medium".to_string()),
                    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                ))
            })
            .map_err(|e| format!("Task not found: {}", e))?
        };

        // Trường vắng trong `updates` = GIỮ NGUYÊN giá trị cũ, không phải xoá.
        let giu = |key: &str, cu: String| {
            updates
                .get(key)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or(cu)
        };
        let title = giu("title", current.0);
        let description = giu("description", current.1);
        let status = giu("status", current.2);
        let priority = giu("priority", current.3);
        let result = giu("result", current.4);

        tx.execute(
            "UPDATE tasks SET title = ?1, description = ?2, status = ?3, priority = ?4, \
             result = ?5, updated_at = ?6 WHERE id = ?7",
            rusqlite::params![title, description, status, priority, result, bay_gio(), id],
        )
        .map_err(|e| format!("Failed to update task: {}", e))?;

        tx.commit()
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("Blocking task panicked: {}", e))??;

    Ok(json!({ "success": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `owns()` phải khớp đúng tập lệnh mà `handle()` xử lý — lệch nhau thì một
    /// lệnh có thật rơi vào nhánh `_` và trả "Unknown command".
    #[test]
    fn owns_dung_bon_lenh_va_khong_om_lenh_khac() {
        assert_eq!(OWNED.len(), 4);
        for name in OWNED {
            assert!(owns(name));
        }
        // `task_plan_chat` bắt đầu bằng "task" nhưng KHÔNG thuộc miền này —
        // đây là ca mà định tuyến theo tiền tố sẽ bắt nhầm.
        assert!(!owns("task_plan_chat"), "task_plan_chat thuộc miền llm");
        assert!(!owns("get_config"));
    }

    #[test]
    fn chuoi_bat_buoc_neu_tenkhop_thi_lay_duoc_va_thieu_thi_bao_ten() {
        let p = json!({ "id": "abc" });
        assert_eq!(chuoi_bat_buoc(&p, "id").unwrap(), "abc");
        let err = chuoi_bat_buoc(&p, "title").unwrap_err();
        assert!(
            err.contains("title"),
            "thông điệp phải nêu tên trường: {err}"
        );
    }
}
