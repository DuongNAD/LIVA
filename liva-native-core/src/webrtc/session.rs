use crate::AppState;
use crate::webrtc::aec::{SelfEchoCanceller, WasapiLoopbackCapturer};
use crate::webrtc::denoise::GtcrnDenoiser;
use crate::webrtc::turn_shadow::{AdaptiveTurnDecision, SmartTurnClassifier};
use crate::webrtc::vad::{VadEngine, VadEvent};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub type SessionAec = Arc<Mutex<Option<SelfEchoCanceller>>>;
type MicProcessingResult = (Vec<(VadEvent, f32)>, Vec<f32>);

#[derive(Clone, Copy, Debug)]
pub struct VoiceRuntimeConfig {
    pub vad_enabled: bool,
    pub denoise_enabled: bool,
    pub turn_shadow_enabled: bool,
    pub aec_enabled: bool,
}

#[cfg(target_os = "windows")]
const DEFAULT_AEC_ENABLED: bool = true;

#[cfg(not(target_os = "windows"))]
const DEFAULT_AEC_ENABLED: bool = false;

impl VoiceRuntimeConfig {
    pub fn from_env() -> Self {
        Self {
            vad_enabled: crate::env_flag("LIVA_VAD_ENABLED", true),
            denoise_enabled: crate::env_flag("LIVA_DENOISE_ENABLED", true),
            turn_shadow_enabled: crate::env_flag("LIVA_TURN_SHADOW_ENABLED", true),
            aec_enabled: crate::env_flag("LIVA_AEC_ENABLED", DEFAULT_AEC_ENABLED),
        }
    }
}

/// Process-level voice processors loaded once and forked per WebSocket session.
pub struct VoiceRuntimeComponents {
    pub vad: Option<VadEngine>,
    pub denoiser: Option<GtcrnDenoiser>,
    pub turn_shadow: Option<SmartTurnClassifier>,
    pub aec: Option<SelfEchoCanceller>,
}

impl VoiceRuntimeComponents {
    pub fn from_env(stt_model_dir: &str) -> Self {
        Self::load(stt_model_dir, VoiceRuntimeConfig::from_env())
    }

    pub fn load(stt_model_dir: &str, config: VoiceRuntimeConfig) -> Self {
        let vad = if config.vad_enabled {
            let path = crate::webrtc::vad::resolve_model_path(stt_model_dir);
            if path.exists() {
                match VadEngine::new(&path, crate::webrtc::vad::VadConfig::from_env()) {
                    Ok(engine) => Some(engine),
                    Err(error) => {
                        tracing::warn!("Failed to initialize VadEngine: {error}");
                        None
                    }
                }
            } else {
                tracing::warn!("VAD model not found at {:?}", path);
                None
            }
        } else {
            tracing::info!("VAD disabled via LIVA_VAD_ENABLED");
            None
        };

        let denoiser = if config.denoise_enabled {
            let path = crate::webrtc::denoise::resolve_model_path();
            if path.exists() {
                match GtcrnDenoiser::new(&path) {
                    Ok(denoiser) => {
                        tracing::info!("GTCRN denoise enabled (model {:?})", path);
                        Some(denoiser)
                    }
                    Err(error) => {
                        tracing::warn!(
                            "Failed to initialize GtcrnDenoiser: {error}; running without denoise"
                        );
                        None
                    }
                }
            } else {
                tracing::warn!(
                    "GTCRN denoise model not found at {:?}; running without denoise",
                    path
                );
                None
            }
        } else {
            tracing::info!("GTCRN denoise disabled via LIVA_DENOISE_ENABLED");
            None
        };

        let turn_shadow = if config.turn_shadow_enabled {
            let path = crate::webrtc::turn_shadow::resolve_model_path();
            if path.exists() {
                match SmartTurnClassifier::new(&path) {
                    Ok(classifier) => Some(classifier),
                    Err(error) => {
                        tracing::warn!("Failed to initialize SmartTurnClassifier: {error}");
                        None
                    }
                }
            } else {
                tracing::warn!("Smart Turn model not found at {:?}", path);
                None
            }
        } else {
            None
        };

        let aec = config.aec_enabled.then(SelfEchoCanceller::new);

        Self {
            vad,
            denoiser,
            turn_shadow,
            aec,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum TurnAudioAction {
    Started,
    Ended(Vec<f32>),
    SilenceProbe {
        consecutive_silence_frames: usize,
        audio: Vec<f32>,
    },
}

/// Assembles one utterance at chunk granularity and retains bounded idle audio
/// as historical pre-roll for the next `SpeechStart`.
pub struct TurnAudioBuffer {
    pre_roll: VecDeque<f32>,
    active: Option<Vec<f32>>,
    pre_roll_capacity: usize,
}

impl TurnAudioBuffer {
    pub const MAX_TURN_SAMPLES: usize = 16_000 * 20; // 20 seconds @ 16kHz
    pub const MIN_TURN_SAMPLES: usize = 16_000 / 10; // 100ms @ 16kHz (1600 samples)

    pub fn new(pre_roll_capacity: usize) -> Self {
        Self {
            pre_roll: VecDeque::with_capacity(pre_roll_capacity),
            active: None,
            pre_roll_capacity,
        }
    }

    pub fn ingest(&mut self, chunk: &[f32], events: &[VadEvent]) -> Vec<TurnAudioAction> {
        let mut actions = Vec::new();
        let mut chunk_attached = false;

        for event in events {
            match event {
                VadEvent::SpeechStart if self.active.is_none() => {
                    self.active = Some(self.pre_roll.drain(..).collect());
                    actions.push(TurnAudioAction::Started);
                }
                VadEvent::SilenceProbe {
                    consecutive_silence_frames,
                } if self.active.is_some() => {
                    if !chunk_attached && let Some(active) = self.active.as_mut() {
                        active.extend_from_slice(chunk);
                        chunk_attached = true;
                    }
                    if let Some(active) = self.active.as_ref()
                        && active.len() >= Self::MIN_TURN_SAMPLES
                    {
                        actions.push(TurnAudioAction::SilenceProbe {
                            consecutive_silence_frames: *consecutive_silence_frames,
                            audio: active.clone(),
                        });
                    }
                }
                VadEvent::SpeechEnd if self.active.is_some() => {
                    if !chunk_attached && let Some(active) = self.active.as_mut() {
                        active.extend_from_slice(chunk);
                        chunk_attached = true;
                    }
                    if let Some(turn_data) = self.active.take() {
                        if turn_data.len() >= Self::MIN_TURN_SAMPLES {
                            actions.push(TurnAudioAction::Ended(turn_data));
                        } else if !turn_data.is_empty() {
                            self.push_pre_roll(&turn_data);
                        }
                    }
                }
                _ => {}
            }
        }

        if !chunk_attached && let Some(active) = self.active.as_mut() {
            active.extend_from_slice(chunk);
            chunk_attached = true;
            if active.len() >= Self::MAX_TURN_SAMPLES {
                let speech = self.active.take().unwrap_or_default();
                if speech.len() >= Self::MIN_TURN_SAMPLES {
                    actions.push(TurnAudioAction::Ended(speech));
                }
                // Silent re-arming for continuous noise / speech segmentation.
                // CRITICAL: Do NOT emit Started here, because downstream in
                // WebRTCActor::handle_vad_start, Started triggers cancel_active_operations(),
                // which would immediately abort the in-flight STT task of the 20s turn above!
                self.active = Some(Vec::new());
            }
        }
        if self.active.is_none() && !chunk_attached {
            self.push_pre_roll(chunk);
        }

        actions
    }

    pub fn ingest_event(
        &mut self,
        chunk: &[f32],
        vad_event: Option<VadEvent>,
    ) -> Option<TurnAudioAction> {
        let events = match vad_event {
            Some(ev) => vec![ev],
            None => Vec::new(),
        };
        self.ingest(chunk, &events).into_iter().next()
    }

    fn push_pre_roll(&mut self, chunk: &[f32]) {
        if self.pre_roll_capacity == 0 {
            return;
        }
        if chunk.len() >= self.pre_roll_capacity {
            self.pre_roll.clear();
            self.pre_roll.extend(
                chunk[chunk.len() - self.pre_roll_capacity..]
                    .iter()
                    .copied(),
            );
            return;
        }

        let overflow = self
            .pre_roll
            .len()
            .saturating_add(chunk.len())
            .saturating_sub(self.pre_roll_capacity);
        self.pre_roll.drain(..overflow);
        self.pre_roll.extend(chunk.iter().copied());
    }

    /// Immediately force the current turn to end and extract accumulated speech audio.
    /// Used by Stage 1 Fast Cutoff when Smart Turn confirms turn completion at ~200ms.
    pub fn force_end(&mut self) -> Option<Vec<f32>> {
        if let Some(turn_data) = self.active.take() {
            if turn_data.len() >= Self::MIN_TURN_SAMPLES {
                Some(turn_data)
            } else {
                if !turn_data.is_empty() {
                    self.push_pre_roll(&turn_data);
                }
                None
            }
        } else {
            None
        }
    }
}

/// DSP state owned by exactly one WebSocket connection.
///
/// VAD/GTCRN forks share their immutable loaded model, but all recurrent,
/// buffering, debounce, and AEC state is connection-local.
#[derive(Clone)]
pub struct VoiceSessionAudio {
    vad: Arc<Mutex<Option<VadEngine>>>,
    denoiser: Arc<Mutex<Option<GtcrnDenoiser>>>,
    turn_shadow: Arc<Mutex<Option<SmartTurnClassifier>>>,
    aec: SessionAec,
    loopback: Arc<Mutex<Option<WasapiLoopbackCapturer>>>,
}

impl VoiceSessionAudio {
    pub fn new(
        vad: Option<VadEngine>,
        denoiser: Option<GtcrnDenoiser>,
        aec: Option<SelfEchoCanceller>,
    ) -> Self {
        Self::with_components(vad, denoiser, None, aec, None)
    }

    pub fn with_components(
        vad: Option<VadEngine>,
        denoiser: Option<GtcrnDenoiser>,
        turn_shadow: Option<SmartTurnClassifier>,
        aec: Option<SelfEchoCanceller>,
        loopback: Option<WasapiLoopbackCapturer>,
    ) -> Self {
        Self {
            vad: Arc::new(Mutex::new(vad)),
            denoiser: Arc::new(Mutex::new(denoiser)),
            turn_shadow: Arc::new(Mutex::new(turn_shadow)),
            aec: Arc::new(Mutex::new(aec)),
            loopback: Arc::new(Mutex::new(loopback)),
        }
    }

    /// Fork the process-level model holders into state owned by one WebSocket.
    pub async fn from_app_state(state: &AppState) -> Self {
        let vad = state.vad.lock().await.as_ref().map(VadEngine::fork_session);
        let denoiser = state
            .denoiser
            .lock()
            .await
            .as_ref()
            .map(GtcrnDenoiser::fork_session);
        let turn_shadow = state
            .turn_shadow
            .lock()
            .await
            .as_ref()
            .map(SmartTurnClassifier::fork_session);
        let aec = state
            .aec
            .lock()
            .await
            .as_ref()
            .map(|_| SelfEchoCanceller::new());

        let aec_arc = Arc::new(Mutex::new(aec));
        let loopback = if aec_arc.lock().map(|g| g.is_some()).unwrap_or(false) {
            match WasapiLoopbackCapturer::start(Arc::clone(&aec_arc)) {
                Ok(capturer) => {
                    tracing::info!(
                        device = capturer.device_name(),
                        sample_rate = capturer.sample_rate(),
                        channels = capturer.channels(),
                        "WASAPI loopback audio capture active for AEC3"
                    );
                    Some(capturer)
                }
                Err(err) => {
                    tracing::warn!(
                        "Could not start WASAPI loopback capture: {err}; continuing with software TTS reference only"
                    );
                    None
                }
            }
        } else {
            None
        };

        Self {
            vad: Arc::new(Mutex::new(vad)),
            denoiser: Arc::new(Mutex::new(denoiser)),
            turn_shadow: Arc::new(Mutex::new(turn_shadow)),
            aec: aec_arc,
            loopback: Arc::new(Mutex::new(loopback)),
        }
    }

    pub fn evaluate_turn(&self, samples: &[f32]) -> Option<Result<AdaptiveTurnDecision, String>> {
        let guard = self.turn_shadow.lock().ok()?;
        guard.as_ref().map(|c| c.evaluate_turn(samples))
    }

    pub fn force_vad_speech_end(&self) {
        if let Ok(mut guard) = self.vad.lock()
            && let Some(vad) = guard.as_mut()
        {
            vad.force_speech_end();
        }
    }

    pub fn loopback_capturer(&self) -> Option<WasapiLoopbackCapturer> {
        self.loopback.lock().ok().and_then(|g| g.clone())
    }

    pub fn aec_handle(&self) -> SessionAec {
        Arc::clone(&self.aec)
    }

    pub fn reset_aec(&self) {
        if let Ok(mut guard) = self.aec.lock()
            && let Some(aec) = guard.as_mut()
        {
            aec.reset();
        }
    }

    pub fn reset_vad(&self) {
        if let Ok(mut guard) = self.vad.lock() {
            let Some(vad) = guard.as_mut() else {
                return;
            };
            vad.reset();
        }
    }

    pub fn reset_denoiser(&self) {
        if let Ok(mut guard) = self.denoiser.lock() {
            let Some(denoiser) = guard.as_mut() else {
                return;
            };
            denoiser.reset();
        }
    }

    /// Reset all DSP audio pipeline components (VAD recurrent state, GTCRN denoiser caches, and AEC render history).
    pub fn reset_dsp(&self) {
        self.reset_vad();
        self.reset_denoiser();
        self.reset_aec();
    }

    pub fn clear_aec_render(&self) {
        if let Ok(mut guard) = self.aec.lock()
            && let Some(aec) = guard.as_mut()
        {
            aec.clear_render();
        }
    }

    /// Run the synchronous capture chain. Call only from `spawn_blocking`.
    pub fn process_mic(&self, samples: Vec<f32>) -> Result<MicProcessingResult, String> {
        let mut working = samples;

        {
            let mut guard = self
                .aec
                .lock()
                .map_err(|_| "WebSocket AEC mutex poisoned".to_string())?;
            if let Some(aec) = guard.as_mut() {
                match aec.process_capture(&working) {
                    Ok(output) => working = output,
                    Err(error) => tracing::error!("AEC process_capture failed: {}", error),
                }
            }
        }

        {
            let mut guard = self
                .denoiser
                .lock()
                .map_err(|_| "WebSocket denoiser mutex poisoned".to_string())?;
            if let Some(denoiser) = guard.as_mut() {
                match denoiser.process_audio(&working) {
                    Ok(output) => working = output,
                    Err(error) => tracing::error!("GTCRN denoise failed: {}", error),
                }
            }
        }

        let events = {
            let mut guard = self
                .vad
                .lock()
                .map_err(|_| "WebSocket VAD mutex poisoned".to_string())?;
            match guard.as_mut() {
                Some(vad) => vad.process_audio(&working)?,
                None => Vec::new(),
            }
        };

        Ok((events, working))
    }
}

#[cfg(test)]
mod tests {
    use super::{TurnAudioAction, TurnAudioBuffer, VoiceSessionAudio};
    use crate::webrtc::aec::SelfEchoCanceller;
    use crate::webrtc::vad::VadEvent;
    use std::sync::Arc;

    #[test]
    fn websocket_sessions_never_share_aec_state() {
        let session_a = VoiceSessionAudio::new(None, None, Some(SelfEchoCanceller::new()));
        let session_b = VoiceSessionAudio::new(None, None, Some(SelfEchoCanceller::new()));

        let aec_a = session_a.aec_handle();
        let aec_b = session_b.aec_handle();

        assert!(
            !Arc::ptr_eq(&aec_a, &aec_b),
            "TTS render/capture state must be owned by one WebSocket only"
        );
    }

    #[test]
    fn turn_audio_uses_historical_pre_roll_and_keeps_each_chunk_once() {
        let mut buffer = TurnAudioBuffer::new(300);
        assert!(buffer.ingest(&vec![1.0; 300], &[]).is_empty());

        let started = buffer.ingest(&vec![2.0; 800], &[VadEvent::SpeechStart]);
        assert!(matches!(started.as_slice(), [TurnAudioAction::Started]));
        assert!(buffer.ingest(&vec![3.0; 200], &[]).is_empty());

        let ended = buffer.ingest(&vec![4.0; 500], &[VadEvent::SpeechEnd]);
        let [TurnAudioAction::Ended(audio)] = ended.as_slice() else {
            panic!("SpeechEnd must emit exactly one completed turn");
        };

        // 300 pre-roll + 800 start + 200 mid + 500 end = 1800 samples (>= MIN_TURN_SAMPLES 1600)
        assert_eq!(audio.len(), 1800);
        assert_eq!(&audio[..300], &[1.0; 300]);
        assert_eq!(&audio[300..1100], &[2.0; 800]);
        assert_eq!(&audio[1100..1300], &[3.0; 200]);
        assert_eq!(&audio[1300..], &[4.0; 500]);

        assert!(buffer.ingest(&vec![5.0; 300], &[]).is_empty());
        assert!(matches!(
            buffer
                .ingest(&vec![6.0; 800], &[VadEvent::SpeechStart])
                .as_slice(),
            [TurnAudioAction::Started]
        ));
        let next_ended = buffer.ingest(&vec![7.0; 800], &[VadEvent::SpeechEnd]);
        let [TurnAudioAction::Ended(next_audio)] = next_ended.as_slice() else {
            panic!("next SpeechEnd must emit exactly one completed turn");
        };

        // 300 pre-roll + 800 start + 800 end = 1900 samples (>= MIN_TURN_SAMPLES 1600)
        assert_eq!(next_audio.len(), 1900);
        assert_eq!(&next_audio[..300], &[5.0; 300]);
        assert_eq!(&next_audio[300..1100], &[6.0; 800]);
        assert_eq!(&next_audio[1100..], &[7.0; 800]);
    }

    #[test]
    fn turn_audio_terminates_when_exceeding_max_turn_samples_ceiling() {
        let mut buffer = TurnAudioBuffer::new(1536);
        let chunk = vec![0.2f32; 1600];

        // 60 seconds of continuous speech = 600 chunks of 1600 samples = 960,000 samples.
        // Chunk 0 has SpeechStart, subsequent chunks have empty VAD events &[] (VAD stuck in speech mode).
        let mut ended_turns = Vec::new();
        let mut started_count = 0usize;

        for chunk_idx in 0..600 {
            let events = if chunk_idx == 0 {
                vec![VadEvent::SpeechStart]
            } else {
                vec![]
            };

            let actions = buffer.ingest(&chunk, &events);

            for action in actions {
                match action {
                    TurnAudioAction::Started => started_count += 1,
                    TurnAudioAction::Ended(audio) => ended_turns.push(audio),
                    TurnAudioAction::SilenceProbe { .. } => {}
                }
            }
        }

        // Exactly 3 turns must terminate across 60 seconds (at chunks 200, 400, 600)
        assert_eq!(
            ended_turns.len(),
            3,
            "60s continuous speech must emit exactly 3 turns"
        );
        // Silent re-arming must NOT emit Started actions on ceiling transitions (Started count remains 1)
        assert_eq!(
            started_count, 1,
            "Must only emit the initial Started action (silent re-arming emits no Started)"
        );

        let mut total_samples = 0;
        for (idx, turn) in ended_turns.iter().enumerate() {
            assert_eq!(
                turn.len(),
                TurnAudioBuffer::MAX_TURN_SAMPLES,
                "Turn {} must terminate at exactly MAX_TURN_SAMPLES (320,000 samples)",
                idx + 1
            );
            total_samples += turn.len();
        }
        assert_eq!(
            total_samples, 960_000,
            "Total samples across all 3 turns must equal 960,000 (zero sample drop)"
        );

        // Test trailing SpeechEnd with < 1600 samples:
        // Turn 4 is currently active with 0 samples.
        // Ingest a sub-minimum chunk of 800 samples (< MIN_TURN_SAMPLES = 1600) with SpeechEnd.
        let small_chunk = vec![0.3f32; 800];
        let trailing_actions = buffer.ingest(&small_chunk, &[VadEvent::SpeechEnd]);
        // Must NOT emit Ended action because 800 < MIN_TURN_SAMPLES (1600)
        assert!(
            trailing_actions.is_empty(),
            "Trailing SpeechEnd with < 1600 samples must be filtered out"
        );

        // Verify the 800 samples were recycled into pre-roll:
        // When a new SpeechStart arrives, it should recover the recycled pre-roll samples.
        let new_start = buffer.ingest(&vec![0.5f32; 1600], &[VadEvent::SpeechStart]);
        assert_eq!(new_start, vec![TurnAudioAction::Started]);
        let next_turn_actions = buffer.ingest(&vec![0.5f32; 1600], &[VadEvent::SpeechEnd]);
        assert_eq!(next_turn_actions.len(), 1);
        let TurnAudioAction::Ended(ref next_audio) = next_turn_actions[0] else {
            panic!("Expected Ended turn");
        };
        // 800 pre-roll samples + 1600 start chunk + 1600 end chunk = 4000 samples
        assert_eq!(next_audio.len(), 800 + 1600 + 1600);
        assert_eq!(&next_audio[..800], &small_chunk[..]);
    }

    #[test]
    fn turn_audio_ingest_event_convenience_helper() {
        let mut buffer = TurnAudioBuffer::new(800);
        assert!(buffer.ingest_event(&vec![1.0; 800], None).is_none());
        assert_eq!(
            buffer.ingest_event(&vec![2.0; 800], Some(VadEvent::SpeechStart)),
            Some(TurnAudioAction::Started)
        );
        let ended = buffer.ingest_event(&vec![3.0; 800], Some(VadEvent::SpeechEnd));
        let Some(TurnAudioAction::Ended(audio)) = ended else {
            panic!("Expected Ended turn from helper");
        };
        assert_eq!(audio.len(), 2400);
    }

    #[test]
    fn voice_session_audio_aec_reset_and_clear_render() {
        let session = VoiceSessionAudio::new(None, None, Some(SelfEchoCanceller::new()));
        let aec_handle = session.aec_handle();
        {
            let mut guard = aec_handle.lock().unwrap();
            let aec = guard.as_mut().unwrap();
            aec.push_render(&[0.5; 320], 16000);
            assert_eq!(aec.render_queue_len(), 320);
        }

        session.clear_aec_render();
        {
            let guard = aec_handle.lock().unwrap();
            let aec = guard.as_ref().unwrap();
            assert_eq!(aec.render_queue_len(), 0);
        }

        session.reset_aec();
    }

    #[test]
    fn test_turn_audio_buffer_silence_probe_and_force_end() {
        let mut buffer = TurnAudioBuffer::new(800);
        // Start speech
        let started = buffer.ingest(&vec![1.0; 800], &[VadEvent::SpeechStart]);
        assert_eq!(started, vec![TurnAudioAction::Started]);

        // Mid speech chunk
        let mid = buffer.ingest(&vec![2.0; 800], &[]);
        assert!(mid.is_empty());

        // Probe event at frame 6 (~192ms)
        let probe = buffer.ingest(
            &vec![0.0; 160],
            &[VadEvent::SilenceProbe {
                consecutive_silence_frames: 6,
            }],
        );
        assert_eq!(probe.len(), 1);
        let TurnAudioAction::SilenceProbe {
            consecutive_silence_frames,
            ref audio,
        } = probe[0]
        else {
            panic!("Expected SilenceProbe action");
        };
        assert_eq!(consecutive_silence_frames, 6);
        // 800 + 800 + 160 = 1760 samples
        assert_eq!(audio.len(), 1760);

        // Verify active buffer is STILL intact (not consumed by probe)
        assert!(buffer.active.is_some());

        // Stage 1 Fast Cutoff forces end
        let forced = buffer.force_end();
        assert!(forced.is_some());
        let forced_audio = forced.unwrap();
        assert_eq!(forced_audio.len(), 1760);
        assert!(buffer.active.is_none());
    }
}
