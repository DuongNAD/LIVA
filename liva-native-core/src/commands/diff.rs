//! Command handlers for Diff Reviewer operations (`diff:*`, `agent:submit_hunk_decision`).

use crate::AppState;
use crate::agent::graph::diff_reviewer::{
    DiffReviewRegistry, HunkStatus, evaluate_session_decision, parse_unified_diff,
};
use serde_json::{Value, json};
use std::sync::Arc;

pub const COMMANDS: &[&str] = &[
    "diff:get_pending_hunks",
    "diff:parse_raw_diff",
    "diff:get_session",
    "diff:list_sessions",
    "diff:submit_hunk_decision",
    "agent:submit_hunk_decision",
];

pub fn owns(command: &str) -> bool {
    COMMANDS.contains(&command)
}

pub async fn handle(
    _state: Arc<AppState>,
    command: &str,
    payload: Value,
) -> Result<Value, String> {
    let registry = DiffReviewRegistry::global();

    match command {
        "diff:get_pending_hunks" => {
            let session_id = payload.get("session_id").and_then(|v| v.as_str());
            if let Some(sid) = session_id {
                let session = registry
                    .get_session(sid)
                    .ok_or_else(|| format!("Diff session '{}' not found", sid))?;
                Ok(serde_json::to_value(&session)
                    .map_err(|e| format!("Serialization error: {}", e))?)
            } else {
                let pending = registry.list_pending();
                Ok(serde_json::to_value(&pending)
                    .map_err(|e| format!("Serialization error: {}", e))?)
            }
        }

        "diff:get_session" => {
            let session_id = payload
                .get("session_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required parameter 'session_id'".to_string())?;

            let session = registry
                .get_session(session_id)
                .ok_or_else(|| format!("Diff session '{}' not found", session_id))?;
            Ok(serde_json::to_value(&session)
                .map_err(|e| format!("Serialization error: {}", e))?)
        }

        "diff:list_sessions" => {
            let sessions = registry.list_sessions();
            Ok(serde_json::to_value(&sessions)
                .map_err(|e| format!("Serialization error: {}", e))?)
        }

        "diff:parse_raw_diff" => {
            let raw_diff = payload
                .get("raw_diff")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required parameter 'raw_diff'".to_string())?;

            let files = parse_unified_diff(raw_diff)?;
            let total_hunks: usize = files.iter().map(|f| f.hunks.len()).sum();

            Ok(json!({
                "files": files,
                "total_hunks": total_hunks,
                "files_count": files.len(),
            }))
        }

        "agent:submit_hunk_decision" | "diff:submit_hunk_decision" => {
            let session_id = payload
                .get("session_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required parameter 'session_id'".to_string())?;

            // Check if batch decision is requested
            if let Some(batch) = payload.get("batch").and_then(|v| v.as_str()) {
                let session = registry.submit_batch_decisions(session_id, batch)?;
                let is_decided = session.is_fully_decided();
                let approval_decision = if is_decided {
                    evaluate_session_decision(&session)
                } else {
                    None
                };

                return Ok(json!({
                    "status": "ok",
                    "session_id": session_id,
                    "session": session,
                    "is_fully_decided": is_decided,
                    "approval_decision": approval_decision,
                }));
            }

            let hunk_id = payload
                .get("hunk_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing required parameter 'hunk_id'".to_string())?;

            let decision_str = payload
                .get("decision")
                .and_then(|v| v.as_str())
                .unwrap_or("approved");

            let status = match decision_str.to_lowercase().as_str() {
                "approved" | "approve" => HunkStatus::Approved,
                "rejected" | "reject" => {
                    let reason = payload
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    HunkStatus::Rejected { reason }
                }
                "modified" | "modify" | "edited" => {
                    let user_override = payload
                        .get("custom_content")
                        .or_else(|| payload.get("user_override"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            "Missing 'custom_content' for modified decision".to_string()
                        })?
                        .to_string();
                    HunkStatus::Modified { user_override }
                }
                other => {
                    return Err(format!("Unknown decision type '{}'", other));
                }
            };

            let updated_session = registry.submit_decision(session_id, hunk_id, status)?;
            let is_decided = updated_session.is_fully_decided();
            let approval_decision = if is_decided {
                evaluate_session_decision(&updated_session)
            } else {
                None
            };

            Ok(json!({
                "status": "ok",
                "session_id": session_id,
                "hunk_id": hunk_id,
                "session": updated_session,
                "is_fully_decided": is_decided,
                "approval_decision": approval_decision,
            }))
        }

        other => Err(format!("Unknown diff command '{}'", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::graph::diff_reviewer::DiffReviewSession;
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
    async fn test_diff_parse_command() {
        let state = test_state();
        let raw = "--- a/test.txt\n+++ b/test.txt\n@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n";
        let res = handle(
            state,
            "diff:parse_raw_diff",
            json!({"raw_diff": raw}),
        )
        .await
        .expect("parse command");

        assert_eq!(res["total_hunks"], 1);
        assert_eq!(res["files_count"], 1);
    }

    #[tokio::test]
    async fn test_submit_hunk_decision_flow() {
        let state = test_state();
        let raw = "--- a/test.txt\n+++ b/test.txt\n@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n";
        let files = parse_unified_diff(raw).unwrap();
        let session = DiffReviewSession::new("test-sess-1", "th-1", "act-1", files);

        DiffReviewRegistry::global().create_session(session);

        let hunk_id = "hunk-test_txt-1";
        let res = handle(
            state.clone(),
            "agent:submit_hunk_decision",
            json!({
                "session_id": "test-sess-1",
                "hunk_id": hunk_id,
                "decision": "approved"
            }),
        )
        .await
        .expect("submit decision");

        assert_eq!(res["status"], "ok");
        assert_eq!(res["is_fully_decided"], true);
        assert!(res["approval_decision"].is_object());
    }
}
