//! Command handlers for Living Canvas & Generative UI operations (`canvas:*`).

use crate::AppState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use tokio::sync::mpsc::Sender;

pub const COMMANDS: &[&str] = &[
    "canvas:stream_widget",
    "canvas:get_canvas_state",
    "canvas:update_widget_state",
    "canvas:close_widget",
    "canvas:set_layout",
    "canvas:clear_widgets",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasWidget {
    pub widget_id: String,
    pub title: String,
    pub component_type: String, // "generative_ui", "chart", "table", "form", "custom"
    pub html: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub css: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub js: Option<String>,
    #[serde(default)]
    pub props: Value,
    pub version: u32,
    pub interactive: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasLayout {
    pub split_ratio: f64, // 0.0 to 1.0 (default: 0.5)
    pub active_mode: String, // "diff", "canvas", "hybrid", "empty"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab: Option<String>,
}

impl Default for CanvasLayout {
    fn default() -> Self {
        Self {
            split_ratio: 0.5,
            active_mode: "hybrid".to_string(),
            active_tab: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasState {
    pub canvas_id: String,
    pub active_widgets: HashMap<String, CanvasWidget>,
    pub layout: CanvasLayout,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            canvas_id: "default_canvas".to_string(),
            active_widgets: HashMap::new(),
            layout: CanvasLayout::default(),
        }
    }
}

/// Global thread-safe state manager for Living Canvas state.
pub struct CanvasStateManager {
    state: RwLock<CanvasState>,
}

impl Default for CanvasStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasStateManager {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(CanvasState::default()),
        }
    }

    pub fn global() -> &'static Arc<CanvasStateManager> {
        static MANAGER: OnceLock<Arc<CanvasStateManager>> = OnceLock::new();
        MANAGER.get_or_init(|| Arc::new(CanvasStateManager::new()))
    }

    pub fn get_state(&self) -> CanvasState {
        let guard = self.state.read().unwrap();
        guard.clone()
    }

    pub fn upsert_widget(&self, widget: CanvasWidget) -> CanvasWidget {
        let mut guard = self.state.write().unwrap();
        let id = widget.widget_id.clone();
        guard.active_widgets.insert(id, widget.clone());
        widget
    }

    pub fn update_widget_props(&self, widget_id: &str, props: Value) -> Result<CanvasWidget, String> {
        let mut guard = self.state.write().unwrap();
        let widget = guard
            .active_widgets
            .get_mut(widget_id)
            .ok_or_else(|| format!("Widget '{}' not found", widget_id))?;

        widget.props = props;
        widget.version += 1;
        widget.updated_at = Utc::now().timestamp_millis();
        Ok(widget.clone())
    }

    pub fn remove_widget(&self, widget_id: &str) -> Option<CanvasWidget> {
        let mut guard = self.state.write().unwrap();
        guard.active_widgets.remove(widget_id)
    }

    pub fn set_layout(&self, layout: CanvasLayout) {
        let mut guard = self.state.write().unwrap();
        guard.layout = layout;
    }

    pub fn clear(&self) {
        let mut guard = self.state.write().unwrap();
        guard.active_widgets.clear();
    }
}

pub fn owns(command: &str) -> bool {
    COMMANDS.contains(&command)
}

pub async fn handle(
    _state: Arc<AppState>,
    command: &str,
    payload: Value,
    tx: Option<Sender<String>>,
    req_id: Option<String>,
) -> Result<Value, String> {
    let manager = CanvasStateManager::global();

    match command {
        "canvas:get_canvas_state" => {
            let state = manager.get_state();
            Ok(serde_json::to_value(&state)
                .map_err(|e| format!("Serialization error: {}", e))?)
        }

        "canvas:stream_widget" => {
            let widget_id = payload
                .get("widget_id")
                .and_then(|v| v.as_str())
                .unwrap_or("widget_auto")
                .to_string();

            let title = payload
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Generative UI Component")
                .to_string();

            let component_type = payload
                .get("component_type")
                .and_then(|v| v.as_str())
                .unwrap_or("generative_ui")
                .to_string();

            let html = payload
                .get("html")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required parameter 'html'".to_string())?
                .to_string();

            let css = payload
                .get("css")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let js = payload
                .get("js")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let props = payload.get("props").cloned().unwrap_or_else(|| json!({}));
            let interactive = payload
                .get("interactive")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let now = Utc::now().timestamp_millis();
            let widget = CanvasWidget {
                widget_id: widget_id.clone(),
                title,
                component_type,
                html: html.clone(),
                css: css.clone(),
                js: js.clone(),
                props: props.clone(),
                version: 1,
                interactive,
                created_at: now,
                updated_at: now,
            };

            let stored = manager.upsert_widget(widget);

            // Stream frame if IPC stream sender tx is active
            if let Some(stream_tx) = tx {
                let stream_event = json!({
                    "event": "canvas_widget_frame",
                    "req_id": req_id,
                    "widget_id": widget_id,
                    "version": stored.version,
                    "html": stored.html,
                    "css": stored.css,
                    "js": stored.js,
                    "props": stored.props,
                });

                let _ = stream_tx.send(serde_json::to_string(&stream_event).unwrap_or_default()).await;
            }

            Ok(json!({
                "status": "ok",
                "widget": stored,
            }))
        }

        "canvas:update_widget_state" => {
            let widget_id = payload
                .get("widget_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required parameter 'widget_id'".to_string())?;

            let props = payload
                .get("props")
                .cloned()
                .ok_or_else(|| "Missing required parameter 'props'".to_string())?;

            let updated = manager.update_widget_props(widget_id, props)?;
            Ok(json!({
                "status": "ok",
                "widget": updated,
            }))
        }

        "canvas:close_widget" => {
            let widget_id = payload
                .get("widget_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required parameter 'widget_id'".to_string())?;

            let removed = manager.remove_widget(widget_id);
            Ok(json!({
                "status": "ok",
                "removed": removed.is_some(),
                "widget_id": widget_id,
            }))
        }

        "canvas:set_layout" => {
            let split_ratio = payload
                .get("split_ratio")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5);

            let active_mode = payload
                .get("active_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("hybrid")
                .to_string();

            let active_tab = payload
                .get("active_tab")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let layout = CanvasLayout {
                split_ratio,
                active_mode,
                active_tab,
            };

            manager.set_layout(layout.clone());
            Ok(json!({
                "status": "ok",
                "layout": layout,
            }))
        }

        "canvas:clear_widgets" => {
            manager.clear();
            Ok(json!({
                "status": "ok",
                "cleared": true,
            }))
        }

        other => Err(format!("Unknown canvas command '{}'", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::EncryptionEngine;
    use crate::db;
    use crate::llm;
    use crate::stt;
    use crate::tts;

    fn test_state() -> Arc<AppState> {
        let db = db::DatabasePool::new_in_memory().expect("in-memory db");
        let stt_manager = stt::SttManager::new("non-existent-model");
        let llm_manager = llm::LlamaRouterManager::new(2048, 0).expect("LLM manager");
        let mock_capturer = Arc::new(crate::vision::capture::MockScreenCapturer::new(
            64,
            64,
            crate::vision::capture::PixelFormat::Rgba,
        ));

        Arc::new(AppState {
            db,
            crypto: EncryptionEngine::new("00000000000000000000000000000000"),
            stt: tokio::sync::Mutex::new(stt_manager),
            tts: tokio::sync::Mutex::new(None),
            tts_player: tts::audio::TtsAudioPlayer::new(None),
            llm: tokio::sync::Mutex::new(llm_manager),
            vad: tokio::sync::Mutex::new(None),
            denoiser: tokio::sync::Mutex::new(None),
            turn_shadow: tokio::sync::Mutex::new(None),
            aec: tokio::sync::Mutex::new(None),
            mcp_server: Arc::new(crate::mcp::server::NativeMcpServer::new("test_vault")),
            embedder: tokio::sync::Mutex::new(None),
            vision: tokio::sync::Mutex::new(crate::vision::VisionManager::new(
                mock_capturer,
                crate::vision::VisionConfig::default(),
            )),
        })
    }

    #[tokio::test]
    async fn test_canvas_stream_and_state() {
        let state = test_state();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        let res = handle(
            state.clone(),
            "canvas:stream_widget",
            json!({
                "widget_id": "widget_chart_1",
                "title": "Latency Breakdown",
                "html": "<div id='chart'></div>",
                "css": "#chart { color: purple; }",
                "props": {"latency_ms": 42}
            }),
            Some(tx),
            Some("req-stream-1".to_string()),
        )
        .await
        .expect("stream widget");

        assert_eq!(res["status"], "ok");
        assert_eq!(res["widget"]["widget_id"], "widget_chart_1");

        // Verify stream message emitted
        let stream_msg = rx.try_recv().expect("stream message received");
        assert!(stream_msg.contains("canvas_widget_frame"));
        assert!(stream_msg.contains("widget_chart_1"));

        // Verify state retrieval
        let state_res = handle(state.clone(), "canvas:get_canvas_state", json!({}), None, None)
            .await
            .expect("get canvas state");

        assert!(state_res["active_widgets"]["widget_chart_1"].is_object());
    }

    #[tokio::test]
    async fn test_canvas_update_and_close() {
        let state = test_state();
        let manager = CanvasStateManager::global();

        manager.upsert_widget(CanvasWidget {
            widget_id: "w_close".to_string(),
            title: "Temp".to_string(),
            component_type: "gen_ui".to_string(),
            html: "<div>Temp</div>".to_string(),
            css: None,
            js: None,
            props: json!({"step": 1}),
            version: 1,
            interactive: true,
            created_at: 0,
            updated_at: 0,
        });

        let update_res = handle(
            state.clone(),
            "canvas:update_widget_state",
            json!({
                "widget_id": "w_close",
                "props": {"step": 2}
            }),
            None,
            None,
        )
        .await
        .expect("update widget");

        assert_eq!(update_res["widget"]["version"], 2);

        let close_res = handle(
            state.clone(),
            "canvas:close_widget",
            json!({"widget_id": "w_close"}),
            None,
            None,
        )
        .await
        .expect("close widget");

        assert_eq!(close_res["removed"], true);
    }
}
