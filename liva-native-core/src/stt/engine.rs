use ort::{session::Session, value::Value};
use std::path::Path;

pub struct SttEngine {
    encoder_session: Session,
    decoder_session: Session,
    joint_session: Session,

    // Encoder Cache State
    cache_last_channel: Vec<f32>,
    cache_last_time: Vec<f32>,
    cache_last_channel_len: Vec<i64>,

    // Decoder LSTM State
    decoder_hidden_state: Vec<f32>,
    decoder_cell_state: Vec<f32>,
    last_decoder_token: i64,

    blank_id: i64,
    lang_id: i64,
    cached_decoder_output: Vec<f32>,
}

impl SttEngine {
    pub fn new<P: AsRef<Path>>(model_dir: P) -> Result<Self, String> {
        let encoder_path = model_dir.as_ref().join("encoder.onnx");
        let decoder_path = model_dir.as_ref().join("decoder.onnx");
        let joint_path = model_dir.as_ref().join("joint.onnx");

        if !encoder_path.exists() || !decoder_path.exists() || !joint_path.exists() {
            return Err("Nemotron ONNX model files missing in specified directory".to_string());
        }

        // Initialize ONNX sessions on CPU
        let encoder_session = Session::builder()
            .map_err(|e| format!("Failed to create session builder: {}", e))?
            .with_intra_threads(2)
            .map_err(|e| format!("Failed to set intra threads: {}", e))?
            .with_inter_threads(1)
            .map_err(|e| format!("Failed to set inter threads: {}", e))?
            .commit_from_file(&encoder_path)
            .map_err(|e| format!("Failed to load encoder: {}", e))?;

        let decoder_session = Session::builder()
            .map_err(|e| format!("Failed to create session builder: {}", e))?
            .with_intra_threads(2)
            .map_err(|e| format!("Failed to set intra threads: {}", e))?
            .with_inter_threads(1)
            .map_err(|e| format!("Failed to set inter threads: {}", e))?
            .commit_from_file(&decoder_path)
            .map_err(|e| format!("Failed to load decoder: {}", e))?;

        let joint_session = Session::builder()
            .map_err(|e| format!("Failed to create session builder: {}", e))?
            .with_intra_threads(2)
            .map_err(|e| format!("Failed to set intra threads: {}", e))?
            .with_inter_threads(1)
            .map_err(|e| format!("Failed to set inter threads: {}", e))?
            .commit_from_file(&joint_path)
            .map_err(|e| format!("Failed to load joint: {}", e))?;

        let mut engine = Self {
            encoder_session,
            decoder_session,
            joint_session,
            cache_last_channel: vec![0.0; 24 * 56 * 1024],
            cache_last_time: vec![0.0; 24 * 1024 * 8],
            cache_last_channel_len: vec![0; 1],
            decoder_hidden_state: vec![0.0; 2 * 640],
            decoder_cell_state: vec![0.0; 2 * 640],
            last_decoder_token: 13087,
            blank_id: 13087,
            lang_id: super::lang::DEFAULT_LANG_ID,
            cached_decoder_output: vec![0.0; 640],
        };

        engine.reset_states();
        Ok(engine)
    }

    /// Set the encoder language conditioning id (see `stt::lang`).
    /// Takes effect from the next chunk; does not reset stream state.
    pub fn set_lang_id(&mut self, id: i64) {
        self.lang_id = id;
    }

    pub fn lang_id(&self) -> i64 {
        self.lang_id
    }

    pub fn reset_states(&mut self) {
        self.cache_last_channel.fill(0.0);
        self.cache_last_time.fill(0.0);
        self.cache_last_channel_len.fill(0);

        self.decoder_hidden_state.fill(0.0);
        self.decoder_cell_state.fill(0.0);
        self.last_decoder_token = self.blank_id;

        // Run decoder session once on blank ID with zeroed states to bootstrap cache
        let decoder_inputs = ort::inputs![
            "targets" => Value::from_array((vec![1, 1], vec![self.last_decoder_token])).expect("Failed to create target value"),
            "h_in" => Value::from_array((vec![2, 1, 640], self.decoder_hidden_state.clone())).expect("Failed to create h_in value"),
            "c_in" => Value::from_array((vec![2, 1, 640], self.decoder_cell_state.clone())).expect("Failed to create c_in value"),
        ];

        let decoder_outputs = self
            .decoder_session
            .run(decoder_inputs)
            .expect("Decoder bootstrapping failed during reset_states");

        let h_out_val = decoder_outputs
            .get("h_out")
            .expect("Missing h_out from decoder during reset_states");
        let (_, h_out_data) = h_out_val
            .try_extract_tensor::<f32>()
            .expect("Failed to extract h_out from decoder during reset_states");
        self.decoder_hidden_state = h_out_data.to_vec();

        let c_out_val = decoder_outputs
            .get("c_out")
            .expect("Missing c_out from decoder during reset_states");
        let (_, c_out_data) = c_out_val
            .try_extract_tensor::<f32>()
            .expect("Failed to extract c_out from decoder during reset_states");
        self.decoder_cell_state = c_out_data.to_vec();

        let decoder_out_val = decoder_outputs
            .get("decoder_output")
            .expect("Missing decoder_output from decoder during reset_states");
        let (_, decoder_out_data) = decoder_out_val
            .try_extract_tensor::<f32>()
            .expect("Failed to extract decoder_output from decoder during reset_states");
        self.cached_decoder_output = decoder_out_data.to_vec();
    }

    pub fn run_chunk(&mut self, log_mel: &[f32], num_frames: usize) -> Result<Vec<u32>, String> {
        let encoder_inputs = ort::inputs![
            "audio_signal" => Value::from_array((vec![1, num_frames, 128], log_mel.to_vec())).map_err(|e| e.to_string())?,
            "length" => Value::from_array((vec![1], vec![num_frames as i64])).map_err(|e| e.to_string())?,
            "cache_last_channel" => Value::from_array((vec![1, 24, 56, 1024], self.cache_last_channel.clone())).map_err(|e| e.to_string())?,
            "cache_last_time" => Value::from_array((vec![1, 24, 1024, 8], self.cache_last_time.clone())).map_err(|e| e.to_string())?,
            "cache_last_channel_len" => Value::from_array((vec![1], self.cache_last_channel_len.clone())).map_err(|e| e.to_string())?,
            "lang_id" => Value::from_array((vec![1], vec![self.lang_id])).map_err(|e| e.to_string())?,
        ];

        let encoder_outputs = self
            .encoder_session
            .run(encoder_inputs)
            .map_err(|e| format!("Encoder run failed: {}", e))?;

        // Update encoder cache states
        let next_channel_val = encoder_outputs
            .get("cache_last_channel_next")
            .ok_or_else(|| "Missing cache_last_channel_next".to_string())?;
        let (_, next_channel_data) = next_channel_val
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        self.cache_last_channel = next_channel_data.to_vec();

        let next_time_val = encoder_outputs
            .get("cache_last_time_next")
            .ok_or_else(|| "Missing cache_last_time_next".to_string())?;
        let (_, next_time_data) = next_time_val
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        self.cache_last_time = next_time_data.to_vec();

        let next_channel_len_val = encoder_outputs
            .get("cache_last_channel_len_next")
            .ok_or_else(|| "Missing cache_last_channel_len_next".to_string())?;
        let (_, next_channel_len_data) = next_channel_len_val
            .try_extract_tensor::<i64>()
            .map_err(|e| e.to_string())?;
        self.cache_last_channel_len = next_channel_len_data.to_vec();

        // Get encoder output
        let encoder_out_val = encoder_outputs
            .get("outputs")
            .ok_or_else(|| "Missing outputs tensor from encoder".to_string())?;
        let (_, out_data) = encoder_out_val
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;

        let encoded_lengths_val = encoder_outputs
            .get("encoded_lengths")
            .ok_or_else(|| "Missing encoded_lengths".to_string())?;
        let (_, encoded_lengths_data) = encoded_lengths_val
            .try_extract_tensor::<i64>()
            .map_err(|e| e.to_string())?;
        let out_len = encoded_lengths_data[0] as usize;

        // Perform greedy RNN-T decoding
        let mut emitted_tokens = Vec::new();
        let max_symbols_per_step = 10;

        for t in 0..out_len {
            let frame_start = t * 1024;
            let encoder_frame = &out_data[frame_start..frame_start + 1024];

            let mut steps = 0;
            while steps < max_symbols_per_step {
                // Run joiner
                let joint_inputs = ort::inputs![
                    "encoder_output" => Value::from_array((vec![1, 1, 1024], encoder_frame.to_vec())).map_err(|e| e.to_string())?,
                    "decoder_output" => Value::from_array((vec![1, 1, 640], self.cached_decoder_output.clone())).map_err(|e| e.to_string())?,
                ];

                let joint_outputs = self
                    .joint_session
                    .run(joint_inputs)
                    .map_err(|e| format!("Joint run failed: {}", e))?;

                let joint_out_val = joint_outputs
                    .get("joint_output")
                    .ok_or_else(|| "Missing joint_output".to_string())?;
                let (_, joint_logits) = joint_out_val
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?;

                // Argmax over vocab dimension
                let mut max_idx = 0;
                let mut max_val = joint_logits[0];
                for (idx, &val) in joint_logits.iter().enumerate() {
                    if val > max_val {
                        max_val = val;
                        max_idx = idx;
                    }
                }

                let token_id = max_idx as i64;
                steps += 1;

                if token_id == self.blank_id {
                    break;
                }

                emitted_tokens.push(token_id as u32);
                self.last_decoder_token = token_id;

                // Run decoder session: input is target token ID (shape [1, 1])
                let decoder_inputs = ort::inputs![
                    "targets" => Value::from_array((vec![1, 1], vec![self.last_decoder_token])).map_err(|e| e.to_string())?,
                    "h_in" => Value::from_array((vec![2, 1, 640], self.decoder_hidden_state.clone())).map_err(|e| e.to_string())?,
                    "c_in" => Value::from_array((vec![2, 1, 640], self.decoder_cell_state.clone())).map_err(|e| e.to_string())?,
                ];

                let decoder_outputs = self
                    .decoder_session
                    .run(decoder_inputs)
                    .map_err(|e| format!("Decoder run failed: {}", e))?;

                // Update decoder LSTM states
                let h_out_val = decoder_outputs
                    .get("h_out")
                    .ok_or_else(|| "Missing h_out".to_string())?;
                let (_, h_out_data) = h_out_val
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?;
                self.decoder_hidden_state = h_out_data.to_vec();

                let c_out_val = decoder_outputs
                    .get("c_out")
                    .ok_or_else(|| "Missing c_out".to_string())?;
                let (_, c_out_data) = c_out_val
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?;
                self.decoder_cell_state = c_out_data.to_vec();

                // Get decoder output: shape [1, 640, 1] and cache it
                let decoder_out_val = decoder_outputs
                    .get("decoder_output")
                    .ok_or_else(|| "Missing decoder_output".to_string())?;
                let (_, decoder_out_data) = decoder_out_val
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?;
                self.cached_decoder_output = decoder_out_data.to_vec();
            }
        }

        Ok(emitted_tokens)
    }
}
