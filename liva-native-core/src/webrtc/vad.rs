use ort::{session::Session, value::Value};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    SpeechStart,
    SpeechEnd,
    None,
}

#[derive(Clone, Copy)]
pub struct VadConfig {
    pub sample_rate: i64,
    pub frame_size: usize,
    pub threshold: f32,
    pub speech_start_threshold: usize,
    pub speech_end_threshold: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            frame_size: 512,
            threshold: 0.5,
            speech_start_threshold: 3,
            speech_end_threshold: 45, // ~1.44s of silence at 32ms frame size
        }
    }
}

impl VadConfig {
    /// Product config: `Default` values overridable via env, with a snappier
    /// end-of-turn (22 frames ≈ 0.7s vs the conservative 1.44s default) so
    /// barge-in and turn-taking feel responsive.
    pub fn from_env() -> Self {
        let base = Self::default();
        let get_usize = |key: &str, d: usize| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d)
        };
        Self {
            threshold: std::env::var("LIVA_VAD_THRESHOLD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(base.threshold),
            speech_start_threshold: get_usize("LIVA_VAD_START_FRAMES", base.speech_start_threshold),
            speech_end_threshold: get_usize("LIVA_VAD_END_FRAMES", 22),
            ..base
        }
    }
}

/// Resolve the Silero VAD model path shared by the server and verify bins.
///
/// Priority: `LIVA_VAD_MODEL_PATH` env (honored even if missing, so the
/// caller's "not found" error names the user's explicit choice) →
/// standalone v6 model `models/silero_vad_v6.onnx` (kept outside the
/// nemotron-asr nested repo; `../` variant for bins run from
/// `liva-native-core/`) → legacy copy bundled in the STT model dir.
pub fn resolve_model_path(stt_model_dir: &str) -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(p) = std::env::var("LIVA_VAD_MODEL_PATH")
        && !p.trim().is_empty()
    {
        return PathBuf::from(p);
    }
    for candidate in [
        PathBuf::from("models/silero_vad_v6.onnx"),
        PathBuf::from("../models/silero_vad_v6.onnx"),
        // liva-desktop/src-tauri is two levels below the repo root
        PathBuf::from("../../models/silero_vad_v6.onnx"),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    std::path::Path::new(stt_model_dir).join("silero_vad.onnx")
}

pub struct VadEngine {
    /// The immutable ONNX model is shared across WebSocket sessions. `Session::run`
    /// needs mutable access, so inference is serialized while recurrent state stays
    /// on each `VadEngine` fork below.
    session: Arc<Mutex<Session>>,
    config: VadConfig,
    state: Vec<f32>, // recurrent state buffer [2, 1, 128]

    // Internal audio buffering
    residual_buffer: Vec<f32>,

    // Debounce counters
    consecutive_speech_frames: usize,
    consecutive_silence_frames: usize,
    is_speaking: bool,
}

impl VadEngine {
    /// Initialize a new VAD Engine with the specified ONNX model path and config.
    pub fn new<P: AsRef<Path>>(model_path: P, config: VadConfig) -> Result<Self, String> {
        let session = Session::builder()
            .map_err(|e| format!("Failed to create SessionBuilder: {}", e))?
            .with_intra_threads(1)
            .map_err(|e| format!("Failed to configure intra threads: {}", e))?
            .with_inter_threads(1)
            .map_err(|e| format!("Failed to configure inter threads: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| format!("Failed to load VAD ONNX model: {}", e))?;

        // Initialize state to 0.0 with dimension [2, 1, 128] = 256 floats
        let state = vec![0.0f32; 2 * 128];

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            config,
            state,
            residual_buffer: Vec::new(),
            consecutive_speech_frames: 0,
            consecutive_silence_frames: 0,
            is_speaking: false,
        })
    }

    /// Create a stream-local VAD state while reusing the already-loaded ONNX model.
    pub fn fork_session(&self) -> Self {
        Self {
            session: Arc::clone(&self.session),
            config: self.config,
            state: vec![0.0; 2 * 128],
            residual_buffer: Vec::new(),
            consecutive_speech_frames: 0,
            consecutive_silence_frames: 0,
            is_speaking: false,
        }
    }

    /// Reset recurrent states and debounce counters.
    pub fn reset(&mut self) {
        self.state.fill(0.0);
        self.residual_buffer.clear();
        self.consecutive_speech_frames = 0;
        self.consecutive_silence_frames = 0;
        self.is_speaking = false;
    }

    /// Push raw PCM samples and process any complete 512-sample frames.
    /// Returns a list of VAD events triggered during processing.
    pub fn process_audio(&mut self, samples: &[f32]) -> Result<Vec<(VadEvent, f32)>, String> {
        self.residual_buffer.extend_from_slice(samples);
        let mut events = Vec::new();
        let frame_size = self.config.frame_size;

        while self.residual_buffer.len() >= frame_size {
            // Drain 512 samples
            let frame: Vec<f32> = self.residual_buffer.drain(0..frame_size).collect();

            // Run model inference
            let (is_speech, confidence) = self.run_inference(&frame)?;

            // Process debounce logic
            if let Some(event) = self.update_state_machine(is_speech, confidence) {
                events.push((event, confidence));
            }
        }

        Ok(events)
    }

    /// Execute ONNX Runtime forward pass for a single frame.
    fn run_inference(&mut self, frame: &[f32]) -> Result<(bool, f32), String> {
        let inputs = ort::inputs![
            "input" => Value::from_array((vec![1, self.config.frame_size], frame.to_vec())).map_err(|e| e.to_string())?,
            "sr" => Value::from_array((vec![1], vec![self.config.sample_rate])).map_err(|e| e.to_string())?,
            "state" => Value::from_array((vec![2, 1, 128], self.state.clone())).map_err(|e| e.to_string())?,
        ];

        let mut session = self
            .session
            .lock()
            .map_err(|_| "VAD ONNX session mutex poisoned".to_string())?;
        let outputs = session
            .run(inputs)
            .map_err(|e| format!("ONNX VAD run failed: {}", e))?;

        // 1. Extract Speech Confidence
        let output_tensor = outputs
            .get("output")
            .ok_or_else(|| "Missing output tensor".to_string())?;
        let (_, output_data) = output_tensor
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract output tensor: {}", e))?;
        let confidence = output_data[0];

        // 2. Extract and Update LSTM States
        let state_n_tensor = outputs
            .get("stateN")
            .ok_or_else(|| "Missing stateN tensor".to_string())?;
        let (_, state_n_data) = state_n_tensor
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract stateN tensor: {}", e))?;

        self.state.copy_from_slice(state_n_data);

        let is_speech = confidence >= self.config.threshold;
        Ok((is_speech, confidence))
    }

    /// Process the debouncing state machine.
    fn update_state_machine(&mut self, is_speech: bool, _confidence: f32) -> Option<VadEvent> {
        if is_speech {
            self.consecutive_speech_frames += 1;
            self.consecutive_silence_frames = 0;

            if !self.is_speaking
                && self.consecutive_speech_frames >= self.config.speech_start_threshold
            {
                self.is_speaking = true;
                return Some(VadEvent::SpeechStart);
            }
        } else {
            self.consecutive_silence_frames += 1;
            self.consecutive_speech_frames = 0;

            if self.is_speaking
                && self.consecutive_silence_frames >= self.config.speech_end_threshold
            {
                self.is_speaking = false;
                return Some(VadEvent::SpeechEnd);
            }
        }
        None
    }

    pub fn test_update_state_machine(&mut self, is_speech: bool) -> Option<VadEvent> {
        self.update_state_machine(is_speech, 0.0)
    }

    pub fn is_speaking(&self) -> bool {
        self.is_speaking
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_session_starts_with_independent_stream_state() {
        let model_path = resolve_model_path("models/nemotron-asr");
        if !model_path.exists() {
            eprintln!("skip: Silero VAD model not present");
            return;
        }

        let mut source =
            VadEngine::new(&model_path, VadConfig::default()).expect("load Silero VAD model");
        for _ in 0..source.config.speech_start_threshold {
            let _ = source.test_update_state_machine(true);
        }
        assert!(source.is_speaking());

        let fork = source.fork_session();

        assert!(
            Arc::ptr_eq(&source.session, &fork.session),
            "forks should reuse the loaded ONNX model"
        );
        assert!(
            source.is_speaking(),
            "fork must not reset the source session"
        );
        assert!(!fork.is_speaking(), "fork must start outside an utterance");
        assert!(fork.state.iter().all(|&value| value == 0.0));
        assert!(fork.residual_buffer.is_empty());
        assert_eq!(fork.consecutive_speech_frames, 0);
        assert_eq!(fork.consecutive_silence_frames, 0);
    }

    #[test]
    fn forked_sessions_can_run_shared_model_concurrently() {
        let model_path = resolve_model_path("models/nemotron-asr");
        if !model_path.exists() {
            eprintln!("skip: Silero VAD model not present");
            return;
        }

        let prototype =
            VadEngine::new(&model_path, VadConfig::default()).expect("load Silero VAD model");
        let mut session_a = prototype.fork_session();
        let mut session_b = prototype.fork_session();

        let worker_a = std::thread::spawn(move || session_a.process_audio(&vec![0.0; 512 * 4]));
        let worker_b = std::thread::spawn(move || session_b.process_audio(&vec![0.1; 512 * 4]));

        assert!(worker_a.join().expect("session A thread").is_ok());
        assert!(worker_b.join().expect("session B thread").is_ok());
        assert!(
            prototype.state.iter().all(|&value| value == 0.0),
            "session inference must not mutate the process-level prototype"
        );
    }
}
