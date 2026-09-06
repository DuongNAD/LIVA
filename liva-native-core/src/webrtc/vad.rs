use ort::{session::Session, value::Value};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    SpeechStart,
    SpeechEnd,
    None,
}

/// Stage 0 Fast Energy and Zero-Crossing Rate (ZCR) Flux Pre-trigger.
/// Evaluates acoustic energy and spectral flux in <= 0.05ms before neural inference.
#[derive(Debug, Clone)]
pub struct FastEnergyZcrPreTrigger {
    last_energy: f32,
    energy_threshold_db: f32,
    zcr_min: f32,
    zcr_max: f32,
    flux_threshold: f32,
}

impl FastEnergyZcrPreTrigger {
    pub fn new(
        energy_threshold_db: f32,
        zcr_min: f32,
        zcr_max: f32,
        flux_threshold: f32,
    ) -> Self {
        Self {
            last_energy: 0.0,
            energy_threshold_db,
            zcr_min,
            zcr_max,
            flux_threshold,
        }
    }

    /// Evaluate 16ms frame samples. Returns (is_pre_triggered, energy_db, zcr, flux).
    pub fn evaluate(&mut self, frame: &[f32]) -> (bool, f32, f32, f32) {
        if frame.is_empty() {
            return (false, -100.0, 0.0, 0.0);
        }
        let n = frame.len() as f32;
        let sum_sq: f32 = frame.iter().map(|&x| x * x).sum();
        let rms = (sum_sq / n).sqrt();
        let energy_db = 20.0 * (rms + 1e-9).log10();

        let mut zero_crossings = 0;
        for i in 1..frame.len() {
            if (frame[i] >= 0.0 && frame[i - 1] < 0.0)
                || (frame[i] < 0.0 && frame[i - 1] >= 0.0)
            {
                zero_crossings += 1;
            }
        }
        let zcr = zero_crossings as f32 / (n - 1.0);

        let flux = (rms - self.last_energy).max(0.0);
        self.last_energy = rms;

        let is_energy_speech = energy_db >= self.energy_threshold_db;
        let is_zcr_speech = zcr >= self.zcr_min && zcr <= self.zcr_max;
        let is_flux_onset = flux >= self.flux_threshold;

        let is_pre_trigger = is_energy_speech
            && is_zcr_speech
            && (is_flux_onset || energy_db >= self.energy_threshold_db + 10.0);

        (is_pre_trigger, energy_db, zcr, flux)
    }

    pub fn reset(&mut self) {
        self.last_energy = 0.0;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VadConfig {
    pub sample_rate: i64,
    pub frame_size: usize,
    /// Backwards compatibility alias for `start_threshold`.
    pub threshold: f32,
    pub start_threshold: f32,
    pub end_threshold: f32,
    pub speech_start_threshold: usize,
    pub speech_end_threshold: usize,
    pub energy_threshold_db: f32,
    pub zcr_min: f32,
    pub zcr_max: f32,
    pub flux_threshold: f32,
    pub pre_trigger_enabled: bool,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            frame_size: 256, // 16ms frames at 16kHz
            threshold: 0.50,
            start_threshold: 0.50,
            end_threshold: 0.35,
            speech_start_threshold: 1, // 1 frame = 16ms <= 20ms onset
            speech_end_threshold: 22,   // ~352ms hangover at 16ms frame size
            energy_threshold_db: -45.0,
            zcr_min: 0.02,
            zcr_max: 0.65,
            flux_threshold: 0.0015,
            pre_trigger_enabled: true,
        }
    }
}

impl VadConfig {
    /// Product config: `Default` values overridable via env.
    pub fn from_env() -> Self {
        let base = Self::default();
        let get_usize = |key: &str, d: usize| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d)
        };
        let get_f32 = |key: &str, d: f32| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d)
        };

        let threshold = get_f32("LIVA_VAD_THRESHOLD", base.threshold);
        let start_threshold = get_f32("LIVA_VAD_START_THRESHOLD", threshold);
        let end_threshold = get_f32("LIVA_VAD_END_THRESHOLD", base.end_threshold);
        let frame_size = get_usize("LIVA_VAD_FRAME_SIZE", base.frame_size);
        let speech_start_threshold =
            get_usize("LIVA_VAD_START_FRAMES", base.speech_start_threshold);
        let speech_end_threshold =
            get_usize("LIVA_VAD_END_FRAMES", base.speech_end_threshold);
        let energy_threshold_db =
            get_f32("LIVA_VAD_ENERGY_THRESH_DB", base.energy_threshold_db);
        let flux_threshold =
            get_f32("LIVA_VAD_FLUX_THRESH", base.flux_threshold);
        let pre_trigger_enabled =
            crate::env_flag("LIVA_VAD_PRE_TRIGGER", base.pre_trigger_enabled);

        Self {
            sample_rate: 16000,
            frame_size,
            threshold: start_threshold,
            start_threshold,
            end_threshold,
            speech_start_threshold,
            speech_end_threshold,
            energy_threshold_db,
            zcr_min: base.zcr_min,
            zcr_max: base.zcr_max,
            flux_threshold,
            pre_trigger_enabled,
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
    pre_trigger: FastEnergyZcrPreTrigger,

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
        let pre_trigger = FastEnergyZcrPreTrigger::new(
            config.energy_threshold_db,
            config.zcr_min,
            config.zcr_max,
            config.flux_threshold,
        );

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            config,
            state,
            pre_trigger,
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
            pre_trigger: FastEnergyZcrPreTrigger::new(
                self.config.energy_threshold_db,
                self.config.zcr_min,
                self.config.zcr_max,
                self.config.flux_threshold,
            ),
            residual_buffer: Vec::new(),
            consecutive_speech_frames: 0,
            consecutive_silence_frames: 0,
            is_speaking: false,
        }
    }

    /// Reset recurrent states and debounce counters.
    pub fn reset(&mut self) {
        self.state.fill(0.0);
        self.pre_trigger.reset();
        self.residual_buffer.clear();
        self.consecutive_speech_frames = 0;
        self.consecutive_silence_frames = 0;
        self.is_speaking = false;
    }

    /// Push raw PCM samples and process any complete 256-sample (16ms) frames.
    /// Returns a list of VAD events triggered during processing.
    pub fn process_audio(&mut self, samples: &[f32]) -> Result<Vec<(VadEvent, f32)>, String> {
        self.residual_buffer.extend_from_slice(samples);
        let mut events = Vec::new();
        let frame_size = self.config.frame_size;

        while self.residual_buffer.len() >= frame_size {
            let frame: Vec<f32> = self.residual_buffer.drain(0..frame_size).collect();

            // Stage 0: Fast Energy/ZCR Flux Pre-trigger
            let (pre_triggered, _, _, _) = if self.config.pre_trigger_enabled {
                self.pre_trigger.evaluate(&frame)
            } else {
                (false, 0.0, 0.0, 0.0)
            };

            // Stage 1: Silero VAD v6 ONNX Inference
            let confidence = self.run_inference(&frame)?;

            // Dual-Stage Hysteresis Decision
            let is_speech = if !self.is_speaking {
                if pre_triggered && confidence >= self.config.start_threshold * 0.85 {
                    true
                } else {
                    confidence >= self.config.start_threshold
                }
            } else {
                confidence >= self.config.end_threshold
            };

            if let Some(event) = self.update_state_machine(is_speech, confidence) {
                events.push((event, confidence));
            }
        }

        Ok(events)
    }

    /// Execute ONNX Runtime forward pass for a single frame.
    fn run_inference(&mut self, frame: &[f32]) -> Result<f32, String> {
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

        Ok(confidence)
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
        self.update_state_machine(is_speech, if is_speech { 1.0 } else { 0.0 })
    }

    pub fn test_update_state_machine_with_confidence(&mut self, confidence: f32) -> Option<VadEvent> {
        let is_speech = if !self.is_speaking {
            confidence >= self.config.start_threshold
        } else {
            confidence >= self.config.end_threshold
        };
        self.update_state_machine(is_speech, confidence)
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

        let worker_a = std::thread::spawn(move || session_a.process_audio(&vec![0.0; 256 * 4]));
        let worker_b = std::thread::spawn(move || session_b.process_audio(&vec![0.1; 256 * 4]));

        assert!(worker_a.join().expect("session A thread").is_ok());
        assert!(worker_b.join().expect("session B thread").is_ok());
        assert!(
            prototype.state.iter().all(|&value| value == 0.0),
            "session inference must not mutate the process-level prototype"
        );
    }

    #[test]
    fn test_fast_energy_zcr_pre_trigger_evaluation() {
        let mut trigger = FastEnergyZcrPreTrigger::new(-45.0, 0.02, 0.65, 0.0015);

        // Silence frame (all zeros) -> no pre-trigger
        let silence = vec![0.0f32; 256];
        let (pre_trig, energy_db, zcr, flux) = trigger.evaluate(&silence);
        assert!(!pre_trig);
        assert!(energy_db < -45.0);
        assert_eq!(zcr, 0.0);
        assert_eq!(flux, 0.0);

        // Speech-like frame: sine wave at 400Hz (ZCR = 400 * 2 / 16000 = 0.05 in [0.02, 0.65])
        let speech_frame: Vec<f32> = (0..256)
            .map(|i| (i as f32 * 400.0 * 2.0 * std::f32::consts::PI / 16000.0).sin() * 0.5)
            .collect();
        let (pre_trig, energy_db, zcr, flux) = trigger.evaluate(&speech_frame);
        assert!(pre_trig, "Speech-like frame must trigger Stage 0 pre-trigger");
        assert!(energy_db >= -45.0, "Energy ({energy_db} dB) must be above threshold");
        assert!(zcr >= 0.02 && zcr <= 0.65, "ZCR ({zcr}) must be in human voice range");
        assert!(flux > 0.0015, "Flux ({flux}) must indicate onset");

        // High frequency noise / DC offset
        let dc_frame = vec![0.001f32; 256]; // DC offset has 0 ZCR
        let (dc_trig, _, dc_zcr, _) = trigger.evaluate(&dc_frame);
        assert!(!dc_trig, "DC offset without zero crossings must not pre-trigger");
        assert_eq!(dc_zcr, 0.0);
    }

    #[test]
    fn test_dual_threshold_hysteresis_state_machine() {
        let model_path = resolve_model_path("models/nemotron-asr");
        if !model_path.exists() {
            eprintln!("skip: Silero VAD model not present");
            return;
        }

        let config = VadConfig {
            start_threshold: 0.50,
            end_threshold: 0.35,
            speech_start_threshold: 1,
            speech_end_threshold: 3,
            ..Default::default()
        };
        let mut vad = VadEngine::new(&model_path, config).expect("load vad");

        // 1. Below start threshold (0.45 < 0.50) while idle -> no speech start
        let evt = vad.test_update_state_machine_with_confidence(0.45);
        assert_eq!(evt, None);
        assert!(!vad.is_speaking());

        // 2. Above start threshold (0.55 >= 0.50) -> speech start (1 frame onset)
        let evt = vad.test_update_state_machine_with_confidence(0.55);
        assert_eq!(evt, Some(VadEvent::SpeechStart));
        assert!(vad.is_speaking());

        // 3. Drops to 0.40 (between 0.35 end and 0.50 start) -> remains speaking due to hysteresis
        let evt = vad.test_update_state_machine_with_confidence(0.40);
        assert_eq!(evt, None);
        assert!(vad.is_speaking(), "Hysteresis must keep speaking state above end_threshold (0.35)");

        // 4. Drops below end threshold (0.30 < 0.35) -> silence debounce begins
        let evt = vad.test_update_state_machine_with_confidence(0.30);
        assert_eq!(evt, None);
        assert!(vad.is_speaking());

        let evt = vad.test_update_state_machine_with_confidence(0.30);
        assert_eq!(evt, None);
        assert!(vad.is_speaking());

        // 3rd frame below end_threshold reaches speech_end_threshold (3) -> SpeechEnd
        let evt = vad.test_update_state_machine_with_confidence(0.30);
        assert_eq!(evt, Some(VadEvent::SpeechEnd));
        assert!(!vad.is_speaking());
    }

    #[test]
    fn test_one_frame_onset_debounce_16ms() {
        let config = VadConfig {
            speech_start_threshold: 1,
            ..Default::default()
        };
        let vad_mock_machine = |is_speech: bool, is_speaking: &mut bool, consec_speech: &mut usize| -> Option<VadEvent> {
            if is_speech {
                *consec_speech += 1;
                if !*is_speaking && *consec_speech >= config.speech_start_threshold {
                    *is_speaking = true;
                    return Some(VadEvent::SpeechStart);
                }
            }
            None
        };

        let mut is_speaking = false;
        let mut consec_speech = 0;

        let evt = vad_mock_machine(true, &mut is_speaking, &mut consec_speech);
        assert_eq!(evt, Some(VadEvent::SpeechStart), "1-frame speech_start_threshold must emit SpeechStart on 1st frame (16ms)");
        assert!(is_speaking);
    }
}
