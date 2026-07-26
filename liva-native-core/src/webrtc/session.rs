use crate::AppState;
use crate::webrtc::aec::SelfEchoCanceller;
use crate::webrtc::denoise::GtcrnDenoiser;
use crate::webrtc::turn_shadow::SmartTurnClassifier;
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

impl VoiceRuntimeConfig {
    pub fn from_env() -> Self {
        Self {
            vad_enabled: crate::env_flag("LIVA_VAD_ENABLED", true),
            denoise_enabled: crate::env_flag("LIVA_DENOISE_ENABLED", true),
            turn_shadow_enabled: crate::env_flag("LIVA_TURN_SHADOW_ENABLED", false),
            aec_enabled: crate::env_flag("LIVA_AEC_ENABLED", false),
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
}

/// Assembles one utterance at chunk granularity and retains bounded idle audio
/// as historical pre-roll for the next `SpeechStart`.
pub struct TurnAudioBuffer {
    pre_roll: VecDeque<f32>,
    active: Option<Vec<f32>>,
    pre_roll_capacity: usize,
}

impl TurnAudioBuffer {
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
                VadEvent::SpeechEnd if self.active.is_some() => {
                    if !chunk_attached {
                        self.active
                            .as_mut()
                            .expect("active turn checked above")
                            .extend_from_slice(chunk);
                        chunk_attached = true;
                    }
                    actions.push(TurnAudioAction::Ended(
                        self.active.take().expect("active turn checked above"),
                    ));
                }
                _ => {}
            }
        }

        if !chunk_attached && let Some(active) = self.active.as_mut() {
            active.extend_from_slice(chunk);
        }
        if self.active.is_none() && !chunk_attached {
            self.push_pre_roll(chunk);
        }

        actions
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
}

/// DSP state owned by exactly one WebSocket connection.
///
/// VAD/GTCRN forks share their immutable loaded model, but all recurrent,
/// buffering, debounce, and AEC state is connection-local.
#[derive(Clone)]
pub struct VoiceSessionAudio {
    vad: Arc<Mutex<Option<VadEngine>>>,
    denoiser: Arc<Mutex<Option<GtcrnDenoiser>>>,
    aec: SessionAec,
}

impl VoiceSessionAudio {
    fn new(
        vad: Option<VadEngine>,
        denoiser: Option<GtcrnDenoiser>,
        aec: Option<SelfEchoCanceller>,
    ) -> Self {
        Self {
            vad: Arc::new(Mutex::new(vad)),
            denoiser: Arc::new(Mutex::new(denoiser)),
            aec: Arc::new(Mutex::new(aec)),
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
        let aec = state
            .aec
            .lock()
            .await
            .as_ref()
            .map(|_| SelfEchoCanceller::new());

        Self::new(vad, denoiser, aec)
    }

    pub fn aec_handle(&self) -> SessionAec {
        Arc::clone(&self.aec)
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
        let mut buffer = TurnAudioBuffer::new(3);
        assert!(buffer.ingest(&[1.0, 2.0, 3.0], &[]).is_empty());

        let started = buffer.ingest(&[4.0, 5.0], &[VadEvent::SpeechStart]);
        assert!(matches!(started.as_slice(), [TurnAudioAction::Started]));
        assert!(buffer.ingest(&[6.0], &[]).is_empty());

        let ended = buffer.ingest(&[7.0, 8.0], &[VadEvent::SpeechEnd]);
        let [TurnAudioAction::Ended(audio)] = ended.as_slice() else {
            panic!("SpeechEnd must emit exactly one completed turn");
        };

        assert_eq!(audio, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);

        assert!(buffer.ingest(&[9.0], &[]).is_empty());
        assert!(matches!(
            buffer.ingest(&[10.0], &[VadEvent::SpeechStart]).as_slice(),
            [TurnAudioAction::Started]
        ));
        let next_ended = buffer.ingest(&[11.0], &[VadEvent::SpeechEnd]);
        let [TurnAudioAction::Ended(next_audio)] = next_ended.as_slice() else {
            panic!("next SpeechEnd must emit exactly one completed turn");
        };

        assert_eq!(next_audio, &[9.0, 10.0, 11.0]);
    }
}
