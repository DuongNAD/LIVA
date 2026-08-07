use crate::AppState;
use crate::webrtc::frame::{OP_FLUSH, VoiceFrame, speaker_frames};
use crate::webrtc::session::SessionAec;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

fn push_tts_token(
    filter: &mut crate::tts::AvatarSpeechFilter,
    chunker: &mut crate::tts::TtsChunker,
    token: &str,
) -> Vec<String> {
    let spoken = filter.push(token);
    if spoken.is_empty() {
        Vec::new()
    } else {
        chunker.push(&spoken)
    }
}

#[derive(Clone)]
pub struct VoiceOutbound {
    speaker_tx: mpsc::Sender<VoiceFrame>,
    control_tx: mpsc::Sender<VoiceFrame>,
    text_event_tx: Option<mpsc::Sender<String>>,
}

impl VoiceOutbound {
    pub fn new(speaker_tx: mpsc::Sender<VoiceFrame>, control_tx: mpsc::Sender<VoiceFrame>) -> Self {
        Self {
            speaker_tx,
            control_tx,
            text_event_tx: None,
        }
    }

    pub fn with_text_events(mut self, text_event_tx: mpsc::Sender<String>) -> Self {
        self.text_event_tx = Some(text_event_tx);
        self
    }

    /// Fail fast on backpressure and check cancellation around the enqueue side effect.
    fn blocking_send_speaker_if_current(
        &self,
        active_session_id: &std::sync::atomic::AtomicU64,
        session_id: u64,
        frame: VoiceFrame,
    ) -> Result<(), String> {
        if active_session_id.load(std::sync::atomic::Ordering::SeqCst) != session_id {
            return Err("Session cancelled before speaker enqueue".to_string());
        }
        let permit = self.speaker_tx.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => {
                "Speaker output queue full; aborting real-time turn".to_string()
            }
            mpsc::error::TrySendError::Closed(_) => "Speaker output channel closed".to_string(),
        })?;
        if active_session_id.load(std::sync::atomic::Ordering::SeqCst) != session_id {
            return Err("Session cancelled before speaker enqueue".to_string());
        }
        permit.send(frame);
        Ok(())
    }

    async fn send_control(&self, frame: VoiceFrame) -> Result<(), String> {
        self.control_tx
            .send(frame)
            .await
            .map_err(|_| "Voice control channel closed".to_string())
    }
}

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
    SpeakText(String),
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

    pub fn speak_text(&self, text: String) -> Result<(), String> {
        if text.trim().is_empty() {
            return Err("Không thể phát câu TTS rỗng".to_string());
        }
        self.event_tx
            .try_send(PipelineEvent::SpeakText(text))
            .map_err(|e| format!("Failed to queue SpeakText: {e}"))
    }
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
    outgoing: VoiceOutbound,
    session_aec: SessionAec,
}

impl WebRTCActor {
    /// `conversation_id` là khoá bộ nhớ hội thoại và phải GIỮ NGUYÊN suốt kết
    /// nối. Truyền lại cùng một giá trị ở lần kết nối sau thì trợ lý nhớ được
    /// hội thoại cũ.
    pub fn new(
        state_shared: Arc<AppState>,
        outgoing: VoiceOutbound,
        conversation_id: String,
        session_aec: SessionAec,
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
            outgoing,
            session_aec,
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
                PipelineEvent::SpeakText(text) => {
                    self.handle_speak_text(text).await;
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

            let _ = event_tx
                .send(PipelineEvent::SttCompleted { session_id, result })
                .await;
        });

        self.stt_handle = Some(handle);
    }

    async fn handle_interrupted(&mut self) {
        info!("🎙️ [Pipeline] Interruption requested.");
        self.cancel_active_operations().await;
        self.transition_to(PipelineState::Interrupted);
        self.transition_to(PipelineState::Idle);
    }

    async fn handle_speak_text(&mut self, text: String) {
        self.cancel_active_operations().await;
        self.transition_to(PipelineState::LlmGenerating);

        let session_id = self.session_id;
        let (text_tx, text_rx) = mpsc::channel(1);
        if text_tx.send(text).await.is_err() {
            self.transition_to(PipelineState::Idle);
            return;
        }
        drop(text_tx);
        self.spawn_tts_receiver(session_id, text_rx);
    }

    async fn handle_stt_completed(
        &mut self,
        session_id: u64,
        result: Result<Option<String>, String>,
    ) {
        if session_id != self.session_id {
            warn!(
                "[STT] Discarding STT result for stale session {}",
                session_id
            );
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
        let memory_scope =
            crate::agent::graph::ConversationMemoryScope::new("local", &conversation_id)
                .expect("WebSocket conversation id must be valid");
        let event_tx = self.event_tx.clone();
        let state_clone = Arc::clone(&self.state_shared);
        let active_session_id_llm = Arc::clone(&self.active_session_id);
        let tool_event_tx = self.outgoing.text_event_tx.clone();

        let (llm_chunk_tx, llm_chunk_rx) = mpsc::channel::<String>(100);

        // Spawn LLM Task
        let state_llm = Arc::clone(&state_clone);
        let event_tx_llm = event_tx.clone();
        let active_session_id_llm_task = Arc::clone(&active_session_id_llm);
        let llm_handle = tokio::spawn(async move {
            let checkpointer = crate::agent::memory::SqliteCheckpointer::new(
                Arc::new(state_llm.db.clone()),
                state_llm.crypto.clone(),
            );
            // Khoá là conversation_id, KHÔNG phải session_id: session_id tăng ở
            // mỗi sự kiện VAD nên dùng nó thì không bao giờ đọc lại được gì.
            let thread_id = conversation_id;

            // Load existing checkpoint
            let loaded = checkpointer.load_checkpoint(&thread_id).await;
            let state = match loaded {
                Ok(Some(mut st)) => {
                    st.messages
                        .push(serde_json::json!({"role": "user", "content": text}));
                    crate::agent::state::trim_messages(&mut st.messages);
                    st.current_node = "router".to_string();
                    st
                }
                _ => crate::agent::state::AgentState {
                    messages: vec![
                        serde_json::json!({"role": "system", "content": crate::llm::persona::PERSONA_LIVA}),
                        serde_json::json!({"role": "user", "content": text}),
                    ],
                    current_node: "router".to_string(),
                    context: std::collections::HashMap::new(),
                },
            };

            // Build and run the graph
            let graph = crate::agent::graph::build_pipeline_graph(
                Arc::clone(&state_llm),
                memory_scope,
                llm_chunk_tx,
                tool_event_tx,
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

            let _ = event_tx_llm
                .send(PipelineEvent::LlmCompleted { session_id, result })
                .await;
        });
        self.llm_handle = Some(llm_handle);

        self.spawn_tts_receiver(session_id, llm_chunk_rx);
    }

    fn spawn_tts_receiver(&mut self, session_id: u64, mut text_rx: mpsc::Receiver<String>) {
        // Spawn TTS Task
        let state_tts = Arc::clone(&self.state_shared);
        let event_tx_tts = self.event_tx.clone();
        let outgoing = self.outgoing.clone();
        let active_session_id_tts = Arc::clone(&self.active_session_id);
        let session_aec = Arc::clone(&self.session_aec);
        let tts_handle = tokio::task::spawn_blocking(move || {
            let mut chunker = crate::tts::TtsChunker::new();
            let mut avatar_speech_filter = crate::tts::AvatarSpeechFilter::default();
            let mut seq_id = 0u32;
            let mut is_speaking = false;

            let mut process_and_send_chunk = |chunk: &str| -> Result<(), String> {
                if active_session_id_tts.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("Session cancelled".to_string());
                }
                let plan = {
                    let tts_opt = state_tts.tts.blocking_lock();
                    let tts_mgr = tts_opt.as_ref().ok_or("TTS manager not initialized")?;
                    tts_mgr.synthesis_plan(chunk)
                };
                let Some(plan) = plan else {
                    return Ok(());
                };
                let outcome = plan.synthesize(|| {
                    active_session_id_tts.load(std::sync::atomic::Ordering::SeqCst) != session_id
                })?;
                let audio_samples = outcome.samples;
                let sample_rate = outcome.sample_rate;

                if active_session_id_tts.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("Session cancelled post-inference".to_string());
                }
                info!(
                    turn_epoch = session_id,
                    backend = outcome.backend.as_str(),
                    fallback_count = outcome.fallback_count,
                    sample_rate,
                    "TTS clause synthesized for voice stream"
                );

                // Feed the AEC (opt-in, off by default) the same audio we're
                // about to play, so it can cancel LIVA's own voice back out
                // of the mic during barge-in — see webrtc::aec module docs.
                let mut aec_guard = session_aec
                    .lock()
                    .map_err(|_| "WebSocket AEC mutex poisoned".to_string())?;
                if let Some(ref mut aec) = *aec_guard {
                    aec.push_render(&audio_samples, sample_rate);
                }

                if !is_speaking {
                    is_speaking = true;
                    let _ = event_tx_tts.blocking_send(PipelineEvent::TtsSpeaking { session_id });
                }

                // OP_SPEAKER_OUT payload: [turn_epoch u32 LE][sample_rate u32 LE][f32 LE PCM…]
                for mut frame in speaker_frames(session_id as u32, sample_rate, &audio_samples) {
                    frame.seq_id = seq_id;
                    seq_id = seq_id.wrapping_add(1);
                    outgoing.blocking_send_speaker_if_current(
                        &active_session_id_tts,
                        session_id,
                        frame,
                    )?;
                }
                Ok(())
            };

            let mut run_tts = || -> Result<(), String> {
                while let Some(token) = text_rx.blocking_recv() {
                    if active_session_id_tts.load(std::sync::atomic::Ordering::SeqCst) != session_id
                    {
                        return Err("Session cancelled".to_string());
                    }
                    let chunks = push_tts_token(&mut avatar_speech_filter, &mut chunker, &token);
                    for chunk in chunks {
                        process_and_send_chunk(&chunk)?;
                    }
                }
                avatar_speech_filter.finish();
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
        if session_id != self.session_id {
            return;
        }
        if self.state == PipelineState::LlmGenerating {
            self.transition_to(PipelineState::TtsSpeaking);
        }
    }

    async fn handle_llm_completed(&mut self, session_id: u64, result: Result<(), String>) {
        if session_id != self.session_id {
            return;
        }
        if let Err(e) = result {
            error!("[LLM] Error: {}", e);
            self.cancel_active_operations().await;
            self.transition_to(PipelineState::Idle);
        }
    }

    async fn handle_tts_completed(&mut self, session_id: u64, result: Result<(), String>) {
        if session_id != self.session_id {
            return;
        }
        if let Err(e) = result {
            error!("[TTS] Error: {}", e);
        }
        self.transition_to(PipelineState::Idle);
    }

    async fn cancel_active_operations(&mut self) {
        self.session_id += 1;
        self.active_session_id
            .store(self.session_id, std::sync::atomic::Ordering::SeqCst);

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
            seq_id: self.session_id as u32,
            payload: bytes::Bytes::new(),
        };
        let _ = self.outgoing.send_control(flush_frame).await;
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
mod outbound_tests {
    use super::*;
    use crate::webrtc::frame::{OP_FLUSH, OP_SPEAKER_OUT};
    use bytes::Bytes;
    use std::sync::atomic::AtomicU64;
    use std::time::Duration;

    fn frame(op_code: u8) -> VoiceFrame {
        VoiceFrame {
            op_code,
            seq_id: 0,
            payload: Bytes::new(),
        }
    }

    #[test]
    fn cau_tra_loi_truc_tiep_duoc_xep_vao_actor_tts() {
        let (event_tx, mut event_rx) = mpsc::channel(1);
        let (_state_tx, state_rx) = watch::channel(PipelineState::Idle);
        let handle = WebRTCPipelineHandle { event_tx, state_rx };

        handle
            .speak_text("Bạn muốn nhắn bằng Messenger hay Telegram?".to_string())
            .unwrap();

        assert!(matches!(
            event_rx.try_recv(),
            Ok(PipelineEvent::SpeakText(text))
                if text == "Bạn muốn nhắn bằng Messenger hay Telegram?"
        ));
    }

    #[test]
    fn avatar_control_bi_loc_truoc_khi_chia_clause_tts() {
        let mut filter = crate::tts::AvatarSpeechFilter::default();
        let mut chunker = crate::tts::TtsChunker::new();

        assert!(super::push_tts_token(&mut filter, &mut chunker, "[wa").is_empty());
        assert_eq!(
            super::push_tts_token(&mut filter, &mut chunker, "ve]Xin chào."),
            vec!["Xin chào.".to_string()]
        );
    }

    #[tokio::test]
    async fn flush_control_khong_bi_chan_boi_audio_queue_day() {
        let (speaker_tx, _speaker_rx) = mpsc::channel(1);
        let (control_tx, mut control_rx) = mpsc::channel(1);
        speaker_tx.try_send(frame(OP_SPEAKER_OUT)).unwrap();
        let outbound = VoiceOutbound::new(speaker_tx, control_tx);

        outbound.send_control(frame(OP_FLUSH)).await.unwrap();

        let flush = tokio::time::timeout(Duration::from_millis(10), control_rx.recv())
            .await
            .expect("FLUSH khong duoc cho audio queue")
            .expect("control channel phai con mo");
        assert_eq!(flush.op_code, OP_FLUSH);
    }

    /// Queue đầy phải fail-fast, không giữ blocking thread.
    ///
    /// **Bản trước là test NHẤP NHÁY — đo 29/07/2026 thấy đỏ 1/5 lần.** Nó bọc
    /// `spawn_blocking` trong `timeout(20ms)`, nên 20 ms đó phải đủ cho tokio
    /// **lên lịch** tác vụ, chạy nó, *và* trả kết quả về. Khi 491 test chạy song
    /// song trên cùng máy, chỉ riêng độ trễ lên lịch đã vượt 20 ms — lần đỏ bắt
    /// được có tổng thời gian chạy 13,2 s so với ~8 s của các lần xanh, tức máy
    /// đang tải. Đó là đua lịch trình, không phải hồi quy hành vi; và một cổng
    /// CI đỏ ngẫu nhiên ~20% thì tệ hơn không có cổng, vì nó dạy người ta bấm
    /// "chạy lại".
    ///
    /// Sửa bằng cách tách hai thứ bản cũ trộn làm một:
    /// - **Phép đo** giờ nằm *bên trong* closure, nên nó đo đúng thời gian
    ///   `blocking_send_speaker_if_current` tiêu tốn và **loại hẳn** độ trễ lên
    ///   lịch của thread pool khỏi con số.
    /// - **Hàng rào treo** bên ngoài nới rộng có chủ đích. Nó không phải phép đo:
    ///   chế độ hỏng cần bắt là chặn **vô hạn** (không ai rút queue trong test
    ///   này), nên bất kỳ hạn nào cũng bắt được — chọn rộng để không đánh đổi
    ///   độ tin cậy lấy thứ không cần.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn speaker_queue_day_fail_fast_khong_giu_blocking_thread() {
        let (speaker_tx, mut speaker_rx) = mpsc::channel(1);
        let (control_tx, _control_rx) = mpsc::channel(1);
        speaker_tx.try_send(frame(OP_SPEAKER_OUT)).unwrap();
        let outbound = VoiceOutbound::new(speaker_tx, control_tx);
        let active_epoch = Arc::new(AtomicU64::new(7));
        let active_epoch_for_send = Arc::clone(&active_epoch);

        let send = tokio::task::spawn_blocking(move || {
            let bat_dau = std::time::Instant::now();
            let ket_qua = outbound.blocking_send_speaker_if_current(
                &active_epoch_for_send,
                7,
                frame(OP_SPEAKER_OUT),
            );
            (ket_qua, bat_dau.elapsed())
        });

        let (result, ton_bao_lau) = tokio::time::timeout(Duration::from_secs(10), send)
            .await
            .expect("blocking_send_speaker_if_current TREO — fail-fast da hong")
            .unwrap();

        assert!(result.is_err(), "queue day phai tra Err");
        assert!(
            ton_bao_lau < Duration::from_millis(100),
            "fail-fast ton {ton_bao_lau:?} — dang cho queue rut thay vi tra Err ngay"
        );
        let _queued_frame = speaker_rx.recv().await.expect("frame lam day queue");

        assert!(
            matches!(
                speaker_rx.try_recv(),
                Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected)
            ),
            "khong duoc enqueue speaker cua epoch cu",
        );
    }

    #[test]
    fn speaker_epoch_cu_bi_loai_truoc_khi_enqueue() {
        let (speaker_tx, mut speaker_rx) = mpsc::channel(1);
        let (control_tx, _control_rx) = mpsc::channel(1);
        let outbound = VoiceOutbound::new(speaker_tx, control_tx);
        let active_epoch = AtomicU64::new(8);

        let result =
            outbound.blocking_send_speaker_if_current(&active_epoch, 7, frame(OP_SPEAKER_OUT));

        assert!(result.is_err());
        assert!(speaker_rx.try_recv().is_err());
    }
}
