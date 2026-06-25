#![allow(dead_code)]

use ort::{session::Session, value::Value};
use std::path::{Path, PathBuf};

pub struct TtsEngine {
    model_path: PathBuf,
    session: Option<Session>,
    voice_data: Vec<f32>,
    last_active: std::time::Instant,
}

impl TtsEngine {
    pub fn new<P: AsRef<Path>>(model_path: P, voice_path: P) -> Result<Self, String> {
        let voice_bytes = std::fs::read(voice_path.as_ref()).map_err(|e| {
            format!(
                "Failed to read voice file at {:?}: {}",
                voice_path.as_ref(),
                e
            )
        })?;

        let len_rounded = (voice_bytes.len() / 4) * 4;
        let voice_bytes_aligned = &voice_bytes[..len_rounded];
        let voice_data = if voice_bytes_aligned.as_ptr() as usize % std::mem::align_of::<f32>() == 0 {
            bytemuck::cast_slice(voice_bytes_aligned).to_vec()
        } else {
            voice_bytes_aligned
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()
        };

        Ok(Self {
            model_path: model_path.as_ref().to_path_buf(),
            session: None,
            voice_data,
            last_active: std::time::Instant::now(),
        })
    }

    pub fn ensure_session(&mut self) -> Result<(), String> {
        self.last_active = std::time::Instant::now();
        if self.session.is_none() {
            if !self.model_path.exists() {
                return Err(format!(
                    "Kokoro ONNX model not found at {:?}",
                    self.model_path
                ));
            }
            let session = Session::builder()
                .map_err(|e| e.to_string())?
                .with_intra_threads(2)
                .map_err(|e| e.to_string())?
                .with_inter_threads(1)
                .map_err(|e| e.to_string())?
                .commit_from_file(&self.model_path)
                .map_err(|e| e.to_string())?;
            self.session = Some(session);
        }
        Ok(())
    }

    pub fn unload_session(&mut self) {
        self.session = None;
    }

    pub fn check_idle_unload(&mut self, idle_duration: std::time::Duration) {
        if self.session.is_some() && self.last_active.elapsed() >= idle_duration {
            self.unload_session();
        }
    }

    pub fn generate(&mut self, token_ids: &[i64], speed_val: f32) -> Result<Vec<f32>, String> {
        self.ensure_session()?;
        let session = self.session.as_mut().unwrap();

        let seq_len = token_ids.len();
        // Index = min(max(sequence_length - 2, 0), 509)
        let index = (seq_len.saturating_sub(2)).max(0).min(509);
        let offset = index * 256;
        if offset + 256 > self.voice_data.len() {
            return Err("Voice style offset out of bounds".to_string());
        }
        let style_slice = &self.voice_data[offset..offset + 256];

        let inputs = ort::inputs![
            "input_ids" => Value::from_array((vec![1, seq_len], token_ids.to_vec())).map_err(|e| e.to_string())?,
            "style" => Value::from_array((vec![1, 256], style_slice.to_vec())).map_err(|e| e.to_string())?,
            "speed" => Value::from_array((vec![1], vec![speed_val])).map_err(|e| e.to_string())?,
        ];

        let outputs = session
            .run(inputs)
            .map_err(|e| format!("Kokoro ONNX inference failed: {}", e))?;

        let waveform_val = outputs
            .get("waveform")
            .ok_or_else(|| "Missing waveform tensor in output".to_string())?;

        let (_, waveform_data) = waveform_val
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;

        Ok(waveform_data.to_vec())
    }
}
