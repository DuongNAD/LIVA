//! Miền lệnh `consent:*` — công tắc đồng ý cho quan sát thụ động (U20 bước 1).
//!
//! Cổng lõi ở `crate::consent`; module này chỉ là lớp IPC mỏng để giao diện
//! bật/tắt và đọc trạng thái. Xem `consent.rs` để hiểu vì sao nó fail-closed và
//! vì sao nó ra đời trước phần thu thập.

use crate::AppState;
use serde_json::{Value, json};
use std::sync::Arc;

/// Đóng gói trạng thái thành JSON cho UI. `active` luôn `false` cho tới khi có
/// collector — tách bạch "đã cho phép" với "đang ghi" ngay từ hợp đồng IPC.
fn to_json(state: &crate::consent::ObservationConsent) -> Value {
    json!({
        "granted": state.granted,
        "updatedAt": state.updated_at_unix,
        "active": crate::consent::is_capture_active(),
    })
}

pub async fn handle(_state: Arc<AppState>, verb: &str, _payload: Value) -> Result<Value, String> {
    // I/O file chạy trong spawn_blocking: đọc/ghi đĩa không được chẹn runtime,
    // dù ở đây là file vài chục byte.
    match verb {
        "get" => {
            let state = tokio::task::spawn_blocking(crate::consent::load)
                .await
                .map_err(|e| format!("consent get task lỗi: {e}"))?;
            Ok(to_json(&state))
        }
        "grant" => {
            let state = tokio::task::spawn_blocking(crate::consent::grant)
                .await
                .map_err(|e| format!("consent grant task lỗi: {e}"))??;
            Ok(to_json(&state))
        }
        "revoke" => {
            let state = tokio::task::spawn_blocking(crate::consent::revoke)
                .await
                .map_err(|e| format!("consent revoke task lỗi: {e}"))??;
            Ok(to_json(&state))
        }
        _ => Err(format!("Lệnh consent không rõ: consent:{verb}")),
    }
}
