//! voice_coordinator.rs — In-Process Voice and Audio Coordination
//! =============================================================
//! Replaces legacy loopback TCP WebSocket transport with a transport-agnostic
//! in-process voice coordinator for Tauri v2 native IPC Channels.
//!
//! Encapsulates WebRTCActor, Session-scoped DSP (AEC3, GTCRN, VAD), TurnAudioBuffer,
//! Smart-turn classification, WakeGate, and strongly-typed VoiceIpcEvent streaming.

use crate::AppState;
use crate::webrtc::frame::{
    OP_FLUSH, OP_SPEAKER_OUT, OP_VISME, SpeakerEpochGate, VoiceFrame, speaker_turn_epoch,
};
use crate::webrtc::pipeline::{PipelineState, VoiceOutbound, WebRTCActor, WebRTCPipelineHandle};
use crate::webrtc::session::{TurnAudioAction, TurnAudioBuffer, VoiceSessionAudio};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::mpsc;
use tracing::{error, info};

/// Strongly-typed IPC events streamed from native core to frontend UI via Tauri v2 Channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum VoiceIpcEvent {
    #[serde(alias = "speaker")]
    Speaker {
        turn_epoch: u32,
        sample_rate: u32,
        samples: Vec<f32>,
    },
    #[serde(alias = "viseme")]
    Viseme {
        turn_epoch: u64,
        base_seq_id: u32,
        visemes: serde_json::Value,
    },
    #[serde(alias = "flush")]
    Flush { seq_id: u32 },
    #[serde(alias = "text_event", alias = "textevent")]
    TextEvent {
        event: String,
        payload: serde_json::Value,
    },
}

/// Response returned from `voice_wake_probe` command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WakeProbeResponse {
    pub matched: bool,
    pub score: Option<f32>,
    pub transcript: String,
    pub tier: String,
    pub seq_id: u32,
}

enum DataMessage {
    Speaker(Option<VoiceFrame>),
    Text(Option<String>),
}

pub type VoiceEventSink = Arc<dyn Fn(VoiceIpcEvent) + Send + Sync>;

/// In-process Voice Coordinator managing WebRTCActor and audio pipeline.
pub struct VoiceCoordinator {
    state: Arc<AppState>,
    pipeline_handle: WebRTCPipelineHandle,
    voice_session: VoiceSessionAudio,
    turn_audio: Arc<Mutex<TurnAudioBuffer>>,
    wake_gate: Arc<Mutex<crate::wake::WakeGate>>,
    event_sink: Arc<RwLock<Option<VoiceEventSink>>>,
    actor_handle: tokio::task::JoinHandle<()>,
    forwarder_handle: tokio::task::JoinHandle<()>,
}

impl VoiceCoordinator {
    /// Create a new VoiceCoordinator bound to the given AppState and conversation_id.
    pub async fn new(state: Arc<AppState>, conversation_id: String) -> Self {
        Self::with_event_sink_internal(state, conversation_id, None).await
    }

    /// Create a new VoiceCoordinator with an initial event sink callback.
    pub async fn with_event_sink(
        state: Arc<AppState>,
        conversation_id: String,
        event_sink: VoiceEventSink,
    ) -> Self {
        Self::with_event_sink_internal(state, conversation_id, Some(event_sink)).await
    }

    async fn with_event_sink_internal(
        state: Arc<AppState>,
        conversation_id: String,
        initial_sink: Option<VoiceEventSink>,
    ) -> Self {
        let (speaker_tx, mut speaker_rx) = mpsc::channel::<VoiceFrame>(128);
        let (control_tx, mut control_rx) = mpsc::channel::<VoiceFrame>(16);
        let (text_tx, mut text_rx) = mpsc::channel::<String>(128);

        let voice_session = VoiceSessionAudio::from_app_state(state.as_ref()).await;
        let (pipeline_handle, actor) = WebRTCActor::new(
            state.clone(),
            VoiceOutbound::new(speaker_tx.clone(), control_tx.clone())
                .with_text_events(text_tx.clone()),
            conversation_id,
            voice_session.aec_handle(),
        );
        let actor = actor.with_voice_session(voice_session.clone());
        let actor_handle = tokio::spawn(actor.run());

        let event_sink = Arc::new(RwLock::new(initial_sink));
        let forwarder_sink = event_sink.clone();

        let forwarder_handle = tokio::spawn(async move {
            let mut epoch_gate = SpeakerEpochGate::default();
            let mut control_open = true;
            let mut speaker_open = true;
            let mut text_open = true;

            while control_open || speaker_open || text_open {
                tokio::select! {
                    biased;

                    maybe_frame = control_rx.recv(), if control_open => {
                        match maybe_frame {
                            Some(frame) => {
                                if frame.op_code == OP_FLUSH {
                                    epoch_gate.observe_flush(frame.seq_id);
                                    if let Ok(guard) = forwarder_sink.read() {
                                        if let Some(ref sink) = *guard {
                                            sink(VoiceIpcEvent::Flush { seq_id: frame.seq_id });
                                        }
                                    }
                                }
                            }
                            None => control_open = false,
                        }
                    }
                    data = async {
                        tokio::select! {
                            frame = speaker_rx.recv(), if speaker_open => DataMessage::Speaker(frame),
                            text = text_rx.recv(), if text_open => DataMessage::Text(text),
                        }
                    }, if speaker_open || text_open => match data {
                        DataMessage::Speaker(Some(frame)) => {
                            if frame.op_code == OP_SPEAKER_OUT {
                                if epoch_gate.accepts(&frame) {
                                    if let Some(turn_epoch) = speaker_turn_epoch(&frame) {
                                        if frame.payload.len() >= 8 {
                                            let sample_rate = u32::from_le_bytes(
                                                frame.payload[4..8].try_into().unwrap_or([0, 0, 0, 0]),
                                            );
                                            let samples = bytemuck::cast_slice::<u8, f32>(&frame.payload[8..]).to_vec();
                                            if let Ok(guard) = forwarder_sink.read() {
                                                if let Some(ref sink) = *guard {
                                                    sink(VoiceIpcEvent::Speaker {
                                                        turn_epoch,
                                                        sample_rate,
                                                        samples,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            } else if frame.op_code == OP_VISME {
                                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&frame.payload) {
                                    let turn_epoch = val.get("turn_epoch").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let base_seq_id = val.get("base_seq_id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                    let visemes = val.get("visemes").cloned().unwrap_or(serde_json::json!([]));
                                    if let Ok(guard) = forwarder_sink.read() {
                                        if let Some(ref sink) = *guard {
                                            sink(VoiceIpcEvent::Viseme {
                                                turn_epoch,
                                                base_seq_id,
                                                visemes,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        DataMessage::Speaker(None) => speaker_open = false,
                        DataMessage::Text(Some(text)) => {
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                                let event = val.get("event").and_then(|v| v.as_str()).unwrap_or("text_event").to_string();
                                let payload = val.get("payload").cloned().unwrap_or(val.clone());
                                if let Ok(guard) = forwarder_sink.read() {
                                    if let Some(ref sink) = *guard {
                                        sink(VoiceIpcEvent::TextEvent { event, payload });
                                    }
                                }
                            }
                        }
                        DataMessage::Text(None) => text_open = false,
                    }
                }
            }
        });

        let turn_audio = Arc::new(Mutex::new(TurnAudioBuffer::new(1536)));
        let wake_gate = Arc::new(Mutex::new(crate::wake::WakeGate::from_env()));

        Self {
            state,
            pipeline_handle,
            voice_session,
            turn_audio,
            wake_gate,
            event_sink,
            actor_handle,
            forwarder_handle,
        }
    }

    /// Register or replace the event sink callback.
    pub fn subscribe(&self, sink: VoiceEventSink) {
        if let Ok(mut guard) = self.event_sink.write() {
            *guard = Some(sink);
        }
    }

    /// Clear the registered event sink callback.
    pub fn unsubscribe(&self) {
        if let Ok(mut guard) = self.event_sink.write() {
            *guard = None;
        }
    }

    /// Dispatch an arbitrary text event through the IPC event sink.
    pub fn send_text_event(&self, event: &str, payload: serde_json::Value) {
        self.emit_event(VoiceIpcEvent::TextEvent {
            event: event.to_string(),
            payload,
        });
    }

    /// Helper to dispatch event to subscriber.
    fn emit_event(&self, event: VoiceIpcEvent) {
        if let Ok(guard) = self.event_sink.read() {
            if let Some(ref sink) = *guard {
                sink(event);
            }
        }
    }

    /// Ingest a microphone audio chunk (16kHz mono PCM f32) into the voice pipeline.
    pub async fn ingest_mic_chunk(&self, _seq_id: u32, samples: Vec<f32>) -> Result<(), String> {
        if samples.is_empty() {
            return Ok(());
        }

        let voice_session_capture = self.voice_session.clone();
        let samples_vec = samples;
        let (events, cleaned_samples) =
            tokio::task::spawn_blocking(move || voice_session_capture.process_mic(samples_vec))
                .await
                .map_err(|e| format!("Audio pipeline task panicked: {e}"))??;

        if let Ok(mut gate) = self.wake_gate.lock() {
            if let Some((name, score)) = gate.check_streaming(&cleaned_samples) {
                info!(
                    "Wake word detected (streaming classifier): {} ({:.3})",
                    name, score
                );
            }
        }

        let vad_events = events
            .into_iter()
            .map(|(event, _confidence)| event)
            .collect::<Vec<_>>();
        let actions = {
            let mut turn_guard = self
                .turn_audio
                .lock()
                .map_err(|_| "TurnAudioBuffer lock poisoned".to_string())?;
            turn_guard.ingest(&cleaned_samples, &vad_events)
        };

        for action in actions {
            match action {
                TurnAudioAction::Started => {
                    self.voice_session.clear_aec_render();
                    let is_awake = self.wake_gate.lock().map(|g| g.is_awake()).unwrap_or(true);
                    if is_awake {
                        if let Err(e) = self.pipeline_handle.on_vad_start() {
                            error!("Failed on_vad_start: {e}");
                        }
                    }
                }
                TurnAudioAction::SilenceProbe {
                    consecutive_silence_frames,
                    audio,
                } => {
                    let voice_session_turn = self.voice_session.clone();
                    let audio_for_turn = audio.clone();
                    let decision = tokio::task::spawn_blocking(move || {
                        voice_session_turn.evaluate_turn(&audio_for_turn)
                    })
                    .await
                    .ok()
                    .flatten();

                    let silence_duration_ms = (consecutive_silence_frames as u64) * 32;

                    match decision {
                        Some(Ok(
                            crate::webrtc::turn_shadow::AdaptiveTurnDecision::ImmediateCutoff {
                                probability,
                            },
                        )) => {
                            info!(
                                "[smart-turn:adaptive] Stage 1 Fast Cutoff at frame {} (p={:.3} > 0.92, ~{}ms)",
                                consecutive_silence_frames, probability, silence_duration_ms
                            );

                            self.emit_event(VoiceIpcEvent::TextEvent {
                                event: "voice:turn_arbitration".to_string(),
                                payload: serde_json::json!({
                                    "stage": "stage_1_fast",
                                    "confidence_score": probability,
                                    "silence_duration_ms": silence_duration_ms,
                                    "is_turn_complete": true
                                }),
                            });

                            let speech_audio = {
                                let mut turn_guard = self
                                    .turn_audio
                                    .lock()
                                    .map_err(|_| "TurnAudioBuffer lock poisoned".to_string())?;
                                turn_guard.force_end().unwrap_or(audio)
                            };
                            self.voice_session.force_vad_speech_end();

                            let (is_awake, uses_stt) = if let Ok(gate) = self.wake_gate.lock() {
                                (gate.is_awake(), gate.uses_stt_confirm())
                            } else {
                                (true, false)
                            };

                            if is_awake {
                                if let Ok(mut gate) = self.wake_gate.lock() {
                                    gate.note_activity();
                                }
                                if let Err(e) = self.pipeline_handle.on_vad_end(speech_audio) {
                                    error!("Failed on_vad_end in Stage 1 Fast Cutoff: {e}");
                                }
                            } else if uses_stt {
                                let state_wake = self.state.clone();
                                let audio_for_stt = speech_audio.clone();
                                let transcript = tokio::task::spawn_blocking(move || {
                                    let mut stt = state_wake.stt.blocking_lock();
                                    stt.transcribe_for_wake(&audio_for_stt)
                                })
                                .await;

                                if let Ok(Ok(Some(text))) = transcript {
                                    let woken = self
                                        .wake_gate
                                        .lock()
                                        .map(|mut g| g.try_wake(&text))
                                        .unwrap_or(false);
                                    if woken {
                                        info!("Wake word detected (tier-2 STT): {:?}", text);
                                        if let Err(e) =
                                            self.pipeline_handle.on_vad_end(speech_audio)
                                        {
                                            error!("Failed on_vad_end: {e}");
                                        }
                                    }
                                }
                            }
                        }
                        Some(Ok(
                            crate::webrtc::turn_shadow::AdaptiveTurnDecision::HesitationWait {
                                probability,
                            },
                        )) => {
                            info!(
                                "[smart-turn:adaptive] Stage 2 Adaptive Hold at frame {} (p={:.3}, holding for pause/conjunction)",
                                consecutive_silence_frames, probability
                            );
                            self.emit_event(VoiceIpcEvent::TextEvent {
                                event: "voice:turn_arbitration".to_string(),
                                payload: serde_json::json!({
                                    "stage": "stage_2_hold",
                                    "confidence_score": probability,
                                    "silence_duration_ms": silence_duration_ms,
                                    "is_turn_complete": false
                                }),
                            });
                        }
                        Some(Ok(
                            crate::webrtc::turn_shadow::AdaptiveTurnDecision::Incomplete {
                                probability,
                            },
                        )) => {
                            info!(
                                "[smart-turn:adaptive] Stage 2 Adaptive Hold (incomplete utterance, p={:.3} < 0.50)",
                                probability
                            );
                            self.emit_event(VoiceIpcEvent::TextEvent {
                                event: "voice:turn_arbitration".to_string(),
                                payload: serde_json::json!({
                                    "stage": "stage_2_hold",
                                    "confidence_score": probability,
                                    "silence_duration_ms": silence_duration_ms,
                                    "is_turn_complete": false
                                }),
                            });
                        }
                        _ => {}
                    }
                }
                TurnAudioAction::Ended(speech_audio) => {
                    if speech_audio.is_empty() {
                        continue;
                    }
                    let silence_duration_ms = 14u64 * 32;
                    info!(
                        "[smart-turn:adaptive] Stage 3 Timeout fallback at frame 14 (~{}ms)",
                        silence_duration_ms
                    );
                    self.emit_event(VoiceIpcEvent::TextEvent {
                        event: "voice:turn_arbitration".to_string(),
                        payload: serde_json::json!({
                            "stage": "stage_3_timeout",
                            "confidence_score": 0.0,
                            "silence_duration_ms": silence_duration_ms,
                            "is_turn_complete": true
                        }),
                    });

                    let (is_awake, uses_stt) = if let Ok(gate) = self.wake_gate.lock() {
                        (gate.is_awake(), gate.uses_stt_confirm())
                    } else {
                        (true, false)
                    };

                    if is_awake {
                        if let Ok(mut gate) = self.wake_gate.lock() {
                            gate.note_activity();
                        }
                        if let Err(e) = self.pipeline_handle.on_vad_end(speech_audio) {
                            error!("Failed on_vad_end in Stage 3 Timeout: {e}");
                        }
                    } else if uses_stt {
                        let state_wake = self.state.clone();
                        let audio_for_stt = speech_audio.clone();
                        let transcript = tokio::task::spawn_blocking(move || {
                            let mut stt = state_wake.stt.blocking_lock();
                            stt.transcribe_for_wake(&audio_for_stt)
                        })
                        .await;

                        if let Ok(Ok(Some(text))) = transcript {
                            let woken = self
                                .wake_gate
                                .lock()
                                .map(|mut g| g.try_wake(&text))
                                .unwrap_or(false);
                            if woken {
                                info!("Wake word detected (tier-2 STT): {:?}", text);
                                if let Err(e) = self.pipeline_handle.on_vad_end(speech_audio) {
                                    error!("Failed on_vad_end: {e}");
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Evaluate candidate utterance for wake-word activation (widget probe).
    pub async fn evaluate_wake_probe(
        &self,
        seq_id: u32,
        samples: Vec<f32>,
    ) -> Result<WakeProbeResponse, String> {
        const WAKE_PROBE_MIN_SECS: f32 = 0.3;
        const WAKE_PROBE_MAX_SECS: f32 = 4.0;

        let duration_secs = samples.len() as f32 / 16_000.0;
        if !(WAKE_PROBE_MIN_SECS..=WAKE_PROBE_MAX_SECS).contains(&duration_secs) {
            let resp = WakeProbeResponse {
                matched: false,
                score: None,
                transcript: String::new(),
                tier: "length_rejected".to_string(),
                seq_id,
            };
            self.emit_event(VoiceIpcEvent::TextEvent {
                event: "wake_probe_rejected".to_string(),
                payload: serde_json::json!({
                    "source": "widget_probe",
                    "tier": "none",
                    "score": serde_json::Value::Null,
                    "transcript": "",
                    "seq_id": seq_id,
                }),
            });
            return Ok(resp);
        }

        let (best_score, model_threshold, _detector_is_some) = {
            let mut gate = self
                .wake_gate
                .lock()
                .map_err(|_| "WakeGate lock poisoned".to_string())?;
            (
                gate.score_clip(&samples),
                gate.model_threshold(),
                gate.detector_is_some(),
            )
        };

        let clip_score = best_score
            .clone()
            .filter(|(_, score)| score.is_finite() && *score > model_threshold);

        let heard = if clip_score.is_some() {
            String::new()
        } else {
            let state_probe = self.state.clone();
            let audio_for_stt = samples.clone();
            let transcript = tokio::task::spawn_blocking(move || {
                let mut stt = state_probe.stt.blocking_lock();
                stt.transcribe_for_wake(&audio_for_stt)
            })
            .await;

            match transcript {
                Ok(Ok(Some(text))) => text,
                _ => String::new(),
            }
        };

        let stt_matched = !heard.trim().is_empty()
            && self
                .wake_gate
                .lock()
                .map(|g| g.matches_phrase(&heard))
                .unwrap_or(false);
        let matched = clip_score.is_some() || stt_matched;
        let tier = if clip_score.is_some() {
            "classifier"
        } else {
            "stt"
        };
        let score = best_score.as_ref().map(|(_, s)| *s);

        if matched {
            info!("Wake word confirmed via {tier} (widget probe)");
        } else {
            info!("Wake probe rejected — heard: {:?}", heard);
        }

        let resp = WakeProbeResponse {
            matched,
            score,
            transcript: heard.clone(),
            tier: tier.to_string(),
            seq_id,
        };

        self.emit_event(VoiceIpcEvent::TextEvent {
            event: if matched {
                "wake_word_triggered".to_string()
            } else {
                "wake_probe_rejected".to_string()
            },
            payload: serde_json::json!({
                "source": "widget_probe",
                "tier": tier,
                "score": score,
                "transcript": heard,
                "seq_id": seq_id,
            }),
        });

        Ok(resp)
    }

    /// Request immediate cancellation / barge-in cutoff.
    pub fn interrupt(&self) -> Result<(), String> {
        self.pipeline_handle.on_interrupted()
    }

    /// Speak text using the TTS synthesis pipeline.
    pub fn speak_text(&self, text: String) -> Result<(), String> {
        self.pipeline_handle.speak_text(text)
    }

    /// Query the current pipeline state.
    pub fn state(&self) -> PipelineState {
        self.pipeline_handle.state()
    }

    /// Reset DSP states across session.
    pub fn reset_dsp(&self) -> Result<(), String> {
        self.pipeline_handle.reset_dsp()
    }
}

impl Drop for VoiceCoordinator {
    fn drop(&mut self) {
        self.actor_handle.abort();
        self.forwarder_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_ipc_event_serialization_round_trip() {
        let speaker_event = VoiceIpcEvent::Speaker {
            turn_epoch: 42,
            sample_rate: 16000,
            samples: vec![0.1, -0.2, 0.3],
        };
        let json = serde_json::to_string(&speaker_event).expect("serialize speaker event");
        let deserialized: VoiceIpcEvent =
            serde_json::from_str(&json).expect("deserialize speaker event");
        assert_eq!(speaker_event, deserialized);

        let flush_event = VoiceIpcEvent::Flush { seq_id: 10 };
        let json = serde_json::to_string(&flush_event).expect("serialize flush event");
        let deserialized: VoiceIpcEvent =
            serde_json::from_str(&json).expect("deserialize flush event");
        assert_eq!(flush_event, deserialized);

        let viseme_event = VoiceIpcEvent::Viseme {
            turn_epoch: 1234,
            base_seq_id: 5,
            visemes: serde_json::json!([{"v": "aa", "t_ms": 0}]),
        };
        let json = serde_json::to_string(&viseme_event).expect("serialize viseme event");
        let deserialized: VoiceIpcEvent =
            serde_json::from_str(&json).expect("deserialize viseme event");
        assert_eq!(viseme_event, deserialized);

        let text_event = VoiceIpcEvent::TextEvent {
            event: "wake_word_triggered".to_string(),
            payload: serde_json::json!({"score": 0.98}),
        };
        let json = serde_json::to_string(&text_event).expect("serialize text event");
        let deserialized: VoiceIpcEvent =
            serde_json::from_str(&json).expect("deserialize text event");
        assert_eq!(text_event, deserialized);
    }

    #[test]
    fn voice_ipc_event_snake_case_alias_deserialization() {
        let json =
            r#"{"type":"speaker","data":{"turn_epoch":1,"sample_rate":16000,"samples":[0.5]}}"#;
        let event: VoiceIpcEvent =
            serde_json::from_str(json).expect("deserialize lower-case speaker");
        assert!(matches!(
            event,
            VoiceIpcEvent::Speaker { turn_epoch: 1, .. }
        ));

        let json_viseme =
            r#"{"type":"viseme","data":{"turn_epoch":1,"base_seq_id":0,"visemes":[]}}"#;
        let event: VoiceIpcEvent =
            serde_json::from_str(json_viseme).expect("deserialize lower-case viseme");
        assert!(matches!(event, VoiceIpcEvent::Viseme { turn_epoch: 1, .. }));

        let json_flush = r#"{"type":"flush","data":{"seq_id":99}}"#;
        let event: VoiceIpcEvent =
            serde_json::from_str(json_flush).expect("deserialize lower-case flush");
        assert!(matches!(event, VoiceIpcEvent::Flush { seq_id: 99 }));
    }
}
