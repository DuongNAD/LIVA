#[path = "../g2p.rs"]
mod g2p;
#[path = "../tokenizer.rs"]
mod tokenizer;

use ort::{session::Session, value::Value};
use std::env;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::time::Instant;
use std::path::PathBuf;

fn create_wav_header(num_samples: usize) -> [u8; 44] {
    let sample_rate = 24000u32;
    let num_channels = 1u16;
    let bits_per_sample = 16u16;
    
    let subchunk2_size = (num_samples * num_channels as usize * (bits_per_sample as usize / 8)) as u32;
    let chunk_size = 36 + subchunk2_size;
    let byte_rate = sample_rate * num_channels as u32 * (bits_per_sample as u32 / 8);
    let block_align = num_channels * (bits_per_sample / 8);

    let mut header = [0u8; 44];
    
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&chunk_size.to_le_bytes());
    header[8..12].copy_from_slice(b"WAVE");
    header[12..16].copy_from_slice(b"fmt ");
    header[16..20].copy_from_slice(&16u32.to_le_bytes());
    header[20..22].copy_from_slice(&1u16.to_le_bytes());
    header[22..24].copy_from_slice(&num_channels.to_le_bytes());
    header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    header[32..34].copy_from_slice(&block_align.to_le_bytes());
    header[34..36].copy_from_slice(&bits_per_sample.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&subchunk2_size.to_le_bytes());

    header
}

fn resolve_poc_dir() -> PathBuf {
    if let Ok(val) = env::var("OMNIVOICE_POC_DIR") {
        return PathBuf::from(val);
    }

    if let Ok(cwd) = env::current_dir() {
        let mut dir = cwd.clone();
        loop {
            if dir.join("export_onnx.py").exists() {
                return dir;
            }
            if !dir.pop() {
                break;
            }
        }
    }

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let mut dir = exe_dir.to_path_buf();
            loop {
                if dir.join("export_onnx.py").exists() {
                    return dir;
                }
                if !dir.pop() {
                    break;
                }
            }
        }
    }

    PathBuf::from(".")
}

fn find_python_interpreter() -> String {
    let interpreters = ["python", "python3", "py"];
    for py in interpreters {
        if let Ok(output) = Command::new(py).arg("--version").output() {
            if output.status.success() {
                return py.to_string();
            }
        }
    }
    "python".to_string()
}

fn split_text(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut char_count = 0;

    for c in text.chars() {
        current_chunk.push(c);
        char_count += 1;
        if c == '.' || c == '!' || c == '?' {
            let trimmed = current_chunk.trim();
            if !trimmed.is_empty() {
                chunks.push(trimmed.to_string());
            }
            current_chunk.clear();
            char_count = 0;
        } else if char_count >= 400 {
            if let Some(last_space_idx) = current_chunk.rfind(' ') {
                let (left, right) = current_chunk.split_at(last_space_idx);
                let trimmed_left = left.trim();
                if !trimmed_left.is_empty() {
                    chunks.push(trimmed_left.to_string());
                }
                let remainder = right.trim_start();
                char_count = remainder.chars().count();
                current_chunk = remainder.to_string();
            } else {
                let trimmed = current_chunk.trim();
                if !trimmed.is_empty() {
                    chunks.push(trimmed.to_string());
                }
                current_chunk.clear();
                char_count = 0;
            }
        }
    }

    let trimmed = current_chunk.trim();
    if !trimmed.is_empty() {
        chunks.push(trimmed.to_string());
    }

    if chunks.is_empty() {
        chunks.push(" ".to_string());
    }

    chunks
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut text_opt = None;
    let mut reference_opt = None;
    let mut output_opt = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--text" => text_opt = args.next(),
            "--reference" => reference_opt = args.next(),
            "--output" => output_opt = args.next(),
            _ => {
                eprintln!("Unknown argument: {}", arg);
                std::process::exit(1);
            }
        }
    }

    let poc_dir = resolve_poc_dir();

    let text = text_opt.unwrap_or_else(|| "Hello world.".to_string());
    let reference = reference_opt.unwrap_or_else(|| {
        poc_dir.join("..").join("..").join("models").join("asr_example.wav")
            .to_string_lossy().into_owned()
    });
    let output = output_opt.unwrap_or_else(|| {
        poc_dir.join("rust_cli").join("output.wav")
            .to_string_lossy().into_owned()
    });

    let total_start = Instant::now();

    // 1. Subprocess Invocation Phase
    println!("DBG: Subprocess start");
    let sub_start = Instant::now();

    let export_script_path = poc_dir.join("export_onnx.py");
    let model_path = poc_dir.join("models").join("model.onnx");
    let voices_dir = poc_dir.join("voices");

    let pid = std::process::id();
    let voice_name = format!("temp_voice_{}", pid);
    let voice_filename = format!("{}.bin", voice_name);
    let voice_path = voices_dir.join(&voice_filename);

    let python_exe = find_python_interpreter();

    let status = Command::new(&python_exe)
        .args([
            export_script_path.to_str().unwrap(),
            "--reference",
            &reference,
            "--voice-name",
            &voice_name,
        ])
        .status()?;
    let sub_dur = sub_start.elapsed();
    println!("DBG: Subprocess done, status: {:?}", status);

    if !status.success() {
        return Err(format!("export_onnx.py subprocess failed with status: {:?}", status.code()).into());
    }

    // 2. Initial Loading & Setup Phase
    let setup_start = Instant::now();
    
    println!("DBG: Reading voice file");
    let voice_bytes = std::fs::read(&voice_path)?;
    // Delete the unique file immediately after reading
    let _ = std::fs::remove_file(&voice_path);
    println!("DBG: Voice file read, len = {}", voice_bytes.len());
    
    let len_rounded = (voice_bytes.len() / 4) * 4;
    let voice_bytes_aligned = &voice_bytes[..len_rounded];
    let voice_data: Vec<f32> = if voice_bytes_aligned.as_ptr() as usize % std::mem::align_of::<f32>() == 0 {
        bytemuck::cast_slice(voice_bytes_aligned).to_vec()
    } else {
        voice_bytes_aligned
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    };

    println!("DBG: Loading ORT Model");
    let mut session = Session::builder()?
        .with_intra_threads(2)?
        .with_inter_threads(1)?
        .commit_from_file(&model_path)?;
        
    let setup_dur = setup_start.elapsed();
    println!("DBG: Setup done");

    // 3. ONNX Inference Execution Phase
    println!("DBG: Running inference");
    let infer_start = Instant::now();

    // Split text into chunks to prevent sequence limit crashes
    let chunks = split_text(&text);
    let mut combined_waveform = Vec::new();
    let tokenizer = tokenizer::TtsTokenizer::new();

    for (_i, chunk) in chunks.iter().enumerate() {
        let phonemes = g2p::G2p::phonemize(chunk);
        let token_ids = tokenizer.tokenize(&phonemes);
        let seq_len = token_ids.len();

        let index = (seq_len.saturating_sub(2)).min(509);
        let offset = index * 256;
        if offset + 256 > voice_data.len() {
            return Err(format!(
                "Voice style offset {} out of bounds (voice data length: {})",
                offset + 256,
                voice_data.len()
            ).into());
        }
        let style_slice = &voice_data[offset..offset + 256];

        let inputs = ort::inputs![
            "input_ids" => Value::from_array((vec![1, seq_len], token_ids.to_vec()))?,
            "style" => Value::from_array((vec![1, 256], style_slice.to_vec()))?,
            "speed" => Value::from_array((vec![1], vec![1.0f32]))?,
        ];

        let outputs = session.run(inputs)?;
        let waveform_val = outputs
            .get("waveform")
            .ok_or("Missing waveform tensor in output")?;

        let (_, waveform_data) = waveform_val.try_extract_tensor::<f32>()?;
        let waveform_vec = waveform_data.to_vec();
        combined_waveform.extend(waveform_vec);
    }

    let infer_dur = infer_start.elapsed();
    println!("DBG: Inference done");

    // 4. Audio Conversion & File Output Phase
    println!("DBG: Writing audio");
    let output_start = Instant::now();
    let pcm_data: Vec<i16> = combined_waveform
        .iter()
        .map(|&val| {
            let clamped = val.clamp(-1.0, 1.0);
            (clamped * 32767.0) as i16
        })
        .collect();

    let mut out_file = File::create(&output)?;
    let header = create_wav_header(pcm_data.len());
    out_file.write_all(&header)?;

    let mut pcm_bytes = Vec::with_capacity(pcm_data.len() * 2);
    for sample in pcm_data {
        pcm_bytes.extend_from_slice(&sample.to_le_bytes());
    }
    out_file.write_all(&pcm_bytes)?;
    let output_dur = output_start.elapsed();
    println!("DBG: Writing done");

    let total_dur = total_start.elapsed();

    // Print timing results to stdout in structured format
    println!("METRIC:SUBPROCESS_MS:{}", sub_dur.as_millis());
    println!("METRIC:SETUP_MS:{}", setup_dur.as_millis());
    println!("METRIC:INFERENCE_MS:{}", infer_dur.as_millis());
    println!("METRIC:OUTPUT_MS:{}", output_dur.as_millis());
    println!("METRIC:TOTAL_MS:{}", total_dur.as_millis());

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error occurred: {:?}", e);
        std::process::exit(1);
    }
}
