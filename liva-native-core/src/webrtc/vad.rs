use ort::{session::Session, value::Value};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    SpeechStart,
    SpeechEnd,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage0Metrics {
    pub rms_energy: f32,
    pub zcr: f32,
    pub is_active: bool,
}

/// Compute Root Mean Square (RMS) energy over PCM f32 samples (<10µs compute).
pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Compute Zero-Crossing Rate (ZCR) over PCM f32 samples (<10µs compute).
pub fn compute_zcr(samples: &[f32]) -> f32 {
    if samples.len() <= 1 {
        return 0.0;
    }
    let mut crossings = 0usize;
    let mut prev_sign = samples[0] >= 0.0;
    for &s in &samples[1..] {
        let sign = s >= 0.0;
        if sign != prev_sign {
            crossings += 1;
            prev_sign = sign;
        }
    }
    crossings as f32 / (samples.len() - 1) as f32
}

/// Stage 0 Instantaneous Energy & Zero-Crossing Rate (ZCR) pre-filter (<1ms compute).
pub fn compute_stage0_metrics(
    samples: &[f32],
    energy_threshold: f32,
    zcr_min: f32,
    zcr_max: f32,
) -> Stage0Metrics {
    let rms_energy = compute_rms(samples);
    let zcr = compute_zcr(samples);
    // Audio is considered active in Stage 0 if RMS energy exceeds acoustic noise floor
    // and either lies in the speech ZCR band or possesses high energy boost.
    let is_active = rms_energy >= energy_threshold
        && ((zcr >= zcr_min && zcr <= zcr_max) || rms_energy >= energy_threshold * 3.0);

    Stage0Metrics {
        rms_energy,
        zcr,
        is_active,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VadConfig {
    pub sample_rate: i64,
    pub frame_size: usize,
    pub threshold: f32,
    pub speech_start_threshold: usize,
    pub speech_end_threshold: usize,
    pub energy_threshold: f32,
    pub high_confidence_threshold: f32,
    pub fast_start_enabled: bool,
    pub zcr_min: f32,
    pub zcr_max: f32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            frame_size: 512,
            threshold: 0.5,
            speech_start_threshold: 3,
            speech_end_threshold: 45, // ~1.44s of silence at 32ms frame size
            energy_threshold: 0.001,
            high_confidence_threshold: 0.85,
            fast_start_enabled: true,
            zcr_min: 0.01,
            zcr_max: 0.50,
        }
    }
}

impl VadConfig {
    /// Ultra-low latency preset (10ms frame size / 160 samples at 16kHz).
    pub fn ultra_low_latency() -> Self {
        Self {
            sample_rate: 16000,
            frame_size: 160,
            threshold: 0.5,
            speech_start_threshold: 2,
            speech_end_threshold: 22,
            energy_threshold: 0.001,
            high_confidence_threshold: 0.85,
            fast_start_enabled: true,
            zcr_min: 0.01,
            zcr_max: 0.50,
        }
    }

    /// Fast responsive preset (16ms frame size / 256 samples at 16kHz).
    pub fn fast() -> Self {
        Self {
            sample_rate: 16000,
            frame_size: 256,
            threshold: 0.5,
            speech_start_threshold: 2,
            speech_end_threshold: 22,
            energy_threshold: 0.001,
            high_confidence_threshold: 0.85,
            fast_start_enabled: true,
            zcr_min: 0.01,
            zcr_max: 0.50,
        }
    }

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
        let get_f32 = |key: &str, d: f32| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d)
        };
        let get_bool = |key: &str, d: bool| {
            std::env::var(key)
                .ok()
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(d)
        };

        let frame_size = get_usize("LIVA_VAD_FRAME_SIZE", base.frame_size);
        let frame_size = if matches!(frame_size, 160 | 256 | 512) {
            frame_size
        } else {
            base.frame_size
        };

        Self {
            sample_rate: std::env::var("LIVA_VAD_SAMPLE_RATE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(base.sample_rate),
            frame_size,
            threshold: get_f32("LIVA_VAD_THRESHOLD", base.threshold),
            speech_start_threshold: get_usize("LIVA_VAD_START_FRAMES", base.speech_start_threshold),
            speech_end_threshold: get_usize("LIVA_VAD_END_FRAMES", 22),
            energy_threshold: get_f32("LIVA_VAD_ENERGY_THRESHOLD", base.energy_threshold),
            high_confidence_threshold: get_f32(
                "LIVA_VAD_HIGH_CONFIDENCE_THRESHOLD",
                base.high_confidence_threshold,
            ),
            fast_start_enabled: get_bool("LIVA_VAD_FAST_START", base.fast_start_enabled),
            zcr_min: get_f32("LIVA_VAD_ZCR_MIN", base.zcr_min),
            zcr_max: get_f32("LIVA_VAD_ZCR_MAX", base.zcr_max),
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

    // Real-time telemetry metrics
    last_stage0: Option<Stage0Metrics>,
    last_confidence: f32,
}

impl VadEngine {
    /// Initialize a new VAD Engine with the specified ONNX model path and config.
    pub fn new<P: AsRef<Path>>(model_path: P, config: VadConfig) -> Result<Self, String> {
        if config.sample_rate == 16000 && !matches!(config.frame_size, 160 | 256 | 512) {
            return Err(format!(
                "Unsupported frame_size {} for Silero VAD at 16kHz (supported: 160, 256, 512)",
                config.frame_size
            ));
        }

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
            last_stage0: None,
            last_confidence: 0.0,
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
            last_stage0: None,
            last_confidence: 0.0,
        }
    }

    /// Reset recurrent states and debounce counters.
    pub fn reset(&mut self) {
        self.state.fill(0.0);
        self.residual_buffer.clear();
        self.consecutive_speech_frames = 0;
        self.consecutive_silence_frames = 0;
        self.is_speaking = false;
        self.last_stage0 = None;
        self.last_confidence = 0.0;
    }

    pub fn config(&self) -> &VadConfig {
        &self.config
    }

    pub fn is_speaking(&self) -> bool {
        self.is_speaking
    }

    pub fn last_confidence(&self) -> f32 {
        self.last_confidence
    }

    pub fn last_stage0(&self) -> Option<Stage0Metrics> {
        self.last_stage0
    }

    /// Push raw PCM samples and process any complete frames (160, 256, or 512 samples).
    /// Returns a list of VAD events triggered during processing with their confidence score.
    pub fn process_audio(&mut self, samples: &[f32]) -> Result<Vec<(VadEvent, f32)>, String> {
        self.residual_buffer.extend_from_slice(samples);
        let mut events = Vec::new();
        let frame_size = self.config.frame_size;

        while self.residual_buffer.len() >= frame_size {
            let frame: Vec<f32> = self.residual_buffer.drain(0..frame_size).collect();

            // Stage 0: Instantaneous Energy & Zero-Crossing Rate pre-filtering (<1ms)
            let stage0 = compute_stage0_metrics(
                &frame,
                self.config.energy_threshold,
                self.config.zcr_min,
                self.config.zcr_max,
            );
            self.last_stage0 = Some(stage0);

            // Stage 1: Silero VAD v6 Neural Model Inference
            let (is_speech, confidence) = self.run_inference(&frame)?;
            self.last_confidence = confidence;

            // Two-Tier Decision and State Machine update
            if let Some(event) =
                self.update_state_machine_with_metrics(is_speech, confidence, Some(&stage0))
            {
                events.push((event, confidence));
            }
        }

        Ok(events)
    }

    /// Execute ONNX Runtime forward pass for a single frame.
    fn run_inference(&mut self, frame: &[f32]) -> Result<(bool, f32), String> {
        let (input_size, padded_frame) = if self.config.frame_size < 256 {
            let mut padded = frame.to_vec();
            padded.resize(256, 0.0);
            (256, padded)
        } else {
            (self.config.frame_size, frame.to_vec())
        };

        let inputs = ort::inputs![
            "input" => Value::from_array((vec![1, input_size], padded_frame)).map_err(|e| e.to_string())?,
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

    /// Process the debouncing state machine with two-tier metrics.
    fn update_state_machine_with_metrics(
        &mut self,
        is_speech: bool,
        confidence: f32,
        stage0: Option<&Stage0Metrics>,
    ) -> Option<VadEvent> {
        if is_speech {
            self.consecutive_speech_frames += 1;
            self.consecutive_silence_frames = 0;

            if !self.is_speaking {
                // Tier 1 Fast Trigger: High confidence (p >= 0.85) fires SpeechStart on frame 1
                let is_high_confidence = self.config.fast_start_enabled
                    && confidence >= self.config.high_confidence_threshold;

                // Tier 2 Energy Boosted Trigger: Acoustic energy surge + strong probability
                let is_energy_boosted = self.config.fast_start_enabled
                    && stage0.is_some_and(|m| {
                        m.rms_energy >= self.config.energy_threshold * 4.0 && confidence >= 0.70
                    });

                // Standard Debounce Trigger: N consecutive speech frames
                let meets_debounce =
                    self.consecutive_speech_frames >= self.config.speech_start_threshold;

                if is_high_confidence || is_energy_boosted || meets_debounce {
                    self.is_speaking = true;
                    return Some(VadEvent::SpeechStart);
                }
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

    /// Legacy / test-facing interface for standard boolean state machine evaluation.
    pub fn test_update_state_machine(&mut self, is_speech: bool) -> Option<VadEvent> {
        self.update_state_machine_with_metrics(
            is_speech,
            if is_speech {
                self.config.threshold
            } else {
                0.0
            },
            None,
        )
    }

    /// Test-facing interface for fine-grained multi-tier evaluation.
    pub fn test_update_state_machine_with_confidence(
        &mut self,
        is_speech: bool,
        confidence: f32,
        rms_energy: f32,
    ) -> Option<VadEvent> {
        let stage0 = Stage0Metrics {
            rms_energy,
            zcr: 0.05,
            is_active: rms_energy >= self.config.energy_threshold,
        };
        self.update_state_machine_with_metrics(is_speech, confidence, Some(&stage0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage0_metrics_computes_rms_and_zcr_accurately() {
        let silence = vec![0.0f32; 160];
        let m_silence = compute_stage0_metrics(&silence, 0.001, 0.01, 0.50);
        assert_eq!(m_silence.rms_energy, 0.0);
        assert_eq!(m_silence.zcr, 0.0);
        assert!(!m_silence.is_active);

        // 1kHz sine wave at 16kHz (period = 16 samples)
        let mut sine = Vec::with_capacity(160);
        for i in 0..160 {
            let val = (2.0 * std::f32::consts::PI * i as f32 / 16.0).sin() * 0.5;
            sine.push(val);
        }
        let m_sine = compute_stage0_metrics(&sine, 0.001, 0.01, 0.50);
        assert!(m_sine.rms_energy > 0.3, "Sine RMS should be ~0.35");
        assert!(
            m_sine.zcr > 0.05 && m_sine.zcr < 0.20,
            "Sine ZCR in expected range"
        );
        assert!(m_sine.is_active);
    }

    #[test]
    fn fast_start_triggers_speech_start_on_single_high_confidence_frame() {
        let model_path = resolve_model_path("models/nemotron-asr");
        if !model_path.exists() {
            eprintln!("skip: Silero VAD model not present");
            return;
        }

        let mut engine =
            VadEngine::new(&model_path, VadConfig::default()).expect("load Silero VAD model");
        assert!(!engine.is_speaking());

        // High confidence (p=0.90 >= 0.85) should trigger immediately on frame 1
        let event = engine.test_update_state_machine_with_confidence(true, 0.90, 0.01);
        assert_eq!(
            event,
            Some(VadEvent::SpeechStart),
            "Single high-confidence frame must trigger SpeechStart immediately"
        );
        assert!(engine.is_speaking());
    }

    #[test]
    fn standard_debounce_triggers_speech_start_after_n_frames() {
        let model_path = resolve_model_path("models/nemotron-asr");
        if !model_path.exists() {
            eprintln!("skip: Silero VAD model not present");
            return;
        }

        let mut engine =
            VadEngine::new(&model_path, VadConfig::default()).expect("load Silero VAD model");
        assert!(!engine.is_speaking());

        // Low confidence (p=0.55 < 0.85) requires 3 frames
        assert_eq!(
            engine.test_update_state_machine_with_confidence(true, 0.55, 0.002),
            None
        );
        assert_eq!(
            engine.test_update_state_machine_with_confidence(true, 0.55, 0.002),
            None
        );
        assert_eq!(
            engine.test_update_state_machine_with_confidence(true, 0.55, 0.002),
            Some(VadEvent::SpeechStart)
        );
        assert!(engine.is_speaking());
    }

    #[test]
    fn multi_frame_sizes_160_256_512_run_inference_successfully() {
        let model_path = resolve_model_path("models/nemotron-asr");
        if !model_path.exists() {
            eprintln!("skip: Silero VAD model not present");
            return;
        }

        // Test 160-sample (10ms) frame size
        let config_160 = VadConfig {
            frame_size: 160,
            ..VadConfig::ultra_low_latency()
        };
        let mut engine_160 =
            VadEngine::new(&model_path, config_160).expect("load 160-sample VAD engine");
        let res_160 = engine_160.process_audio(&vec![0.0f32; 160 * 3]);
        assert!(
            res_160.is_ok(),
            "160-sample frame processing failed: {:?}",
            res_160.err()
        );

        // Test 256-sample (16ms) frame size
        let config_256 = VadConfig {
            frame_size: 256,
            ..VadConfig::fast()
        };
        let mut engine_256 =
            VadEngine::new(&model_path, config_256).expect("load 256-sample VAD engine");
        let res_256 = engine_256.process_audio(&vec![0.0f32; 256 * 3]);
        assert!(res_256.is_ok(), "256-sample frame processing failed");

        // Test 512-sample (32ms) frame size
        let config_512 = VadConfig::default();
        let mut engine_512 =
            VadEngine::new(&model_path, config_512).expect("load 512-sample VAD engine");
        let res_512 = engine_512.process_audio(&vec![0.0f32; 512 * 3]);
        assert!(res_512.is_ok(), "512-sample frame processing failed");
    }

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
