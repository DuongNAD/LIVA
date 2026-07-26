use ort::{session::Session, value::Value};
use std::path::Path;

const ENCODER_FRAME_WIDTH: usize = 1024;
const DECODER_STATE_WIDTH: usize = 2 * 640;
const DECODER_OUTPUT_WIDTH: usize = 640;

struct DecoderBootstrap {
    hidden_state: Vec<f32>,
    cell_state: Vec<f32>,
    output: Vec<f32>,
}

fn validate_finite_tensor(name: &str, values: &[f32], expected_len: usize) -> Result<(), String> {
    if values.len() != expected_len {
        return Err(format!(
            "{name} has invalid length: expected {expected_len}, got {}",
            values.len()
        ));
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(format!("{name} contains a non-finite value"));
    }
    Ok(())
}

fn validated_decoder_bootstrap(
    hidden_state: &[f32],
    cell_state: &[f32],
    output: &[f32],
) -> Result<DecoderBootstrap, String> {
    validate_finite_tensor("decoder h_out", hidden_state, DECODER_STATE_WIDTH)?;
    validate_finite_tensor("decoder c_out", cell_state, DECODER_STATE_WIDTH)?;
    validate_finite_tensor("decoder_output", output, DECODER_OUTPUT_WIDTH)?;
    Ok(DecoderBootstrap {
        hidden_state: hidden_state.to_vec(),
        cell_state: cell_state.to_vec(),
        output: output.to_vec(),
    })
}

fn bootstrap_decoder(
    decoder_session: &mut Session,
    blank_id: i64,
) -> Result<DecoderBootstrap, String> {
    let zero_state = vec![0.0f32; DECODER_STATE_WIDTH];
    let decoder_inputs = ort::inputs![
        "targets" => Value::from_array((vec![1, 1], vec![blank_id]))
            .map_err(|e| format!("Failed to create decoder target value: {e}"))?,
        "h_in" => Value::from_array((vec![2, 1, 640], zero_state.clone()))
            .map_err(|e| format!("Failed to create decoder h_in value: {e}"))?,
        "c_in" => Value::from_array((vec![2, 1, 640], zero_state))
            .map_err(|e| format!("Failed to create decoder c_in value: {e}"))?,
    ];

    let decoder_outputs = decoder_session
        .run(decoder_inputs)
        .map_err(|e| format!("Decoder bootstrapping failed: {e}"))?;
    let (_, hidden_state) = decoder_outputs
        .get("h_out")
        .ok_or_else(|| "Missing h_out from decoder bootstrap".to_string())?
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("Failed to extract decoder h_out: {e}"))?;
    let (_, cell_state) = decoder_outputs
        .get("c_out")
        .ok_or_else(|| "Missing c_out from decoder bootstrap".to_string())?
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("Failed to extract decoder c_out: {e}"))?;
    let (_, output) = decoder_outputs
        .get("decoder_output")
        .ok_or_else(|| "Missing decoder_output from decoder bootstrap".to_string())?
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("Failed to extract decoder_output: {e}"))?;

    validated_decoder_bootstrap(hidden_state, cell_state, output)
}

fn checked_encoder_frame_count(
    encoded_lengths: &[i64],
    encoder_output_len: usize,
) -> Result<usize, String> {
    let raw_count = *encoded_lengths
        .first()
        .ok_or_else(|| "encoded_lengths tensor is empty".to_string())?;
    let frame_count = usize::try_from(raw_count)
        .map_err(|_| format!("encoded_lengths contains a negative value: {raw_count}"))?;
    let required_values = frame_count
        .checked_mul(ENCODER_FRAME_WIDTH)
        .ok_or_else(|| "encoded frame count overflows address space".to_string())?;
    if required_values > encoder_output_len {
        return Err(format!(
            "encoder output is too short: need {required_values} values for {frame_count} frames, got {encoder_output_len}"
        ));
    }
    Ok(frame_count)
}

fn checked_argmax(values: &[f32]) -> Result<usize, String> {
    let mut best = values
        .first()
        .copied()
        .ok_or_else(|| "joint_output tensor is empty".to_string())?;
    if !best.is_finite() {
        return Err("joint_output contains a non-finite value".to_string());
    }
    let mut best_index = 0;
    for (index, value) in values.iter().copied().enumerate().skip(1) {
        if !value.is_finite() {
            return Err("joint_output contains a non-finite value".to_string());
        }
        if value > best {
            best = value;
            best_index = index;
        }
    }
    Ok(best_index)
}

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
    initial_decoder_hidden_state: Vec<f32>,
    initial_decoder_cell_state: Vec<f32>,
    initial_cached_decoder_output: Vec<f32>,

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

        let mut decoder_session = Session::builder()
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

        let blank_id = 13087;
        let bootstrap = bootstrap_decoder(&mut decoder_session, blank_id)?;

        Ok(Self {
            encoder_session,
            decoder_session,
            joint_session,
            cache_last_channel: vec![0.0; 24 * 56 * 1024],
            cache_last_time: vec![0.0; 24 * 1024 * 8],
            cache_last_channel_len: vec![0; 1],
            decoder_hidden_state: bootstrap.hidden_state.clone(),
            decoder_cell_state: bootstrap.cell_state.clone(),
            last_decoder_token: blank_id,
            initial_decoder_hidden_state: bootstrap.hidden_state,
            initial_decoder_cell_state: bootstrap.cell_state,
            initial_cached_decoder_output: bootstrap.output.clone(),
            blank_id,
            lang_id: super::lang::DEFAULT_LANG_ID,
            cached_decoder_output: bootstrap.output,
        })
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

        self.decoder_hidden_state
            .clone_from(&self.initial_decoder_hidden_state);
        self.decoder_cell_state
            .clone_from(&self.initial_decoder_cell_state);
        self.cached_decoder_output
            .clone_from(&self.initial_cached_decoder_output);
        self.last_decoder_token = self.blank_id;
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
        let out_len = checked_encoder_frame_count(encoded_lengths_data, out_data.len())?;

        // Perform greedy RNN-T decoding
        let mut emitted_tokens = Vec::new();
        let max_symbols_per_step = 10;

        for t in 0..out_len {
            let frame_start = t * ENCODER_FRAME_WIDTH;
            let encoder_frame = &out_data[frame_start..frame_start + ENCODER_FRAME_WIDTH];

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
                let token_id = checked_argmax(joint_logits)? as i64;
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

#[cfg(test)]
mod tensor_validation_tests {
    use super::{checked_argmax, checked_encoder_frame_count, validated_decoder_bootstrap};

    #[test]
    fn encoder_length_rejects_missing_negative_and_oversized_values() {
        assert!(checked_encoder_frame_count(&[], 1024).is_err());
        assert!(checked_encoder_frame_count(&[-1], 1024).is_err());
        assert!(checked_encoder_frame_count(&[2], 1024).is_err());
        assert_eq!(checked_encoder_frame_count(&[2], 2048).unwrap(), 2);
    }

    #[test]
    fn argmax_rejects_empty_logits() {
        assert!(checked_argmax(&[]).is_err());
        assert!(checked_argmax(&[f32::NAN, 1.0]).is_err());
        assert_eq!(checked_argmax(&[-3.0, 2.0, 1.0]).unwrap(), 1);
    }

    #[test]
    fn decoder_bootstrap_rejects_invalid_tensor_contract() {
        let valid_state = vec![0.0; 2 * 640];
        let valid_output = vec![0.0; 640];

        assert!(validated_decoder_bootstrap(&valid_state, &valid_state, &valid_output).is_ok());
        assert!(
            validated_decoder_bootstrap(&valid_state[..1279], &valid_state, &valid_output).is_err()
        );
        assert!(
            validated_decoder_bootstrap(&valid_state, &valid_state[..1279], &valid_output).is_err()
        );
        assert!(
            validated_decoder_bootstrap(&valid_state, &valid_state, &valid_output[..639]).is_err()
        );

        let mut non_finite = valid_output.clone();
        non_finite[0] = f32::NAN;
        assert!(validated_decoder_bootstrap(&valid_state, &valid_state, &non_finite).is_err());
    }
}
