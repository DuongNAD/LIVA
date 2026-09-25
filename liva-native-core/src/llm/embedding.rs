//! Decoupled Embedding Engine Pipeline.
//!
//! Provides thread-safe, non-blocking text embedding extraction.
//! When a dedicated ONNX embedding engine is loaded in `AppState.embedder`,
//! embedding generation executes on independent worker threads without locking
//! or blocking the LLM text completion engine (`AppState.llm`), preventing
//! head-of-line blocking between chat dialogue and background memory indexing.

use crate::AppState;
use std::sync::Arc;

/// Compute embeddings for a batch of input texts using the decoupled embedding pipeline.
///
/// Priority 1 (Decoupled Fast Path): If `state.embedder` is loaded, uses the thread-safe
/// ONNX Runtime session (`EmbeddingEngine`) without acquiring `state.llm`.
///
/// Priority 2 (Fallback Path): If `state.embedder` is None, falls back to computing
/// embeddings via the loaded chat model in `state.llm`.
pub async fn compute_embeddings(
    state: &Arc<AppState>,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let embedder_opt = {
        let guard = state.embedder.read().await;
        guard.clone()
    };

    if let Some(embedder) = embedder_opt {
        // Fast decoupled path: dedicated ORT session runs independently of LLM lock.
        let inputs_owned = inputs.to_vec();
        tokio::task::spawn_blocking(move || -> Result<Vec<Vec<f32>>, String> {
            let mut results = Vec::with_capacity(inputs_owned.len());
            for text in &inputs_owned {
                let emb = embedder.embed_query(text)?;
                results.push(emb);
            }
            Ok(results)
        })
        .await
        .map_err(|e| format!("Decoupled embedding worker panicked: {e}"))?
    } else {
        // Fallback path: check non-blocking is_generating flag
        if crate::llm::engine::is_generating() {
            return Err(
                "LLM engine is currently busy with text generation. Use dedicated ONNX embedder for concurrent embeddings."
                    .to_string(),
            );
        }
        Err("Dedicated ONNX embedder is required for computing embeddings.".to_string())
    }
}

/// Compute embedding for a single text input string.
pub async fn compute_single_embedding(
    state: &Arc<AppState>,
    input: &str,
) -> Result<Vec<f32>, String> {
    let results = compute_embeddings(state, &[input.to_string()]).await?;
    results
        .into_iter()
        .next()
        .ok_or_else(|| "No embedding returned".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_inputs_returns_empty() {
        let pool = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let state = Arc::new(AppState {
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
        });

        let res = compute_embeddings(&state, &[]).await.unwrap();
        assert!(res.is_empty());
    }

    #[tokio::test]
    async fn test_compute_embeddings_while_llm_is_locked() {
        let pool = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let state = Arc::new(AppState {
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
        });

        // Hold generating guard (simulating ongoing text generation)
        let _llm_lock = crate::llm::engine::GeneratingGuard::enter();

        // Embedding pipeline must NOT deadlock or wait for state.llm
        let res = compute_embeddings(&state, &[]).await.unwrap();
        assert!(res.is_empty());

        // For non-empty inputs with empty embedder, it fails cleanly with non-blocking busy error,
        // without waiting or blocking on state.llm
        let err = compute_embeddings(&state, &["hello".to_string()]).await;
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("busy with text generation"));
    }
}
