//! Miền lệnh `cua:*` — Desktop Automation, Window Inspection, Kill-Switch & Audit.
//!
//! Exposes native CUA capabilities to Tauri IPC with fail-closed authorization,
//! structured JSON serialization, and machine-readable error mapping.

use crate::AppState;
use liva_cua::audit::{AuditQueryFilter, CuaAuditStore, SecurityVerdict};
use liva_cua::execution::{ExecutionProfile, ProfileAction, ProfilePlan};
use liva_cua::types::{CuaAction, CuaDeliveryMode, CuaError, MouseButton, PermissionMode};
use serde_json::{Value, json};
use std::sync::Arc;

/// Maps a `CuaError` into a structured, machine-readable JSON string.
pub fn map_cua_error(err: CuaError) -> String {
    json!({
        "error": {
            "code": err.error_code(),
            "message": err.to_string(),
            "subsystem": "cua"
        }
    })
    .to_string()
}

/// Helper for invalid request parameters.
pub fn map_invalid_param(msg: &str) -> String {
    json!({
        "error": {
            "code": "invalid_parameter",
            "message": msg,
            "subsystem": "cua"
        }
    })
    .to_string()
}

/// Dispatches a CUA domain command verb.
pub async fn handle(state: Arc<AppState>, verb: &str, payload: Value) -> Result<Value, String> {
    match verb {
        "list_windows" => list_windows(state, payload).await,
        "execute_action" => execute_action(state, payload).await,
        "emergency_stop" => emergency_stop(state, payload).await,
        "set_mode" => set_mode(state, payload).await,
        "get_status" => get_status(state).await,
        "query_audit_logs" => query_audit_logs(state, payload).await,
        "inspect_ui_tree" => inspect_ui_tree(state, payload).await,
        "execute_som_action" => execute_som_action(state, payload).await,
        "run_profile" => run_profile(state, payload).await,
        "list_devices" => list_devices(state, payload).await,
        _ => Err(format!("Unknown command: cua:{verb}")),
    }
}

async fn list_windows(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let filter_pid = payload
        .get("filter_pid")
        .and_then(|v| v.as_u64())
        .map(|p| p as u32);

    let windows = state
        .cua
        .list_windows(filter_pid)
        .await
        .map_err(map_cua_error)?;

    Ok(json!({ "windows": windows }))
}

async fn execute_action(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    // Accepts either wrapped { "action": { ... } } or flattened action object
    let action_value = match payload.get("action") {
        Some(v) if v.is_object() => v.clone(),
        _ => payload,
    };
    let action: CuaAction = serde_json::from_value(action_value)
        .map_err(|e| map_invalid_param(&format!("Failed to parse CuaAction: {e}")))?;

    let result = state
        .cua
        .execute_action(action)
        .await
        .map_err(map_cua_error)?;

    Ok(json!({ "result": result }))
}

async fn emergency_stop(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    if payload
        .get("reset")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        state.cua.reset_emergency_halt().map_err(map_cua_error)?;
        return Ok(json!({
            "halted": false,
            "reset": true
        }));
    }

    let start = std::time::Instant::now();
    state.cua.trigger_emergency_halt().map_err(map_cua_error)?;
    let latency_ms = start.elapsed().as_millis() as u64;

    Ok(json!({
        "halted": true,
        "latency_ms": latency_ms
    }))
}

async fn set_mode(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let mode_str = payload
        .get("mode")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            map_invalid_param(
                "Missing 'mode' field (expected 'Standard', 'Bounded', or 'Unrestricted')",
            )
        })?;

    let mode = match mode_str.to_ascii_lowercase().as_str() {
        "standard" => PermissionMode::Standard,
        "bounded" => PermissionMode::Bounded,
        "unrestricted" => PermissionMode::Unrestricted,
        other => {
            return Err(map_invalid_param(&format!(
                "Invalid permission mode: '{other}'"
            )));
        }
    };

    state
        .cua
        .set_permission_mode(mode)
        .await
        .map_err(map_cua_error)?;

    if let Some(allowlist_arr) = payload.get("allowlist").and_then(|v| v.as_array()) {
        let new_allowlist: Vec<String> = allowlist_arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        state
            .cua
            .security_governor
            .set_allowlist(new_allowlist.clone());
        return Ok(json!({
            "mode": mode,
            "allowlist": new_allowlist,
            "updated": true
        }));
    }

    Ok(json!({
        "mode": mode,
        "updated": true
    }))
}

async fn get_status(state: Arc<AppState>) -> Result<Value, String> {
    let status = state.cua.get_status().await;
    Ok(json!({
        "mode": status.permission_mode,
        "halted": status.is_halted,
        "active_actions": status.active_actions,
        "total_actions_executed": status.total_actions_executed,
        "uptime_seconds": status.uptime_seconds
    }))
}

async fn query_audit_logs(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let limit = payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let offset = payload.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let action_type = payload
        .get("action_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let process_name = payload
        .get("process_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let verdict_str = payload
        .get("security_verdict")
        .or_else(|| payload.get("verdict"))
        .and_then(|v| v.as_str());

    let security_verdict = match verdict_str {
        Some(v) => Some(
            v.parse::<SecurityVerdict>()
                .map_err(|e| map_invalid_param(&e))?,
        ),
        None => None,
    };

    let start_timestamp_ms = payload.get("start_timestamp_ms").and_then(|v| v.as_u64());
    let end_timestamp_ms = payload.get("end_timestamp_ms").and_then(|v| v.as_u64());

    let filter = AuditQueryFilter {
        session_id,
        action_type,
        security_verdict,
        process_name,
        start_timestamp_ms,
        end_timestamp_ms,
        limit: Some(limit),
        offset: Some(offset),
    };

    // Primary: Query SQLite WAL via spawn_blocking
    let db_res = tokio::task::spawn_blocking({
        let state = state.clone();
        let filter = filter.clone();
        move || -> Result<liva_cua::audit::AuditQueryResult, String> {
            let conn = state
                .db
                .readers
                .get()
                .map_err(|e| format!("Failed to acquire reader: {e}"))?;
            CuaAuditStore::query(&conn, &filter).map_err(|e| format!("SQLite query failed: {e}"))
        }
    })
    .await;

    match db_res {
        Ok(Ok(query_res)) => Ok(json!({
            "events": query_res.events,
            "total_matched": query_res.total_matched,
            "limit": query_res.limit,
            "offset": query_res.offset,
            "source": "sqlite_wal"
        })),
        _ => {
            // Secondary Fallback: Read from lock-free RAM ring buffer
            let recent_arcs = state.cua.audit_recorder.get_recent(limit);
            let events: Vec<liva_cua::audit::CuaAuditEvent> =
                recent_arcs.into_iter().map(|arc| (*arc).clone()).collect();
            let total = events.len();
            Ok(json!({
                "events": events,
                "total_matched": total,
                "limit": limit,
                "offset": offset,
                "source": "ring_buffer"
            }))
        }
    }
}

async fn inspect_ui_tree(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let hwnd = payload
        .get("target_hwnd")
        .or_else(|| payload.get("hwnd"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let fallback_roi = payload
        .get("fallback_roi")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if fallback_roi {
        let perception = state
            .cua
            .perceive_target(hwnd, None)
            .await
            .map_err(map_cua_error)?;
        return Ok(json!({ "perception": perception }));
    }

    let tree = state
        .cua
        .inspect_ui_tree(hwnd)
        .await
        .map_err(map_cua_error)?;

    let som_elements = tree.interactive_elements();
    let som_prompt = tree.format_som_prompt();

    Ok(json!({
        "tree": tree,
        "som_elements": som_elements,
        "som_prompt": som_prompt,
        "interactive_count": tree.interactive_count,
        "total_nodes": tree.total_nodes
    }))
}

async fn execute_som_action(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let hwnd = payload
        .get("target_hwnd")
        .or_else(|| payload.get("hwnd"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let som_index = payload
        .get("som_index")
        .or_else(|| payload.get("target_som_id"))
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or_else(|| map_invalid_param("Missing 'som_index' or 'target_som_id' parameter"))?;

    let tree = state
        .cua
        .inspect_ui_tree(hwnd)
        .await
        .map_err(map_cua_error)?;

    let action_str = payload
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("click");

    let delivery_mode = payload
        .get("delivery_mode")
        .and_then(|v| serde_json::from_value::<CuaDeliveryMode>(v.clone()).ok())
        .unwrap_or(CuaDeliveryMode::Background);

    let action_template = match action_str.to_ascii_lowercase().as_str() {
        "click" => {
            let button = payload
                .get("button")
                .and_then(|v| serde_json::from_value::<MouseButton>(v.clone()).ok())
                .unwrap_or(MouseButton::Left);
            let click_count = payload
                .get("click_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as u32;
            CuaAction::Click {
                target_hwnd: hwnd,
                x: None,
                y: None,
                button,
                click_count,
                delivery_mode,
            }
        }
        "double_click" => CuaAction::Click {
            target_hwnd: hwnd,
            x: None,
            y: None,
            button: MouseButton::Left,
            click_count: 2,
            delivery_mode,
        },
        "hover" | "move" | "move_cursor" => CuaAction::MoveCursor {
            target_hwnd: hwnd,
            x: 0,
            y: 0,
            delivery_mode,
        },
        "type" | "type_text" => {
            let text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            CuaAction::TypeText {
                target_hwnd: hwnd,
                text,
                delivery_mode,
            }
        }
        "scroll" => {
            let delta_x = payload.get("delta_x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let delta_y = payload
                .get("delta_y")
                .and_then(|v| v.as_i64())
                .unwrap_or(-120) as i32;
            CuaAction::Scroll {
                target_hwnd: hwnd,
                delta_x,
                delta_y,
                x: None,
                y: None,
            }
        }
        _ => {
            if let Some(act) = payload.get("action_template").cloned() {
                serde_json::from_value::<CuaAction>(act)
                    .map_err(|e| map_invalid_param(&format!("Invalid action_template: {e}")))?
            } else {
                return Err(map_invalid_param(&format!(
                    "Unsupported action type: '{action_str}'"
                )));
            }
        }
    };

    let resolved_action = state
        .cua
        .resolve_som_action(&tree, som_index, action_template)
        .map_err(map_cua_error)?;

    let result = state
        .cua
        .execute_action(resolved_action.clone())
        .await
        .map_err(map_cua_error)?;

    Ok(json!({
        "result": result,
        "resolved_action": resolved_action,
        "som_index": som_index
    }))
}

async fn run_profile(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let profile_plan: Result<ProfilePlan, _> = if let Some(plan_val) = payload.get("plan").cloned()
    {
        serde_json::from_value(plan_val)
    } else {
        serde_json::from_value(payload.clone()).or_else(|_| {
            let profile_str = payload
                .get("profile")
                .and_then(|v| v.as_str())
                .unwrap_or("flash");
            let profile = match profile_str.to_ascii_lowercase().as_str() {
                "pro" => ExecutionProfile::Pro,
                _ => ExecutionProfile::Flash,
            };
            let plan_id = payload
                .get("plan_id")
                .and_then(|v| v.as_str())
                .unwrap_or("plan_default")
                .to_string();
            let session_id = payload
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("session_default")
                .to_string();
            let target_hwnd = payload
                .get("target_hwnd")
                .or_else(|| payload.get("hwnd"))
                .and_then(|v| v.as_u64());

            let steps: Vec<ProfileAction> = payload
                .get("steps")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let mut plan = ProfilePlan::new(plan_id, session_id, profile);
            if let Some(hwnd) = target_hwnd {
                plan = plan.with_target_hwnd(hwnd);
            }
            plan = plan.with_steps(steps);
            Ok(plan)
        })
    };

    let plan = profile_plan
        .map_err(|e| map_invalid_param(&format!("Failed to build ProfilePlan: {e}")))?;

    let result = state
        .cua
        .run_profile_plan(plan, None)
        .await
        .map_err(map_cua_error)?;

    Ok(json!({ "result": result }))
}

async fn list_devices(state: Arc<AppState>, _payload: Value) -> Result<Value, String> {
    let windows = state.cua.list_windows(None).await.unwrap_or_default();

    let mut android_devices = Vec::new();
    if let Ok(output) = tokio::process::Command::new("adb")
        .args(["devices", "-l"])
        .output()
        .await
    {
        if output.status.success() {
            let out_str = String::from_utf8_lossy(&output.stdout);
            android_devices = liva_cua::adb::parse_adb_devices(&out_str);
        }
    }

    Ok(json!({
        "windows": windows,
        "android_devices": android_devices,
        "count": {
            "windows": windows.len(),
            "android_devices": android_devices.len()
        }
    }))
}
