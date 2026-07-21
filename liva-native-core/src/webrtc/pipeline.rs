use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{info, warn, error};
use crate::AppState;
use crate::webrtc::frame::{VoiceFrame, OP_FLUSH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineState {
    Idle,
    VadStart,
    VadEnd,
    SttProcessing,
    LlmGenerating,
    TtsSpeaking,
    Interrupted,
}

#[derive(Debug)]
pub enum PipelineEvent {
    VadStart,
    VadEnd(Vec<f32>), // Raw audio samples
    Interrupted,
    SttCompleted {
        session_id: u64,
        result: Result<Option<String>, String>,
    },
    TtsSpeaking {
        session_id: u64,
    },
    LlmCompleted {
        session_id: u64,
        result: Result<(), String>,
    },
    TtsCompleted {
        session_id: u64,
        result: Result<(), String>,
    },
}

#[derive(Clone)]
pub struct WebRTCPipelineHandle {
    pub event_tx: mpsc::Sender<PipelineEvent>,
    pub state_rx: watch::Receiver<PipelineState>,
}

impl WebRTCPipelineHandle {
    pub fn state(&self) -> PipelineState {
        *self.state_rx.borrow()
    }

    pub fn on_vad_start(&self) -> Result<(), String> {
        self.event_tx
            .try_send(PipelineEvent::VadStart)
            .map_err(|e| format!("Failed to queue VadStart: {}", e))
    }

    pub fn on_vad_end(&self, audio_data: Vec<f32>) -> Result<(), String> {
        self.event_tx
            .try_send(PipelineEvent::VadEnd(audio_data))
            .map_err(|e| format!("Failed to queue VadEnd: {}", e))
    }

    pub fn on_interrupted(&self) -> Result<(), String> {
        self.event_tx
            .try_send(PipelineEvent::Interrupted)
            .map_err(|e| format!("Failed to queue Interrupted: {}", e))
    }

    /// Hook để nhận dữ liệu giải mã từ WebRTC RTP (PCM f32).
    /// Trong thực tế, dữ liệu này sẽ được đẩy qua channel để một VadEngine chạy ngầm xử lý
    pub fn feed_rtp_pcm(&self, _samples: &[f32]) -> Result<(), String> {
        // TODO: Pass samples to VadEngine
        // Nếu VadEngine trả về SpeechStart -> gọi on_vad_start()
        // Nếu VadEngine trả về SpeechEnd -> gọi on_vad_end()
        Ok(())
    }
}

/// Số tin nhắn tối đa giữ lại trong lịch sử hội thoại, KHÔNG kể tin `system`.
/// Đặt qua `LIVA_MAX_HISTORY_MESSAGES` (mặc định 20 ≈ 10 lượt hỏi–đáp).
///
/// Vì sao cần: trước khi khoá checkpoint được sửa, mỗi lượt nói lại dựng một
/// `AgentState` mới nên lịch sử không bao giờ dài ra — bug đó vô tình che mất
/// việc `compile_prompt` không hề cắt cửa sổ. Khi bộ nhớ đa lượt chạy thật,
/// lịch sử tích luỹ và prompt sẽ vượt `n_ctx` (mặc định 4096) sau vài chục lượt.
fn max_history_messages() -> usize {
    std::env::var("LIVA_MAX_HISTORY_MESSAGES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(20)
}

/// Cắt bớt lịch sử tại chỗ, luôn giữ lại tin `system` đầu tiên (persona) và
/// `max_history_messages()` tin gần nhất. Đây là chốt chặn tối thiểu; việc cắt
/// theo số token thật thuộc về F2.
fn trim_history(messages: &mut Vec<serde_json::Value>) {
    let cap = max_history_messages();

    let system_msg = messages
        .first()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"))
        .cloned();
    let body_start = usize::from(system_msg.is_some());

    if messages.len() - body_start <= cap {
        return;
    }

    let keep_from = messages.len() - cap;
    let tail: Vec<serde_json::Value> = messages[keep_from..].to_vec();

    messages.clear();
    if let Some(sys) = system_msg {
        messages.push(sys);
    }
    messages.extend(tail);
}

pub struct WebRTCActor {
    state: PipelineState,
    /// Token huỷ tác vụ cũ khi barge-in. TĂNG mỗi lượt VAD (xem
    /// `cancel_active_operations`) nên KHÔNG được dùng làm khoá bộ nhớ.
    session_id: u64,
    /// Khoá checkpoint hội thoại — ổn định suốt vòng đời một kết nối.
    /// Tách khỏi `session_id` vì hai thứ này có vòng đời trái ngược nhau:
    /// dùng nhầm `session_id` thì `load_checkpoint` luôn trả `None` và trợ lý
    /// mất sạch trí nhớ đa lượt.
    conversation_id: String,
    active_session_id: Arc<std::sync::atomic::AtomicU64>,
    event_rx: mpsc::Receiver<PipelineEvent>,
    event_tx: mpsc::Sender<PipelineEvent>,
    state_tx: watch::Sender<PipelineState>,

    // Background task handles
    stt_handle: Option<JoinHandle<()>>,
    llm_handle: Option<JoinHandle<()>>,
    tts_handle: Option<JoinHandle<()>>,

    state_shared: Arc<AppState>,
    outgoing_tx: mpsc::Sender<VoiceFrame>,
}

impl WebRTCActor {
    /// `conversation_id` là khoá bộ nhớ hội thoại và phải GIỮ NGUYÊN suốt kết
    /// nối. Truyền lại cùng một giá trị ở lần kết nối sau thì trợ lý nhớ được
    /// hội thoại cũ.
    pub fn new(
        state_shared: Arc<AppState>,
        outgoing_tx: mpsc::Sender<VoiceFrame>,
        conversation_id: String,
    ) -> (WebRTCPipelineHandle, Self) {
        let (event_tx, event_rx) = mpsc::channel(128);
        let (state_tx, state_rx) = watch::channel(PipelineState::Idle);

        let handle = WebRTCPipelineHandle {
            event_tx: event_tx.clone(),
            state_rx,
        };

        let actor = Self {
            state: PipelineState::Idle,
            session_id: 0,
            conversation_id,
            active_session_id: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            event_rx,
            event_tx,
            state_tx,
            stt_handle: None,
            llm_handle: None,
            tts_handle: None,
            state_shared,
            outgoing_tx,
        };

        (handle, actor)
    }

    pub async fn run(mut self) {
        info!("WebRTCActor control loop started.");
        while let Some(event) = self.event_rx.recv().await {
            match event {
                PipelineEvent::VadStart => {
                    self.handle_vad_start().await;
                }
                PipelineEvent::VadEnd(audio_data) => {
                    self.handle_vad_end(audio_data).await;
                }
                PipelineEvent::Interrupted => {
                    self.handle_interrupted().await;
                }
                PipelineEvent::SttCompleted { session_id, result } => {
                    self.handle_stt_completed(session_id, result).await;
                }
                PipelineEvent::TtsSpeaking { session_id } => {
                    self.handle_tts_speaking(session_id).await;
                }
                PipelineEvent::LlmCompleted { session_id, result } => {
                    self.handle_llm_completed(session_id, result).await;
                }
                PipelineEvent::TtsCompleted { session_id, result } => {
                    self.handle_tts_completed(session_id, result).await;
                }
            }
        }
        info!("WebRTCActor control loop stopped.");
    }

    fn transition_to(&mut self, new_state: PipelineState) {
        let old = self.state;
        self.state = new_state;
        let _ = self.state_tx.send(new_state);
        info!("🔄 [State Transition] {:?} ➡️ {:?}", old, new_state);
    }

    async fn handle_vad_start(&mut self) {
        info!("🎙️ [VAD] Speech START detected.");
        self.cancel_active_operations().await;
        self.transition_to(PipelineState::VadStart);
    }

    async fn handle_vad_end(&mut self, audio_data: Vec<f32>) {
        info!("🎙️ [VAD] Speech END detected. Processing audio...");
        self.cancel_active_operations().await;
        self.transition_to(PipelineState::VadEnd);
        self.transition_to(PipelineState::SttProcessing);

        let session_id = self.session_id;
        let state_shared = Arc::clone(&self.state_shared);
        let event_tx = self.event_tx.clone();
        let active_session_id_stt = Arc::clone(&self.active_session_id);

        let handle = tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                if active_session_id_stt.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("STT cancelled before start".to_string());
                }
                let mut manager = state_shared.stt.blocking_lock();
                if active_session_id_stt.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("STT cancelled post-lock".to_string());
                }
                manager.feed_audio(&audio_data, true)
            })
            .await
            .map_err(|e| format!("STT task panicked: {}", e))
            .and_then(|r| r);

            let _ = event_tx.send(PipelineEvent::SttCompleted { session_id, result }).await;
        });

        self.stt_handle = Some(handle);
    }

    async fn handle_interrupted(&mut self) {
        info!("🎙️ [Pipeline] Interruption requested.");
        self.cancel_active_operations().await;
        self.transition_to(PipelineState::Interrupted);
        self.transition_to(PipelineState::Idle);
    }

    async fn handle_stt_completed(&mut self, session_id: u64, result: Result<Option<String>, String>) {
        if session_id != self.session_id {
            warn!("[STT] Discarding STT result for stale session {}", session_id);
            return;
        }

        match result {
            Ok(Some(text)) if !text.trim().is_empty() => {
                info!("🎙️ [Pipeline] Transcribed: '{}'", text);
                self.transition_to(PipelineState::LlmGenerating);
                self.spawn_llm_and_tts(text).await;
            }
            Ok(_) => {
                info!("[STT] Completed empty transcript. Standing by.");
                self.transition_to(PipelineState::Idle);
            }
            Err(e) => {
                error!("[STT] Error: {}. Returning to Idle.", e);
                self.transition_to(PipelineState::Idle);
            }
        }
    }

    async fn spawn_llm_and_tts(&mut self, text: String) {
        let session_id = self.session_id;
        let conversation_id = self.conversation_id.clone();
        let event_tx = self.event_tx.clone();
        let state_clone = Arc::clone(&self.state_shared);
        let outgoing_tx_clone = self.outgoing_tx.clone();
        let active_session_id_llm = Arc::clone(&self.active_session_id);
        let active_session_id_tts = Arc::clone(&self.active_session_id);

        let (llm_chunk_tx, mut llm_chunk_rx) = mpsc::channel::<String>(100);

        // Spawn LLM Task
        let state_llm = Arc::clone(&state_clone);
        let event_tx_llm = event_tx.clone();
        let active_session_id_llm_task = Arc::clone(&active_session_id_llm);
        let llm_handle = tokio::spawn(async move {
            let checkpointer = crate::agent::memory::SqliteCheckpointer::new(Arc::new(state_llm.db.clone()));
            // Khoá là conversation_id, KHÔNG phải session_id: session_id tăng ở
            // mỗi sự kiện VAD nên dùng nó thì không bao giờ đọc lại được gì.
            let thread_id = conversation_id;

            // Load existing checkpoint
            let loaded = checkpointer.load_checkpoint(&thread_id).await;
            let state = match loaded {
                Ok(Some(mut st)) => {
                    st.messages.push(serde_json::json!({"role": "user", "content": text}));
                    trim_history(&mut st.messages);
                    st.current_node = "router".to_string();
                    st
                }
                _ => {
                    crate::agent::state::AgentState {
                        messages: vec![
                            serde_json::json!({"role": "system", "content": crate::llm::persona::PERSONA_LIVA}),
                            serde_json::json!({"role": "user", "content": text}),
                        ],
                        current_node: "router".to_string(),
                        context: std::collections::HashMap::new(),
                    }
                }
            };

            // Build and run the graph
            let graph = crate::agent::graph::build_pipeline_graph(
                Arc::clone(&state_llm),
                llm_chunk_tx,
                session_id,
                Arc::clone(&active_session_id_llm_task),
            );

            let run_res = graph.run(state).await;

            let result = match run_res {
                Ok(final_state) => {
                    let save_res = checkpointer.save_checkpoint(&thread_id, &final_state).await;
                    if let Err(e) = save_res {
                        error!("Failed to save checkpoint: {}", e);
                    }
                    Ok(())
                }
                Err(e) => {
                    error!("Graph run error: {}", e);
                    Err(e)
                }
            };

            let _ = event_tx_llm.send(PipelineEvent::LlmCompleted { session_id, result }).await;
        });
        self.llm_handle = Some(llm_handle);

        // Spawn TTS Task
        let state_tts = Arc::clone(&state_clone);
        let event_tx_tts = event_tx.clone();
        let tts_handle = tokio::task::spawn_blocking(move || {
            let mut chunker = crate::tts::TtsChunker::new();
            let mut seq_id = 0u32;
            let mut is_speaking = false;

            let mut process_and_send_chunk = |chunk: &str| -> Result<(), String> {
                if active_session_id_tts.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("Session cancelled".to_string());
                }
                let cleaned_chunk = chunk.replace(|c: char| c == '[' || c == ']', "");
                if cleaned_chunk.trim().is_empty() {
                    return Ok(());
                }

                // Normalize digits/dates/currency to words, then route to the
                // Piper voice per chunk language (vi/en), Kokoro as fallback.
                let (cleaned_chunk, vieneu_voice, piper_voice) = {
                    let tts_opt = state_tts.tts.blocking_lock();
                    let tts_mgr = tts_opt.as_ref().ok_or("TTS manager not initialized")?;
                    let lang = if crate::tts::is_vietnamese_text(&cleaned_chunk) {
                        "vi"
                    } else {
                        tts_mgr.language()
                    };
                    let normalized = crate::tts::normalizer::normalize(&cleaned_chunk, lang);
                    let vieneu = tts_mgr.vieneu_for_chunk(&normalized);
                    let voice = tts_mgr.piper_for_chunk(&normalized);
                    (normalized, vieneu, voice)
                };

                if active_session_id_tts.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("Session cancelled".to_string());
                }
                let (audio_samples, sample_rate) = if let Some(engine) = vieneu_voice {
                    // Premium tier: bilingual VieNeu-TTS (autoregressive, slower).
                    let mut e = engine.lock().unwrap();
                    let rate = e.sample_rate();
                    (e.synthesize(&cleaned_chunk)?, rate)
                } else if let Some(voice) = piper_voice {
                    let mut v = voice.lock().unwrap();
                    let rate = v.sample_rate();
                    (v.synthesize(&cleaned_chunk)?, rate)
                } else {
                    let phonemes = crate::tts::g2p::G2p::phonemize(&cleaned_chunk);
                    let (token_ids, engine) = {
                        let tts_opt = state_tts.tts.blocking_lock();
                        let tts_mgr = tts_opt.as_ref().ok_or("TTS manager not initialized")?;
                        (tts_mgr.tokenizer.tokenize(&phonemes), tts_mgr.engine.clone())
                    };
                    if active_session_id_tts.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                        return Err("Session cancelled".to_string());
                    }
                    let samples = {
                        let mut eng = engine.lock().unwrap();
                        eng.generate(&token_ids, 1.0)
                    }?;
                    (samples, 24000u32)
                };

                if active_session_id_tts.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("Session cancelled post-inference".to_string());
                }

                // Feed the AEC (opt-in, off by default) the same audio we're
                // about to play, so it can cancel LIVA's own voice back out
                // of the mic during barge-in — see webrtc::aec module docs.
                if let Some(ref mut aec) = *state_tts.aec.blocking_lock() {
                    aec.push_render(&audio_samples, sample_rate);
                }

                if !is_speaking {
                    is_speaking = true;
                    let _ = event_tx_tts.blocking_send(PipelineEvent::TtsSpeaking { session_id });
                }

                // OP_SPEAKER_OUT payload contract: [u32 LE sample_rate][f32 LE PCM…]
                let raw_bytes: &[u8] = bytemuck::cast_slice(&audio_samples);
                let mut payload = Vec::with_capacity(4 + raw_bytes.len());
                payload.extend_from_slice(&sample_rate.to_le_bytes());
                payload.extend_from_slice(raw_bytes);
                let frame = VoiceFrame {
                    op_code: crate::webrtc::frame::OP_SPEAKER_OUT,
                    seq_id,
                    payload: bytes::Bytes::from(payload),
                };
                seq_id += 1;
                let _ = outgoing_tx_clone.blocking_send(frame);
                Ok(())
            };

            let mut run_tts = || -> Result<(), String> {
                while let Some(token) = llm_chunk_rx.blocking_recv() {
                    if active_session_id_tts.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                        return Err("Session cancelled".to_string());
                    }
                    let chunks = chunker.push(&token);
                    for chunk in chunks {
                        process_and_send_chunk(&chunk)?;
                    }
                }
                if let Some(remainder) = chunker.flush() {
                    process_and_send_chunk(&remainder)?;
                }
                Ok(())
            };

            let result = run_tts();
            let _ = event_tx_tts.blocking_send(PipelineEvent::TtsCompleted { session_id, result });
        });
        self.tts_handle = Some(tts_handle);
    }

    async fn handle_tts_speaking(&mut self, session_id: u64) {
        if session_id != self.session_id { return; }
        if self.state == PipelineState::LlmGenerating {
            self.transition_to(PipelineState::TtsSpeaking);
        }
    }

    async fn handle_llm_completed(&mut self, session_id: u64, result: Result<(), String>) {
        if session_id != self.session_id { return; }
        if let Err(e) = result {
            error!("[LLM] Error: {}", e);
            self.cancel_active_operations().await;
            self.transition_to(PipelineState::Idle);
        }
    }

    async fn handle_tts_completed(&mut self, session_id: u64, result: Result<(), String>) {
        if session_id != self.session_id { return; }
        if let Err(e) = result {
            error!("[TTS] Error: {}", e);
        }
        self.transition_to(PipelineState::Idle);
    }

    async fn cancel_active_operations(&mut self) {
        self.session_id += 1;
        self.active_session_id.store(self.session_id, std::sync::atomic::Ordering::SeqCst);

        if let Some(h) = self.stt_handle.take() {
            h.abort();
        }
        if let Some(h) = self.llm_handle.take() {
            h.abort();
        }
        if let Some(h) = self.tts_handle.take() {
            h.abort();
        }

        self.state_shared.tts_player.stop().await;

        let flush_frame = VoiceFrame {
            op_code: OP_FLUSH,
            seq_id: 0,
            payload: bytes::Bytes::new(),
        };
        let _ = self.outgoing_tx.send(flush_frame).await;
    }
}

impl Drop for WebRTCActor {
    fn drop(&mut self) {
        if let Some(h) = self.stt_handle.take() {
            h.abort();
        }
        if let Some(h) = self.llm_handle.take() {
            h.abort();
        }
        if let Some(h) = self.tts_handle.take() {
            h.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msgs(n: usize, with_system: bool) -> Vec<serde_json::Value> {
        let mut v = Vec::new();
        if with_system {
            v.push(json!({"role": "system", "content": "persona"}));
        }
        for i in 0..n {
            v.push(json!({"role": "user", "content": format!("m{}", i)}));
        }
        v
    }

    #[test]
    fn trim_history_giu_nguyen_khi_chua_vuot_nguong() {
        let mut v = msgs(5, true);
        let before = v.clone();
        trim_history(&mut v);
        assert_eq!(v, before, "dưới ngưỡng thì không được đụng vào");
    }

    #[test]
    fn trim_history_cat_bot_va_giu_system() {
        let mut v = msgs(50, true);
        trim_history(&mut v);
        // 1 system + 20 tin gần nhất
        assert_eq!(v.len(), 21);
        assert_eq!(v[0]["role"], "system", "tin system phải được giữ lại");
        assert_eq!(v[1]["content"], "m30", "phải giữ đúng 20 tin CUỐI");
        assert_eq!(v[20]["content"], "m49", "tin mới nhất không được mất");
    }

    #[test]
    fn trim_history_khong_co_system() {
        let mut v = msgs(50, false);
        trim_history(&mut v);
        assert_eq!(v.len(), 20);
        assert_eq!(v[0]["content"], "m30");
        assert_eq!(v[19]["content"], "m49");
    }

    #[test]
    fn trim_history_dung_ngay_tai_nguong() {
        let mut v = msgs(20, true);
        let before = v.clone();
        trim_history(&mut v);
        assert_eq!(v, before, "đúng bằng ngưỡng thì chưa cắt");

        let mut v = msgs(21, true);
        trim_history(&mut v);
        assert_eq!(v.len(), 21, "vượt 1 tin thì cắt còn system + 20");
        assert_eq!(v[0]["role"], "system");
        assert_eq!(v[1]["content"], "m1", "tin cũ nhất bị loại là m0");
    }

    #[test]
    fn trim_history_rong_va_chi_co_system() {
        let mut v: Vec<serde_json::Value> = Vec::new();
        trim_history(&mut v);
        assert!(v.is_empty(), "danh sách rỗng không được panic");

        let mut v = msgs(0, true);
        trim_history(&mut v);
        assert_eq!(v.len(), 1, "chỉ có system thì giữ nguyên");
    }
}
