//! Miền `setup:` — chuẩn bị model lần đầu, cho người dùng KHÔNG có Node/Rust/Git.
//!
//! Hai lệnh:
//!
//! - `setup:status` — thiếu gì, mất năng lực gì, còn bao nhiêu byte. Rẻ, chỉ đọc đĩa.
//! - `setup:fetch`  — tải phần thiếu, đẩy tiến độ theo dòng như `chat:completion`.
//!
//! Đây là miền thứ hai nhận `tx`/`req_id`. Lý do giống hệt miền `llm`: một lượt
//! tải 3,7 GB mà không có tiến độ thì người dùng không phân biệt được "đang tải"
//! với "treo", và sẽ tắt ứng dụng giữa chừng.

use crate::{AppState, setup};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn owns(command: &str) -> bool {
    command.starts_with("setup:")
}

/// Profile hợp lệ; giá trị lạ rơi về `minimal` thay vì tải nhầm 5 GB.
fn doc_profile(payload: &Value) -> String {
    match payload.get("profile").and_then(|v| v.as_str()) {
        Some("full") => "full".to_string(),
        _ => "minimal".to_string(),
    }
}

/// Hai gốc ghi: GGUF theo config (`ai.localModelsDir`), còn lại theo gốc tài nguyên.
fn goc_ghi() -> (std::path::PathBuf, std::path::PathBuf) {
    (crate::configured_models_dir(), crate::resource_write_root())
}

pub async fn handle(
    _state: Arc<AppState>,
    command: &str,
    payload: Value,
    tx: Option<tokio::sync::mpsc::Sender<String>>,
    req_id: Option<String>,
) -> Result<Value, String> {
    let verb = command.strip_prefix("setup:").unwrap_or(command);
    let profile = doc_profile(&payload);
    let (llm_dir, res_root) = goc_ghi();

    match verb {
        "status" => {
            let m = setup::load_manifest()?;
            let st = setup::status(&m, &profile, &llm_dir, &res_root);
            serde_json::to_value(st).map_err(|e| e.to_string())
        }

        "fetch" => {
            let m = setup::load_manifest()?;
            let force = payload
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            // Tiến độ đi qua `try_send`: KHÔNG chờ, nên vòng tải không bao giờ
            // bị người nhận chậm làm nghẽn. Rơi mất một khung tiến độ là vô hại;
            // khung cuối cùng của mỗi file luôn tới vì có mốc thời gian ép gửi.
            let mut lan_cuoi = std::time::Instant::now() - std::time::Duration::from_secs(1);
            let mut file_cuoi = String::new();
            let gui = |v: Value| {
                if let (Some(tx), Some(id)) = (tx.as_ref(), req_id.as_ref()) {
                    let khung = json!({ "id": id, "status": "ok", "data": v });
                    if let Ok(s) = serde_json::to_string(&khung) {
                        let _ = tx.try_send(s);
                    }
                }
            };

            let tt = setup::fetch_missing(
                &m,
                &profile,
                &llm_dir,
                &res_root,
                force,
                |p: setup::Progress| {
                    let doi_file = p.dest != file_cuoi;
                    if doi_file || lan_cuoi.elapsed() >= std::time::Duration::from_millis(300) {
                        lan_cuoi = std::time::Instant::now();
                        file_cuoi = p.dest.clone();
                        if let Ok(v) = serde_json::to_value(&p) {
                            gui(json!({ "progress": v, "done": false }));
                        }
                    }
                },
            )
            .await;

            let st = setup::status(&m, &profile, &llm_dir, &res_root);
            let ket = json!({
                "downloaded": tt.downloaded,
                "failed": tt.failed,
                "skippedManual": tt.skipped_manual,
                "status": serde_json::to_value(&st).map_err(|e| e.to_string())?,
                "done": true,
            });
            gui(ket.clone());
            Ok(ket)
        }

        // `paths` để hướng dẫn khắc phục sự cố nói được ĐÚNG thư mục trên máy
        // này, thay vì bắt người dùng đoán `%LOCALAPPDATA%` nghĩa là gì.
        "paths" => Ok(json!({
            "llmDir": llm_dir.display().to_string(),
            "resourceRoot": res_root.display().to_string(),
            "dataDir": crate::data_dir().display().to_string(),
            "configFile": crate::config_file_path().display().to_string(),
            "manifest": crate::resolve_resource_path(setup::MANIFEST_REL).display().to_string(),
        })),

        _ => Err(format!("Unknown setup command: {command}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nhan_dung_mien() {
        assert!(owns("setup:status"));
        assert!(owns("setup:fetch"));
        assert!(!owns("get_config"));
        assert!(!owns("chat:completion"));
    }

    /// Profile lạ phải rơi về `minimal`. Nếu không, một payload gõ sai sẽ khiến
    /// người dùng tải thêm vài GB tuỳ chọn mà không hề yêu cầu.
    #[test]
    fn profile_la_thi_ve_minimal() {
        assert_eq!(doc_profile(&json!({})), "minimal");
        assert_eq!(doc_profile(&json!({"profile": "full"})), "full");
        assert_eq!(doc_profile(&json!({"profile": "FULL"})), "minimal");
        assert_eq!(doc_profile(&json!({"profile": 7})), "minimal");
    }
}
